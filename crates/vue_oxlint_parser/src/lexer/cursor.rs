use crate::lexer::Lexer;

impl<'a> Lexer<'a> {
  pub(super) fn starts_with(&self, needle: &str) -> bool {
    self.source[self.pos as usize..].starts_with(needle.as_bytes())
  }

  pub(super) fn starts_with_ascii_case_insensitive(&self, needle: &str) -> bool {
    let source = &self.source[self.pos as usize..];
    let needle = needle.as_bytes();
    if needle.len() > source.len() {
      return false;
    }
    source[..needle.len()].iter().zip(needle).all(|(a, b)| a.eq_ignore_ascii_case(b))
  }

  pub(super) fn find_after(&self, needle: &str) -> Option<u32> {
    let haystack = &self.source[self.pos as usize..];
    haystack
      .windows(needle.len())
      .position(|window| window == needle.as_bytes())
      .map(|index| self.pos + index as u32 + needle.len() as u32)
  }

  pub(super) fn current_byte(&self) -> u8 {
    self.source[self.pos as usize]
  }

  pub(super) const fn source_len(&self) -> u32 {
    self.source.len() as u32
  }

  pub(super) fn slice(&self, start: u32, end: u32) -> &'a str {
    let bytes = &self.source[start as usize..end as usize];
    self.allocator.alloc_str(str::from_utf8(bytes).unwrap_or_default())
  }
}
