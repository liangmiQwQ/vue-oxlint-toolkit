use napi_derive::napi;
use oxc_allocator::Allocator;
use oxc_estree::{CompactTSSerializer, ESTree};
use vue_oxlint_parser::VueParser;

use crate::{
  diagnostics::native_diagnostic, source_text::SourceOffsets, transform::NativeDiagnostic,
};

#[napi]
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn native_parse(source: String) -> NativeParseResult {
  let vue_allocator = Allocator::default();
  let js_allocator = Allocator::default();
  let ret = VueParser::new(&vue_allocator, &js_allocator, &source).parse();
  let source_offsets = SourceOffsets::new(&source);

  let mut serializer = CompactTSSerializer::new(true);
  ret.sfc.serialize(&mut serializer);

  NativeParseResult {
    ast_json: serializer.into_string(),
    irregular_whitespaces: ret
      .irregular_whitespaces
      .iter()
      .map(|span| source_offsets.range(*span))
      .collect(),
    errors: ret.errors.iter().map(|error| native_diagnostic(&source_offsets, error)).collect(),
    panicked: ret.panicked,
  }
}

#[napi(object)]
pub struct NativeParseResult {
  pub ast_json: String,
  #[napi(ts_type = "[number, number][]")]
  pub irregular_whitespaces: Vec<(u32, u32)>,
  pub errors: Vec<NativeDiagnostic>,
  pub panicked: bool,
}
