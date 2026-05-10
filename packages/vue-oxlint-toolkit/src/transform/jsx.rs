use napi_derive::napi;
use vue_oxlint_jsx::VueJsxCodegen;

use crate::source_text::SourceOffsets;

use super::{
  comments::native_comment,
  diagnostics::native_diagnostic,
  types::{NativeMapping, NativeTransformResult},
};

#[napi]
#[must_use]
#[allow(clippy::needless_pass_by_value, reason = "N-API owns string arguments at the boundary.")]
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
