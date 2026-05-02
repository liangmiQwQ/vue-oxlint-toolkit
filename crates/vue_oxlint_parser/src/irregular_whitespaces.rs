use oxc_span::Span;

/// Collect all irregular whitespace positions in the source text.
#[must_use]
pub fn collect_irregular_whitespaces(source_text: &str) -> Box<[Span]> {
  let mut irregular_whitespaces = Vec::new();
  let mut offset = 0;
  for c in source_text.chars() {
    if oxc_syntax::identifier::is_irregular_whitespace(c) {
      irregular_whitespaces.push(Span::sized(offset, c.len_utf8() as u32));
    }
    offset += c.len_utf8() as u32;
  }
  irregular_whitespaces.into_boxed_slice()
}
