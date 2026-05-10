use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span;

use crate::source_text::SourceOffsets;

use super::NativeDiagnostic;

pub fn native_diagnostic(offsets: &SourceOffsets, error: &OxcDiagnostic) -> NativeDiagnostic {
  let span =
    error.labels.as_ref().and_then(|labels| labels.first()).map_or_else(Span::default, |label| {
      let start = label.offset() as u32;
      Span::new(start, start + label.len() as u32)
    });

  NativeDiagnostic {
    message: error.message.to_string(),
    start: offsets.offset(span.start),
    end: offsets.offset(span.end),
  }
}
