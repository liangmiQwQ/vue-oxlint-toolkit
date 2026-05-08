#![deny(clippy::all)]

use oxc_ast::ast::CommentKind;
use oxc_estree::{CompactJSSerializer, CompactTSSerializer, ESTree};
use vue_oxlint_jsx::VueJsxCodegen;
use vue_oxlint_parser::VueParser;

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
  pub virtual_start: u32,
  pub virtual_end: u32,
  pub original_start: u32,
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
  pub mappings: Vec<NativeMapping>,
}

#[napi(object)]
pub struct NativeParseResult {
  pub ast_json: String,
  pub errors: Vec<NativeDiagnostic>,
  pub panicked: bool,
}

#[napi]
#[must_use]
#[allow(clippy::needless_pass_by_value, reason = "N-API owns string arguments at the boundary.")]
pub fn parse_vue(source: String) -> NativeParseResult {
  let vue_allocator = oxc_allocator::Allocator::new();
  let js_allocator = oxc_allocator::Allocator::new();
  let ret = VueParser::new(&vue_allocator, &js_allocator, &source).parse();
  let ast_json = if ret.sfc.source_type.is_some_and(oxc_ast::ast::SourceType::is_typescript) {
    let mut serializer = CompactTSSerializer::new(true);
    ret.sfc.serialize(&mut serializer);
    serializer.into_string()
  } else {
    let mut serializer = CompactJSSerializer::new(true);
    ret.sfc.serialize(&mut serializer);
    serializer.into_string()
  };

  NativeParseResult {
    ast_json,
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
    panicked: ret.panicked,
  }
}

#[napi]
#[must_use]
#[allow(clippy::needless_pass_by_value, reason = "N-API owns string arguments at the boundary.")]
pub fn transform_jsx(source: String) -> NativeTransformResult {
  let ret = VueJsxCodegen::new(&source).build();
  let script_kind = if ret.source_type.is_typescript() { "tsx" } else { "jsx" }.to_string();

  NativeTransformResult {
    source_text: ret.source_text,
    script_kind,
    comments: ret
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
    mappings: ret
      .mappings
      .iter()
      .map(|mapping| NativeMapping {
        virtual_start: mapping.codegen_span.start,
        virtual_end: mapping.codegen_span.end,
        original_start: mapping.original_span.start,
        original_end: mapping.original_span.end,
      })
      .collect(),
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
