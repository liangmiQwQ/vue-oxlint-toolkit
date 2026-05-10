mod comments;

use napi_derive::napi;
use vue_oxlint_jsx::VueJsxCodegen;

use crate::{
  diagnostics::native_diagnostic, source_text::SourceOffsets, transform::comments::native_comment,
};

#[napi]
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn transform_jsx(source: String) -> NativeTransformResult {
  let ret = VueJsxCodegen::new(&source).build();
  let source_offsets = SourceOffsets::new(&source);
  let generated_offsets = SourceOffsets::new(&ret.source_text);
  let script_kind = if ret.source_type.is_typescript() { "tsx" } else { "jsx" }.to_string();

  NativeTransformResult {
    source_text: ret.source_text,
    script_kind,
    comments: ret
      .comments
      .iter()
      .map(|comment| native_comment(&source, &source_offsets, comment))
      .collect(),
    irregular_whitespaces: ret
      .irregular_whitespaces
      .iter()
      .map(|span| source_offsets.range(*span))
      .collect(),
    errors: ret.errors.iter().map(|error| native_diagnostic(&source_offsets, error)).collect(),
    mappings: ret
      .mappings
      .iter()
      .map(|mapping| NativeMapping {
        virtual_start: generated_offsets.offset(mapping.codegen_span.start),
        virtual_end: generated_offsets.offset(mapping.codegen_span.end),
        original_start: source_offsets.offset(mapping.original_span.start),
        original_end: source_offsets.offset(mapping.original_span.end),
      })
      .collect(),
  }
}

#[napi(object)]
pub struct NativeComment {
  #[napi(ts_type = "'Line' | 'Block'")]
  pub r#type: String,
  pub value: String,
  pub start: u32,
  pub end: u32,
  #[napi(ts_type = "[number, number]")]
  pub range: (u32, u32),
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
  #[napi(ts_type = "[number, number][]")]
  pub irregular_whitespaces: Vec<(u32, u32)>,
  pub errors: Vec<NativeDiagnostic>,
  pub mappings: Vec<NativeMapping>,
}
