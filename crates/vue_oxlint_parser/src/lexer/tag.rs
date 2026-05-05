use crate::lexer::utils::{char_len, is_directive_punctuator, is_html_whitespace};
use crate::lexer::{Lexer, VToken, VTokenKind};

impl<'s> Lexer<'s> {
  pub(super) fn next_tag_token(&mut self) -> VToken<'s> {
    let start = self.pos;
    let bytes = &self.source[start as usize..];

    if bytes.starts_with(b"/>") {
      self.pos += 2;
      self.in_tag = false;
      return self.token(VTokenKind::HTMLSelfClosingTagClose, start, self.pos, None);
    }

    if bytes[0] == b'>' {
      self.pos += 1;
      self.in_tag = false;
      return self.token(VTokenKind::HTMLTagClose, start, self.pos, None);
    }

    if bytes[0] == b'=' {
      self.pos += 1;
      return self.token(VTokenKind::HTMLAssociation, start, self.pos, None);
    }

    if is_html_whitespace(bytes[0]) {
      self.consume_while(is_html_whitespace);
      return self.token(VTokenKind::HTMLWhitespace, start, self.pos, Some((start, self.pos)));
    }

    if matches!(bytes[0], b'\'' | b'"') {
      let quote = bytes[0];
      let mut end = start as usize + 1;
      while end < self.source.len() && self.source[end] != quote {
        end += char_len(self.source[end]);
      }
      if end < self.source.len() {
        end += 1;
      }
      self.pos = end as u32;
      return self.token(VTokenKind::HTMLLiteral, start, self.pos, Some((start + 1, self.pos - 1)));
    }

    if is_directive_punctuator(bytes[0]) {
      self.pos += 1;
      return self.token(VTokenKind::Punctuator, start, self.pos, Some((start, self.pos)));
    }

    let mut end = start as usize;
    while end < self.source.len()
      && !is_html_whitespace(self.source[end])
      && !matches!(self.source[end], b'=' | b'>' | b'/' | b'\'' | b'"' | b':' | b'@' | b'#' | b'.')
    {
      end += char_len(self.source[end]);
    }

    if end == start as usize {
      self.pos += 1;
      return self.token(VTokenKind::Punctuator, start, self.pos, Some((start, self.pos)));
    }

    self.pos = end as u32;
    self.token(VTokenKind::HTMLIdentifier, start, self.pos, Some((start, self.pos)))
  }
}
