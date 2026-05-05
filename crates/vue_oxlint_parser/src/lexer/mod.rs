//! Vue template lexer.
//!
//! Spans are byte offsets in the original SFC. The parser drives raw-text and
//! `v-pre` modes after it has consumed start tags.

mod tokens;

pub use tokens::{VToken, VTokenKind};

use memchr::memmem;
use oxc_span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexerMode<'s> {
  Data,
  RawText(&'s str),
  RcData(&'s str),
}

/// Vue template lexer.
pub struct Lexer<'s> {
  source_text: &'s str,
  source: &'s [u8],
  pos: u32,
  in_tag: bool,
  interpolation: bool,
  v_pre_depth: u32,
  mode: LexerMode<'s>,
  panicked: bool,
}

impl<'s> Lexer<'s> {
  #[must_use]
  pub const fn new(source_text: &'s str) -> Self {
    Self {
      source_text,
      source: source_text.as_bytes(),
      pos: 0,
      in_tag: false,
      interpolation: false,
      v_pre_depth: 0,
      mode: LexerMode::Data,
      panicked: false,
    }
  }

  #[must_use]
  pub const fn panicked(&self) -> bool {
    self.panicked
  }

  pub const fn set_raw_text_mode(&mut self, tag_name: &'s str) {
    self.mode = LexerMode::RawText(tag_name);
  }

  pub const fn set_rc_data_mode(&mut self, tag_name: &'s str) {
    self.mode = LexerMode::RcData(tag_name);
  }

  pub const fn enter_v_pre(&mut self) {
    self.v_pre_depth += 1;
  }

  pub const fn leave_v_pre(&mut self) {
    self.v_pre_depth = self.v_pre_depth.saturating_sub(1);
  }

  pub const fn jump_to_eof(&mut self) {
    self.pos = self.source.len() as u32;
    self.panicked = true;
  }

  pub fn next_token(&mut self) -> Option<VToken<'s>> {
    if self.pos as usize >= self.source.len() {
      return None;
    }

    if self.in_tag {
      return Some(self.next_tag_token());
    }

    if self.interpolation {
      return Some(self.next_interpolation_token());
    }

    match self.mode {
      LexerMode::Data => Some(self.next_data_token()),
      LexerMode::RawText(tag_name) => Some(self.next_raw_text_token(tag_name)),
      LexerMode::RcData(tag_name) => Some(self.next_rc_data_token(tag_name)),
    }
  }

  fn next_data_token(&mut self) -> VToken<'s> {
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
      self.in_tag = true;
      return self.token(VTokenKind::HTMLEndTagOpen, start, self.pos, None);
    }

    if bytes[0] == b'<' {
      self.pos += 1;
      self.in_tag = true;
      return self.token(VTokenKind::HTMLTagOpen, start, self.pos, None);
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

  fn next_tag_token(&mut self) -> VToken<'s> {
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

  fn next_interpolation_token(&mut self) -> VToken<'s> {
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

  fn next_raw_text_token(&mut self, tag_name: &str) -> VToken<'s> {
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
    self.token(VTokenKind::HTMLRawText, start, self.pos, Some((start, self.pos)))
  }

  fn next_rc_data_token(&mut self, tag_name: &str) -> VToken<'s> {
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
    self.token(VTokenKind::HTMLRCDataText, start, self.pos, Some((start, self.pos)))
  }

  fn consume_while(&mut self, predicate: fn(u8) -> bool) {
    while (self.pos as usize) < self.source.len() && predicate(self.source[self.pos as usize]) {
      self.pos += 1;
    }
  }

  fn token(
    &self,
    kind: VTokenKind,
    start: u32,
    end: u32,
    value_span: Option<(u32, u32)>,
  ) -> VToken<'s> {
    VToken::new(
      kind,
      Span::new(start, end),
      value_span.map(|(start, end)| &self.source_text[start as usize..end as usize]),
    )
  }
}

fn find_after(bytes: &[u8], needle: &[u8], needle_len: usize) -> Option<usize> {
  memmem::find(bytes, needle).map(|index| index + needle_len)
}

fn find_case_insensitive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
  haystack.windows(needle.len()).position(|window| starts_with_ignore_ascii_case(window, needle))
}

fn starts_with_ignore_ascii_case(bytes: &[u8], needle: &[u8]) -> bool {
  bytes.len() >= needle.len() && bytes[..needle.len()].eq_ignore_ascii_case(needle)
}

const fn is_directive_punctuator(byte: u8) -> bool {
  matches!(byte, b':' | b'@' | b'#' | b'.')
}

const fn is_html_whitespace(byte: u8) -> bool {
  matches!(byte, b'\t' | b'\n' | b'\x0C' | b'\r' | b' ')
}

const fn char_len(byte: u8) -> usize {
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
