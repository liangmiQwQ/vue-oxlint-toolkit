use oxc_span::Span;

use crate::lexer::{Lexer, LexerMode, VToken, VTokenKind};

impl<'a> Lexer<'a> {
  pub(super) fn scan_mode_text(&mut self) -> Option<VToken<'a>> {
    match self.mode {
      LexerMode::RawText if !self.in_tag => self.scan_text_mode_token(VTokenKind::HTMLRawText),
      LexerMode::RcData if !self.in_tag => self.scan_rcdata_token(),
      _ => None,
    }
  }

  pub(super) fn scan_tag_like(&mut self, kind: VTokenKind, prefix_len: u32) -> VToken<'a> {
    let start = self.pos;
    self.pos += prefix_len;
    let name_start = self.pos;
    while self.pos < self.source_len() {
      let b = self.current_byte();
      if !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        break;
      }
      self.pos += 1;
    }
    self.in_tag = true;
    VToken::new(kind, Span::new(start, self.pos), Some(self.slice(name_start, self.pos)))
  }

  pub(super) fn scan_html_comment(&mut self) -> VToken<'a> {
    let start = self.pos;
    let value_start = start + 4;
    let (end, value_end) = self
      .find_after("-->")
      .map_or_else(|| (self.source_len(), self.source_len()), |end| (end, end - 3));
    self.pos = end;
    VToken::new(
      VTokenKind::HTMLComment,
      Span::new(start, self.pos),
      Some(self.slice(value_start, value_end)),
    )
  }

  pub(super) fn scan_bogus_comment(&mut self) -> VToken<'a> {
    let start = self.pos;
    let value_start = start + 2;
    while self.pos < self.source_len() && self.current_byte() != b'>' {
      self.pos += 1;
    }
    let value_end = self.pos;
    if self.pos < self.source_len() {
      self.pos += 1;
    }
    VToken::new(
      VTokenKind::HTMLBogusComment,
      Span::new(start, self.pos),
      Some(self.slice(value_start, value_end)),
    )
  }

  pub(super) fn scan_cdata_text(&mut self) -> VToken<'a> {
    let start = self.pos;
    let value_start = start + "<![CDATA[".len() as u32;
    let (end, value_end) = self
      .find_after("]]>")
      .map_or_else(|| (self.source_len(), self.source_len()), |end| (end, end - 3));
    self.pos = end;
    VToken::new(
      VTokenKind::HTMLCDataText,
      Span::new(start, self.pos),
      Some(self.slice(value_start, value_end)),
    )
  }

  pub(super) fn scan_quoted_literal(&mut self) -> VToken<'a> {
    let start = self.pos;
    let quote = self.current_byte();
    self.pos += 1;
    let value_start = self.pos;
    while self.pos < self.source_len() && self.current_byte() != quote {
      self.pos += 1;
    }
    let value_end = self.pos;
    if self.pos < self.source_len() {
      self.pos += 1;
    }
    VToken::new(
      VTokenKind::HTMLLiteral,
      Span::new(start, self.pos),
      Some(self.slice(value_start, value_end)),
    )
  }

  pub(super) fn scan_text_or_identifier(&mut self) -> VToken<'a> {
    if self.in_tag {
      return self.scan_identifier();
    }
    self
      .scan_text_mode_token(VTokenKind::HTMLText)
      .unwrap_or_else(|| self.scan_run(VTokenKind::HTMLWhitespace, u8::is_ascii_whitespace))
  }

  pub(super) fn scan_run(&mut self, kind: VTokenKind, predicate: fn(&u8) -> bool) -> VToken<'a> {
    let start = self.pos;
    while self.pos < self.source_len() && predicate(&self.current_byte()) {
      self.pos += 1;
    }
    VToken::new(kind, Span::new(start, self.pos), Some(self.slice(start, self.pos)))
  }

  fn scan_identifier(&mut self) -> VToken<'a> {
    let start = self.pos;
    while self.pos < self.source_len() {
      let b = self.current_byte();
      if b.is_ascii_whitespace()
        || matches!(
          b,
          b'<' | b'>' | b'/' | b'=' | b'\'' | b'"' | b':' | b'@' | b'#' | b'.' | b'[' | b']'
        )
      {
        break;
      }
      self.pos += 1;
    }
    VToken::new(
      VTokenKind::HTMLIdentifier,
      Span::new(start, self.pos),
      Some(self.slice(start, self.pos)),
    )
  }

  fn scan_text_mode_token(&mut self, kind: VTokenKind) -> Option<VToken<'a>> {
    if self.current_byte().is_ascii_whitespace() {
      return Some(self.scan_run(VTokenKind::HTMLWhitespace, u8::is_ascii_whitespace));
    }
    if self.current_byte() == b'<' && self.is_text_end() {
      return None;
    }
    if self.mode != LexerMode::VPre && (self.starts_with("{{") || self.starts_with("}}")) {
      return None;
    }

    let start = self.pos;
    while self.pos < self.source_len() {
      if self.current_byte().is_ascii_whitespace()
        || (self.current_byte() == b'<' && self.is_text_end())
        || (self.mode != LexerMode::VPre && (self.starts_with("{{") || self.starts_with("}}")))
      {
        break;
      }
      self.pos += 1;
    }
    Some(VToken::new(kind, Span::new(start, self.pos), Some(self.slice(start, self.pos))))
  }

  fn scan_rcdata_token(&mut self) -> Option<VToken<'a>> {
    if self.current_byte().is_ascii_whitespace() {
      return Some(self.scan_run(VTokenKind::HTMLWhitespace, u8::is_ascii_whitespace));
    }
    if self.current_byte() == b'<' && self.is_mode_end_tag() {
      return None;
    }

    let start = self.pos;
    if self.current_byte() == b'&'
      && let Some((end, value)) = self.scan_character_reference()
    {
      self.pos = end;
      return Some(VToken::new(VTokenKind::HTMLRCDataText, Span::new(start, end), Some(value)));
    }

    while self.pos < self.source_len() {
      if self.current_byte().is_ascii_whitespace()
        || (self.current_byte() == b'&' && self.character_reference().is_some())
        || (self.current_byte() == b'<' && self.is_mode_end_tag())
      {
        break;
      }
      self.pos += 1;
    }
    Some(VToken::new(
      VTokenKind::HTMLRCDataText,
      Span::new(start, self.pos),
      Some(self.slice(start, self.pos)),
    ))
  }

  fn scan_character_reference(&self) -> Option<(u32, &'a str)> {
    let (end, value) = self.character_reference()?;
    Some((end, self.allocator.alloc_str(value)))
  }

  fn character_reference(&self) -> Option<(u32, &'static str)> {
    let start = self.pos + 1;
    let mut end = start;
    while end < self.source_len() && self.source[end as usize].is_ascii_alphanumeric() {
      end += 1;
    }
    if end >= self.source_len() || self.source[end as usize] != b';' {
      return None;
    }

    let name = self.slice(start, end);
    let value = match name {
      "amp" => "&",
      "lt" => "<",
      "gt" => ">",
      "quot" => "\"",
      "apos" => "'",
      _ => return None,
    };
    Some((end + 1, value))
  }

  fn is_mode_end_tag(&self) -> bool {
    let Some(end_tag) = self.mode_end_tag else {
      return true;
    };
    if !self.starts_with_ascii_case_insensitive(end_tag) {
      return false;
    }

    let boundary = self.pos + end_tag.len() as u32;
    if boundary >= self.source_len() {
      return true;
    }

    !is_tag_name_continue(self.source[boundary as usize])
  }

  fn is_text_end(&self) -> bool {
    if self.mode_end_tag.is_some() {
      return self.is_mode_end_tag();
    }

    self.starts_with("<!--")
      || self.starts_with("<!")
      || (self.starts_with("</") && self.is_tag_name_start(2))
      || self.is_tag_name_start(1)
  }
}

const fn is_tag_name_continue(byte: u8) -> bool {
  byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
}
