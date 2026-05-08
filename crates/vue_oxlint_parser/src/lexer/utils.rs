use memchr::memmem;
use oxc_span::Span;

use crate::lexer::{Lexer, VToken, VTokenKind};

impl<'s> Lexer<'s> {
  pub(super) fn consume_while(&mut self, predicate: fn(u8) -> bool) {
    while (self.pos as usize) < self.source.len() && predicate(self.source[self.pos as usize]) {
      self.pos += 1;
    }
  }

  pub(super) fn token(
    &self,
    kind: VTokenKind,
    start: u32,
    end: u32,
    value_span: Option<(u32, u32)>,
  ) -> VToken<'s> {
    VToken::new(
      kind,
      Span::new(start, end),
      value_span.map_or_else(
        || kind.default_value(),
        |(start, end)| &self.source_text[start as usize..end as usize],
      ),
    )
  }
}

pub(super) fn find_after(bytes: &[u8], needle: &[u8], needle_len: usize) -> Option<usize> {
  memmem::find(bytes, needle).map(|index| index + needle_len)
}

pub(super) fn find_case_insensitive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack.windows(needle.len()).position(|window| starts_with_ignore_ascii_case(window, needle))
}

pub(super) fn starts_with_ignore_ascii_case(bytes: &[u8], needle: &[u8]) -> bool {
  bytes.len() >= needle.len() && bytes[..needle.len()].eq_ignore_ascii_case(needle)
}

pub(super) const fn is_directive_punctuator(byte: u8) -> bool {
  matches!(byte, b':' | b'@' | b'#' | b'.')
}

pub(super) const fn is_html_whitespace(byte: u8) -> bool {
  matches!(byte, b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
}

pub(super) const fn char_len(byte: u8) -> usize {
  if byte < 0x80 {
    1
  } else if byte < 0xE0 {
    2
  } else if byte < 0xF0 {
    3
  } else {
    4
  }
}
