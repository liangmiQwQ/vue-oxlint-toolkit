//! Top-level entry point: split an SFC, build the V* AST, parse `<script>`
//! bodies via `oxc_parser` into a *separate* allocator, and serialise
//! everything to a single JSON string for transport.
//!
//! ## Allocator split
//!
//! Two allocators are passed in (or constructed by `parse_to_json`):
//!
//! * `vue_alloc` — owns all `V*` nodes (template AST).
//! * `js_alloc`  — owns the `oxc_ast::Program` produced from each
//!   `<script>` block. Keeping it separate means a future
//!   `vue_oxlint_jsx` migration can swap that allocator (or its lifetime)
//!   without touching the Vue side.

use oxc_allocator::{Allocator, Box as ArenaBox, Vec as ArenaVec};
use oxc_ast::ast::Program;
use oxc_estree::{CompactTSSerializer, ESTree};
use oxc_parser::{Parser as JsParser, ParserReturn};
use oxc_span::SourceType;
use serde::Serialize;
use serde_json::value::RawValue;

use crate::ast::{Span, VDocumentFragment, VElementChild, VRootChild, VText};
use crate::sfc::{SfcError, split};
use crate::template::{build_block_element, parse_attributes, parse_template_body};

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
  pub scripts: Vec<ParsedScript<'j>>,
  pub diagnostics: Vec<String>,
}

pub struct ParsedScript<'j> {
  /// Tag name (`script` or `script setup`-tagged via attribute).
  pub tag: String,
  /// `true` when the original block had a `setup` attribute.
  pub setup: bool,
  /// `lang="..."` attribute value if present.
  pub lang: Option<String>,
  /// Inner content range within the original SFC source.
  pub content_range: Span,
  pub program: Program<'j>,
  pub errors: Vec<String>,
}

/// Parse an SFC source string into a [`ParsedSfc`].
///
/// # Errors
/// Returns a [`SfcError`] when the top-level block layout is malformed.
pub fn parse<'v, 'j>(
  vue_alloc: &'v Allocator,
  js_alloc: &'j Allocator,
  source: &'v str,
  opts: &ParseOptions,
) -> Result<ParsedSfc<'v, 'j>, SfcError>
where
  'v: 'j,
{
  let layout = split(source)?;

  let mut root_children: ArenaVec<'v, VRootChild<'v>> = ArenaVec::new_in(vue_alloc);

  // Top-level text segments (whitespace etc.) are preserved as VText nodes
  // so byte offsets in the document fragment line up with the source.
  let mut block_iter = layout.blocks.iter().peekable();
  let mut text_iter = layout.text_segments.iter().peekable();

  let mut next_offset = 0u32;
  let total_len = source.len() as u32;
  while next_offset < total_len {
    // Consume any text segment starting at `next_offset`.
    if let Some(&(span, txt)) = text_iter.peek().copied()
      && span.start == next_offset
    {
      let node = ArenaBox::new_in(VText { r#type: "VText", range: span, value: txt }, vue_alloc);
      root_children.push(VRootChild::Text(node));
      next_offset = span.end;
      text_iter.next();
      continue;
    }
    if let Some(block) = block_iter.peek()
      && block.range.start == next_offset
    {
      let block = block_iter.next().unwrap();
      let inner_children = if block.tag.eq_ignore_ascii_case("template") && !block.self_closing {
        let body = &source[block.content_range.start as usize..block.content_range.end as usize];
        parse_template_body(vue_alloc, body, block.content_range.start)
      } else {
        // Treat <script>/<style>/custom blocks as opaque text (their content
        // is not parsed as HTML; <script> goes through oxc_parser below).
        let mut v: ArenaVec<'v, VElementChild<'v>> = ArenaVec::new_in(vue_alloc);
        if !block.self_closing && block.content_range.end > block.content_range.start {
          let txt = &source[block.content_range.start as usize..block.content_range.end as usize];
          let n = ArenaBox::new_in(
            VText { r#type: "VText", range: block.content_range, value: txt },
            vue_alloc,
          );
          v.push(VElementChild::Text(n));
        }
        v
      };
      let raw_attrs_offset =
        source[..block.start_tag_range.start as usize].len() as u32 + 1 + block.tag.len() as u32;
      let element = build_block_element(
        vue_alloc,
        source,
        block.tag,
        block.range,
        block.start_tag_range,
        block.end_tag_range,
        block.raw_attributes,
        raw_attrs_offset,
        block.self_closing,
        inner_children,
      );
      root_children.push(VRootChild::Element(element));
      next_offset = block.range.end;
      continue;
    }
    // Nothing matched at this offset — should not happen if split() is
    // correct, but guard against advancing infinitely.
    break;
  }

  let document =
    ArenaBox::new_in(VDocumentFragment::new(Span::new(0, total_len), root_children), vue_alloc);

  // Parse <script> blocks via oxc_parser into the JS allocator.
  let mut scripts: Vec<ParsedScript<'j>> = Vec::new();
  for block in &layout.blocks {
    if !block.tag.eq_ignore_ascii_case("script") || block.self_closing {
      continue;
    }
    let attrs = parse_attributes(
      vue_alloc,
      source,
      block.raw_attributes,
      0, // we only inspect text/keys here, span isn't used
    );
    let mut setup = false;
    let mut lang: Option<String> = None;
    for a in &attrs {
      if let crate::ast::VAttributeKey::Identifier(id) = &*a.key {
        if id.name.eq_ignore_ascii_case("setup") {
          setup = true;
        } else if id.name.eq_ignore_ascii_case("lang")
          && let Some(v) = &a.value
          && let crate::ast::VAttributeValue::Literal(lit) = &**v
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
    scripts.push(ParsedScript {
      tag: block.tag.to_string(),
      setup,
      lang,
      content_range: block.content_range,
      program,
      errors: errors.into_iter().map(|e| e.to_string()).collect(),
    });
  }

  Ok(ParsedSfc { document, scripts, diagnostics: Vec::new() })
}

/// Convenience helper: create both allocators internally and return a JSON
/// string with the V AST plus serialised script programs.
///
/// # Errors
/// Returns the underlying [`SfcError`] from [`parse`] when block splitting
/// fails. JSON serialisation is infallible for our owned types.
pub fn parse_to_json(source: &str, opts: &ParseOptions) -> Result<String, SfcError> {
  let vue_alloc = Allocator::default();
  let js_alloc = Allocator::default();
  let parsed = parse(&vue_alloc, &js_alloc, source, opts)?;

  // Serialise each script Program through oxc_estree (its native JSON form),
  // then wrap as a serde_json `RawValue` so we can splice it into our serde
  // tree without re-parsing.
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
          .unwrap_or_else(|_| RawValue::from_string("null".to_string()).unwrap()),
      }
    })
    .collect();

  let payload = SfcJson { document: &parsed.document, scripts: &scripts_json };
  Ok(serde_json::to_string(&payload).unwrap_or_else(|_| "null".to_string()))
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
  /// Raw, already-serialised oxc_estree JSON.
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
    // oxc_ast Program serialises with a `type` field.
    assert!(json.contains("\"Program\""));
  }
}
