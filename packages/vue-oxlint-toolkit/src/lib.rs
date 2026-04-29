#![deny(clippy::all)]

mod codegen;

use codegen::{Codegen, CodegenHook};
use oxc_allocator::Allocator;
use oxc_ast::ast::CommentKind;
use oxc_span::Span;
use vue_oxlint_jsx::{ParseConfig, VueOxcParser};

use napi_derive::napi;

#[napi(object)]
pub struct NativeRange {
  pub start: u32,
  pub end: u32,
}

#[napi(object)]
pub struct NativeComment {
  #[napi(ts_type = "'Line' | 'Block'")]
  pub r#type: String,
  pub value: String,
  pub start: u32,
  pub end: u32,
}

#[napi(object)]
pub struct NativeDiagnostic {
  pub message: String,
  pub start: u32,
  pub end: u32,
}

#[napi(object)]
pub struct NativeMapping {
  /// AST node type at this mapping point.
  pub r#type: String,
  /// Byte offset in the generated source where this node starts.
  pub virtual_start: u32,
  /// Byte offset in the generated source where this node ends.
  pub virtual_end: u32,
  /// Byte offset in the original source where this node starts.
  pub original_start: u32,
  /// Byte offset in the original source where this node ends.
  pub original_end: u32,
}

#[napi(object)]
pub struct NativeTransformResult {
  pub source_text: String,
  #[napi(ts_type = "'jsx' | 'tsx'")]
  pub script_kind: String,
  pub comments: Vec<NativeComment>,
  pub irregular_whitespaces: Vec<NativeRange>,
  pub errors: Vec<NativeDiagnostic>,
  /// One entry per AST node with a non-zero span. Synthesised wrapper nodes
  /// (`span: 0,0`) are skipped.
  pub mappings: Vec<NativeMapping>,
}

struct MappingCollector {
  out: Vec<NativeMapping>,
}

impl CodegenHook for MappingCollector {
  fn record(&mut self, kind: &'static str, span: Span, virtual_start: u32, virtual_end: u32) {
    self.out.push(NativeMapping {
      r#type: kind.to_string(),
      virtual_start,
      virtual_end,
      original_start: span.start,
      original_end: span.end,
    });
  }
}

#[napi]
#[must_use]
#[allow(clippy::needless_pass_by_value, reason = "N-API owns string arguments at the boundary.")]
pub fn transform_jsx(source: String) -> NativeTransformResult {
  let allocator = Allocator::default();
  let ret =
    VueOxcParser::new(&allocator, &source).with_config(ParseConfig { codegen: true }).parse();
  let script_kind = if ret.program.source_type.is_typescript() { "tsx" } else { "jsx" }.to_string();

  let (source_text, mappings) = if ret.panicked {
    (String::new(), Vec::new())
  } else {
    let codegen = Codegen::new(&source, MappingCollector { out: Vec::new() });
    let (text, hook) = codegen.build(&ret.program);
    (text, hook.out)
  };

  NativeTransformResult {
    source_text,
    script_kind,
    comments: ret
      .program
      .comments
      .iter()
      .map(|comment| {
        let comment_data =
          comment_data(&source, comment.kind, comment.span.start, comment.span.end);

        NativeComment {
          r#type: match comment.kind {
            CommentKind::Line => "Line",
            CommentKind::SingleLineBlock | CommentKind::MultiLineBlock => "Block",
          }
          .to_string(),
          value: comment_data.value.to_string(),
          start: comment_data.start,
          end: comment_data.end,
        }
      })
      .collect(),
    irregular_whitespaces: ret
      .irregular_whitespaces
      .iter()
      .map(|span| NativeRange { start: span.start, end: span.end })
      .collect(),
    errors: ret
      .errors
      .iter()
      .map(|error| {
        let (start, end) =
          error.labels.as_ref().and_then(|labels| labels.first()).map_or((0, 0), |label| {
            let start = label.offset() as u32;
            let end = start + label.len() as u32;
            (start, end)
          });

        NativeDiagnostic { message: error.message.to_string(), start, end }
      })
      .collect(),
    mappings,
  }
}

struct CommentData<'a> {
  value: &'a str,
  start: u32,
  end: u32,
}

fn comment_data(source: &str, kind: CommentKind, start: u32, end: u32) -> CommentData<'_> {
  let start = start as usize;
  let end = end as usize;

  if kind == CommentKind::Line {
    let value_start = start + 2;
    let end = line_comment_end(source, value_start);

    return CommentData {
      value: source.get(value_start..end).unwrap_or_default(),
      start: start as u32,
      end: end as u32,
    };
  }

  CommentData {
    value: source.get(start..end).unwrap_or_default(),
    start: start as u32,
    end: end as u32,
  }
}

fn line_comment_end(source: &str, value_start: usize) -> usize {
  source[value_start..].find('\n').map_or(source.len(), |newline| value_start + newline)
}
