use crate::lexer::utils::{char_len, find_after, is_html_whitespace};
use crate::lexer::{Lexer, VToken, VTokenKind};

impl<'s> Lexer<'s> {
  pub(super) fn next_data_token(&mut self) -> VToken<'s> {
    let start = self.pos;
    let bytes = &self.source[start as usize..];

    if bytes.starts_with(b"<!--") {
      let end = find_after(bytes, b"-->", 3).unwrap_or(bytes.len());
      self.pos += end as u32;
      return self.token(VTokenKind::HTMLComment, start, self.pos, Some((start + 4, self.pos - 3)));
    }

    if bytes.starts_with(b"<![CDATA[") {
      let end = find_after(bytes, b"]]>", 3).unwrap_or(bytes.len());
      self.pos += end as u32;
      return self.token(
        VTokenKind::HTMLCDataText,
        start,
        self.pos,
        Some((start + 9, self.pos - 3)),
      );
    }

    if bytes.starts_with(b"<!") {
      let end = bytes.iter().position(|byte| *byte == b'>').map_or(bytes.len(), |index| index + 1);
      self.pos += end as u32;
      return self.token(
        VTokenKind::HTMLBogusComment,
        start,
        self.pos,
        Some((start + 2, self.pos.saturating_sub(1))),
      );
    }

    if bytes.starts_with(b"</") {
      self.pos += 2;
      let name_start = self.pos;
      self.consume_tag_name();
      self.in_tag = true;
      return self.token(VTokenKind::HTMLEndTagOpen, start, self.pos, Some((name_start, self.pos)));
    }

    if bytes[0] == b'<' {
      self.pos += 1;
      let name_start = self.pos;
      self.consume_tag_name();
      self.in_tag = true;
      return self.token(VTokenKind::HTMLTagOpen, start, self.pos, Some((name_start, self.pos)));
    }

    if self.v_pre_depth == 0 && bytes.starts_with(b"{{") {
      self.pos += 2;
      self.interpolation = true;
      return self.token(VTokenKind::VExpressionStart, start, self.pos, None);
    }

    if is_html_whitespace(bytes[0]) {
      self.consume_while(is_html_whitespace);
      return self.token(VTokenKind::HTMLWhitespace, start, self.pos, Some((start, self.pos)));
    }

    let mut end = start as usize;
    while end < self.source.len() {
      let rest = &self.source[end..];
      if rest.starts_with(b"<")
        || (self.v_pre_depth == 0 && rest.starts_with(b"{{"))
        || is_html_whitespace(self.source[end])
      {
        break;
      }
      end += char_len(self.source[end]);
    }
    self.pos = end as u32;
    self.token(VTokenKind::HTMLText, start, self.pos, Some((start, self.pos)))
  }

  fn consume_tag_name(&mut self) {
    while (self.pos as usize) < self.source.len() {
      let byte = self.source[self.pos as usize];
      if is_html_whitespace(byte) || matches!(byte, b'=' | b'>' | b'/' | b'\'' | b'"') {
        break;
      }
      self.pos += char_len(byte) as u32;
    }
  }
}
