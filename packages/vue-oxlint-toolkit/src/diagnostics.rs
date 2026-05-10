use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span;

use crate::transform::NativeDiagnostic;

pub fn native_diagnostic(error: &OxcDiagnostic) -> NativeDiagnostic {
  let span =
    error.labels.as_ref().and_then(|labels| labels.first()).map_or_else(Span::default, |label| {
      let start = label.offset() as u32;
      Span::new(start, start + label.len() as u32)
    });

  NativeDiagnostic {
    message: error.message.to_string(),
    // We do not process utf8 and utf16 converting on Rust side
    // It is to avoid modifying ast, as we need to reuse the generated ast on jsx crate.
    start: span.start,
    end: span.end,
  }
}
