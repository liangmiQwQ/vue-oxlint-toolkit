use memchr::memmem;

use crate::lexer::utils::{
  find_case_insensitive, is_html_whitespace, starts_with_ignore_ascii_case,
};
use crate::lexer::{Lexer, LexerMode, VToken, VTokenKind};

impl<'s> Lexer<'s> {
  pub(super) fn next_interpolation_token(&mut self) -> VToken<'s> {
    let start = self.pos;
    let bytes = &self.source[start as usize..];

    if bytes.starts_with(b"}}") {
      self.pos += 2;
      self.interpolation = false;
      return self.token(VTokenKind::VExpressionEnd, start, self.pos, None);
    }

    let end = memmem::find(bytes, b"}}").map_or(self.source.len(), |index| start as usize + index);
    self.pos = end as u32;
    self.token(VTokenKind::HTMLText, start, self.pos, Some((start, self.pos)))
  }

  pub(super) fn next_raw_text_token(&mut self, tag_name: &str) -> VToken<'s> {
    self.next_text_mode_token(tag_name, VTokenKind::HTMLRawText)
  }

  pub(super) fn next_rc_data_token(&mut self, tag_name: &str) -> VToken<'s> {
    self.next_text_mode_token(tag_name, VTokenKind::HTMLRCDataText)
  }

  fn next_text_mode_token(&mut self, tag_name: &str, text_kind: VTokenKind) -> VToken<'s> {
    let start = self.pos;
    let bytes = &self.source[start as usize..];
    let close = format!("</{tag_name}");

    if starts_with_ignore_ascii_case(bytes, close.as_bytes()) {
      self.mode = LexerMode::Data;
      self.pos += 2;
      self.in_tag = true;
      return self.token(VTokenKind::HTMLEndTagOpen, start, self.pos, None);
    }

    if is_html_whitespace(bytes[0]) {
      self.consume_while(is_html_whitespace);
      return self.token(VTokenKind::HTMLWhitespace, start, self.pos, Some((start, self.pos)));
    }

    let close_index = find_case_insensitive(bytes, close.as_bytes()).unwrap_or(bytes.len());
    let whitespace_index =
      bytes.iter().position(|byte| is_html_whitespace(*byte)).unwrap_or(bytes.len());
    let end = start as usize + close_index.min(whitespace_index);
    self.pos = end as u32;
    self.token(text_kind, start, self.pos, Some((start, self.pos)))
  }
}
