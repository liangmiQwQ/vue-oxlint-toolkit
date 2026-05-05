use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span;

pub fn unexpected_eof(span: Span) -> OxcDiagnostic {
  OxcDiagnostic::error("Unexpected end of input.").with_label(span.label("unexpected EOF"))
}

pub fn unexpected_token(span: Span, expected: &str) -> OxcDiagnostic {
  OxcDiagnostic::error(format!("Unexpected token, expected {expected}."))
    .with_label(span.label("unexpected token"))
}

pub fn unexpected_closing_tag(span: Span) -> OxcDiagnostic {
  OxcDiagnostic::error("Unexpected closing tag.").with_label(span.label("unexpected closing tag"))
}

pub fn unexpected_script_lang(lang: &str) -> OxcDiagnostic {
  OxcDiagnostic::error(format!("Unexpected script lang \"{lang}\"."))
}
