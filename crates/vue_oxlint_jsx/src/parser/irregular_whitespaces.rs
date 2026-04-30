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
  use crate::VueJsxParser;
  use oxc_allocator::Allocator;

  #[test]
  fn test_irregular_whitespaces() {
    let allocator = Allocator::default();
    // \u{000B} is vertical tab, an irregular whitespace
    let source_text = "<div>\u{000B}</div>";
    let parser = VueJsxParser::new(&allocator, source_text);
    let ret = parser.parse();
    assert_eq!(ret.irregular_whitespaces.len(), 1);
    assert_eq!(ret.irregular_whitespaces[0].start, 5);
    assert_eq!(ret.irregular_whitespaces[0].end, 6);
  }
}
