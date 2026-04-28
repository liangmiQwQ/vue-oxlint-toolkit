//! Top-level parser entry point.
//!
//! Orchestrates the lexer-driven SFC scan, runs `oxc_parser` over each
//! `<script>` body in a separate allocator, and serializes the combined
//! result to JSON for the napi binding.

use oxc_allocator::{Allocator, Box as ArenaBox};
use oxc_ast::ast::Program;
use oxc_diagnostics::OxcDiagnostic;
use oxc_estree::{CompactTSSerializer, ESTree};
use oxc_parser::{Parser as JsParser, ParserReturn};
use oxc_span::SourceType;
use serde::Serialize;
use serde_json::value::RawValue;

use crate::ast::{Span, VAttributeKey, VAttributeValue, VDocumentFragment};

mod attr;
mod sfc;
mod template;

use crate::lexer::Lexer;
use crate::token::{LexMode, Token, TokenKind};

/// Drive the lexer (which must already be in `InTag` mode) until the start
/// tag closes with `>` or `/>`. Returns the captured attribute list and
/// the closing token. Handles `key`, `key=`, `key=value`, `key="v"`, and
/// `key='v'`, with the `attr_end` field tracking how far each attribute
/// span reaches into the source.
pub(crate) fn read_start_tag_attrs<'a>(
  lexer: &mut Lexer<'a>,
) -> (Vec<attr::AttrTok<'a>>, Token<'a>) {
  let mut out: Vec<attr::AttrTok<'a>> = Vec::new();
  let mut state: AttrState<'a> = AttrState::Idle;
  loop {
    let t = lexer.next();
    match t.kind {
      TokenKind::TagEnd | TokenKind::TagSelfClose | TokenKind::Eof => {
        flush(&mut state, &mut out);
        return (out, t);
      }
      TokenKind::AttrName { name } => match state {
        AttrState::Idle => {
          state = AttrState::HaveKey { span: t.span, name, attr_end: t.span.end };
        }
        AttrState::HaveKey { span, name: prev, attr_end } => {
          out.push(attr::AttrTok { key_span: span, key: prev, value: None, attr_end });
          state = AttrState::HaveKey { span: t.span, name, attr_end: t.span.end };
        }
        AttrState::AfterEq { span, name: prev, attr_end: _ } => {
          // `key= value` — this AttrName is the unquoted value of `key`.
          out.push(attr::AttrTok {
            key_span: span,
            key: prev,
            value: Some(attr::AttrValue {
              outer_span: t.span,
              inner_span: t.span,
              text: name,
              quoted: false,
            }),
            attr_end: t.span.end,
          });
          state = AttrState::Idle;
        }
      },
      TokenKind::AttrEq => match state {
        AttrState::HaveKey { span, name, attr_end: _ } => {
          state = AttrState::AfterEq { span, name, attr_end: t.span.end };
          // Lex the upcoming unquoted value as one chunk so embedded
          // `=` in `foo=abc=def` stays inside the value text.
          lexer.set_mode(LexMode::AttrValueUnquoted);
        }
        // Stray `=` with no preceding key, or `==` — fold into trailing
        // span of the current build, if any.
        AttrState::AfterEq { span, name, .. } => {
          state = AttrState::AfterEq { span, name, attr_end: t.span.end };
        }
        AttrState::Idle => {}
      },
      TokenKind::AttrValue { value, quote, inner_span } => {
        if let AttrState::AfterEq { span, name, .. } = state {
          out.push(attr::AttrTok {
            key_span: span,
            key: name,
            value: Some(attr::AttrValue {
              outer_span: t.span,
              inner_span,
              text: value,
              quoted: quote.is_some(),
            }),
            attr_end: t.span.end,
          });
          state = AttrState::Idle;
          lexer.set_mode(LexMode::InTag);
        } else {
          // Bare quoted value with no preceding `key=` — record as a
          // key-only attribute so source coverage doesn't drop bytes.
          flush(&mut state, &mut out);
          out.push(attr::AttrTok {
            key_span: t.span,
            key: value,
            value: None,
            attr_end: t.span.end,
          });
        }
      }
      _ => {} // defensive: nothing else in InTag mode
    }
  }
}

enum AttrState<'a> {
  Idle,
  HaveKey { span: crate::ast::Span, name: &'a str, attr_end: u32 },
  AfterEq { span: crate::ast::Span, name: &'a str, attr_end: u32 },
}

fn flush<'a>(state: &mut AttrState<'a>, out: &mut Vec<attr::AttrTok<'a>>) {
  match std::mem::replace(state, AttrState::Idle) {
    AttrState::Idle => {}
    AttrState::HaveKey { span, name, attr_end } | AttrState::AfterEq { span, name, attr_end } => {
      out.push(attr::AttrTok { key_span: span, key: name, value: None, attr_end });
    }
  }
}

#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
  /// Override the source type used for `<script>` parsing. When `None` we
  /// detect TypeScript via `lang="ts"` / `lang="tsx"`.
  pub default_source_type: Option<SourceType>,
}

/// Parsed SFC living in two allocators.
///
/// `document` borrows from `vue_alloc`; `script_program` (when present)
/// borrows from `js_alloc`.
pub struct ParsedSfc<'v, 'j> {
  pub document: ArenaBox<'v, VDocumentFragment<'v>>,
  pub scripts: Vec<ScriptProgram<'j>>,
  pub diagnostics: Vec<String>,
}

pub struct ScriptProgram<'j> {
  pub tag: String,
  pub setup: bool,
  pub lang: Option<String>,
  pub content_range: Span,
  pub program: Program<'j>,
  pub errors: Vec<String>,
}

/// Parse an SFC into a [`ParsedSfc`].
///
/// # Errors
/// Returns an [`OxcDiagnostic`] for unrecoverable scan failures.
pub fn parse<'v, 'j>(
  vue_alloc: &'v Allocator,
  js_alloc: &'j Allocator,
  source: &'v str,
  opts: &ParseOptions,
) -> Result<ParsedSfc<'v, 'j>, OxcDiagnostic>
where
  'v: 'j,
{
  let layout = sfc::parse_sfc(vue_alloc, source);

  // Pre-scan: pick the source type for template expressions from the first
  // <script lang="..."> block (matches upstream vue-eslint-parser).
  let template_source_type = detect_template_source_type(&layout.blocks);

  // Build the document tree from the layout, parsing the <template> body
  // and storing inert text bodies for other blocks.
  let document = sfc::build_document(vue_alloc, source, &layout, template_source_type);

  // Parse each <script> block's body via oxc_parser.
  let scripts = layout
    .blocks
    .iter()
    .filter(|b| b.tag.eq_ignore_ascii_case("script") && !b.self_closing)
    .map(|block| parse_script_block(vue_alloc, js_alloc, source, block, opts))
    .collect();

  Ok(ParsedSfc { document, scripts, diagnostics: Vec::new() })
}

/// Convenience helper: create both allocators, parse, and serialize the
/// result to JSON.
///
/// # Errors
/// Returns the underlying [`OxcDiagnostic`] from [`parse`].
///
/// # Panics
/// Does not panic in practice: the inner `RawValue::from_string("null")`
/// fallback uses a known-valid JSON literal.
pub fn parse_to_json(source: &str, opts: &ParseOptions) -> Result<String, OxcDiagnostic> {
  let vue_alloc = Allocator::default();
  let js_alloc = Allocator::default();
  let parsed = parse(&vue_alloc, &js_alloc, source, opts)?;

  let scripts_json: Vec<ScriptJson<'_>> = parsed
    .scripts
    .iter()
    .map(|s| {
      let mut ser = CompactTSSerializer::new(true);
      s.program.serialize(&mut ser);
      let body = ser.into_string();
      ScriptJson {
        tag: &s.tag,
        setup: s.setup,
        lang: s.lang.as_deref(),
        content_range: s.content_range,
        errors: &s.errors,
        program: RawValue::from_string(body)
          .unwrap_or_else(|_| RawValue::from_string("null".into()).unwrap()),
      }
    })
    .collect();

  let payload = SfcJson { document: &parsed.document, scripts: &scripts_json };
  Ok(serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string()))
}

fn detect_template_source_type(layout: &[sfc::LayoutBlock<'_>]) -> SourceType {
  let default = SourceType::default().with_module(true).with_typescript(true);
  for block in layout {
    if !block.tag.eq_ignore_ascii_case("script") || block.self_closing {
      continue;
    }
    let lang = block.attrs.iter().find_map(|a| {
      if a.key.eq_ignore_ascii_case("lang") { a.value.as_ref().map(|v| v.text) } else { None }
    });
    let mut st = SourceType::default().with_module(true);
    match lang {
      Some(l) if l.eq_ignore_ascii_case("ts") => st = st.with_typescript(true),
      Some(l) if l.eq_ignore_ascii_case("tsx") => {
        st = st.with_typescript(true).with_jsx(true);
      }
      Some(l) if l.eq_ignore_ascii_case("jsx") => st = st.with_jsx(true),
      _ => {}
    }
    return st;
  }
  default
}

fn parse_script_block<'v, 'j>(
  vue_alloc: &'v Allocator,
  js_alloc: &'j Allocator,
  source: &'v str,
  block: &sfc::LayoutBlock<'v>,
  opts: &ParseOptions,
) -> ScriptProgram<'j>
where
  'v: 'j,
{
  let template_source_type = SourceType::default().with_module(true).with_typescript(true);
  let attrs = attr::build_vattributes(
    vue_alloc,
    &block.attrs,
    /* in_v_pre */ false,
    template_source_type,
  );

  let mut setup = false;
  let mut lang: Option<String> = None;
  for a in &attrs {
    if let VAttributeKey::Identifier(id) = &*a.key {
      if id.name.eq_ignore_ascii_case("setup") {
        setup = true;
      } else if id.name.eq_ignore_ascii_case("lang")
        && let Some(v) = &a.value
        && let VAttributeValue::Literal(lit) = &**v
      {
        lang = Some(lit.value.to_string());
      }
    }
  }
  let source_type = opts.default_source_type.unwrap_or_else(|| {
    let mut st = SourceType::default().with_module(true);
    if let Some(l) = lang.as_deref() {
      if l.eq_ignore_ascii_case("ts") {
        st = st.with_typescript(true);
      } else if l.eq_ignore_ascii_case("tsx") {
        st = st.with_typescript(true).with_jsx(true);
      } else if l.eq_ignore_ascii_case("jsx") {
        st = st.with_jsx(true);
      }
    }
    st
  });
  let body = &source[block.content_range.start as usize..block.content_range.end as usize];
  let ParserReturn { program, errors, .. } = JsParser::new(js_alloc, body, source_type).parse();
  ScriptProgram {
    tag: block.tag.to_string(),
    setup,
    lang,
    content_range: block.content_range,
    program,
    errors: errors.into_iter().map(|e| e.to_string()).collect(),
  }
}

#[derive(Serialize)]
struct SfcJson<'a> {
  document: &'a VDocumentFragment<'a>,
  scripts: &'a [ScriptJson<'a>],
}

#[derive(Serialize)]
struct ScriptJson<'a> {
  tag: &'a str,
  setup: bool,
  lang: Option<&'a str>,
  content_range: Span,
  errors: &'a [String],
  program: Box<RawValue>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn basic_sfc_round_trip() {
    let src =
      "<template><div class=\"a\">hi {{ x }}</div></template>\n<script setup>let x = 1</script>\n";
    let json = parse_to_json(src, &ParseOptions::default()).unwrap();
    assert!(json.contains("\"VDocumentFragment\""));
    assert!(json.contains("\"VElement\""));
    assert!(json.contains("\"VExpressionContainer\""));
    assert!(json.contains("\"setup\":true"));
    assert!(json.contains("\"Program\""));
  }
}
