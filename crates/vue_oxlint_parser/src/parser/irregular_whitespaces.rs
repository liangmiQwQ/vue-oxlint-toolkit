use oxc_span::Span;

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

#[cfg(test)]
mod tests {
  use oxc_allocator::Allocator;

  use crate::VueParser;

  #[test]
  fn test_irregular_whitespaces() {
    let vue_allocator = Allocator::default();
    let js_allocator = Allocator::default();
    // U+000B is a vertical tab, which Oxlint reports as irregular whitespace.
    let source_text = "<div>\u{000B}</div>";
    let ret = VueParser::new(&vue_allocator, &js_allocator, source_text).parse();

    assert_eq!(ret.irregular_whitespaces.len(), 1);
    assert_eq!(ret.irregular_whitespaces[0].start, 5);
    assert_eq!(ret.irregular_whitespaces[0].end, 6);
  }
}
