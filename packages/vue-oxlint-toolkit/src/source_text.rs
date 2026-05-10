use oxc_ast_visit::utf8_to_utf16::Utf8ToUtf16;
use oxc_span::Span;

pub struct SourceOffsets {
  span_converter: Utf8ToUtf16,
}

impl SourceOffsets {
  pub fn new(source_text: &str) -> Self {
    Self { span_converter: Utf8ToUtf16::new(source_text) }
  }

  pub fn offset(&self, offset: u32) -> u32 {
    let mut offset = offset;

    if let Some(mut converter) = self.span_converter.converter() {
      converter.convert_offset(&mut offset);
    }

    offset
  }

  pub fn range(&self, span: Span) -> Vec<u32> {
    vec![self.offset(span.start), self.offset(span.end)]
  }
}
