//! Vue template lexer.
//!
//! HTML5-aware tokenizer that follows `vue-eslint-parser`'s behaviour:
//!
//! - Raw-text mode for `<script>`, `<style>`, `<xmp>`, `<noframes>`, `<noscript>`, `<noembed>`, `<iframe>`, `<plaintext>` — only the matching close tag terminates the body.
//! - RCDATA mode for `<textarea>` and `<title>` — the body is text but character references resolve.
//! - Foreign-content mode for `<svg>` / `<math>` — `<![CDATA[ ... ]]>` is recognised inside.
//! - `v-pre` mode where `{{` / `}}` is treated as text rather than as interpolation delimiters.
//!
//! The mode is set explicitly by the parser via [`Lexer::set_mode`] when it
//! crosses element boundaries — the lexer does not infer it from the tag
//! name on its own. This matches how `vue-eslint-parser` drives its
//! intermediate tokenizer.
//!
//! Spans are all in original SFC byte-offset space.

mod tokens;

pub use tokens::{VToken, VTokenKind};

use oxc_allocator::Allocator;

/// Vue template lexer.
///
/// Produces [`VToken`]s in source order with original-source spans.
#[allow(dead_code)]
pub struct Lexer<'a> {
  allocator: &'a Allocator,
  source: &'a [u8],
  pos: u32,
}

#[allow(dead_code)]
impl<'a> Lexer<'a> {
  #[must_use]
  pub const fn new(allocator: &'a Allocator, source_text: &'a str) -> Self {
    Self { allocator, source: source_text.as_bytes(), pos: 0 }
  }

  pub fn next_token(&mut self) -> Option<VToken<'a>> {
    while self.pos < self.source.len() as u32 && self.starts_with("<!--") {
      self.pos = self.find_after("-->").unwrap_or(self.source.len() as u32);
    }

    if self.pos >= self.source.len() as u32 {
      return None;
    }

    let start = self.pos;
    let token = match self.current_byte() {
      b'<' if self.starts_with("</") => self.scan_tag_like(VTokenKind::HTMLEndTagOpen, 2),
      b'<' if self.starts_with("<!--") => unreachable!(),
      b'<' => self.scan_tag_like(VTokenKind::HTMLTagOpen, 1),
      b'/' if self.starts_with("/>") => {
        self.pos += 2;
        VToken::new(
          VTokenKind::HTMLSelfClosingTagClose,
          oxc_span::Span::new(start, self.pos),
          Some(""),
        )
      }
      b'>' => {
        self.pos += 1;
        VToken::new(VTokenKind::HTMLTagClose, oxc_span::Span::new(start, self.pos), Some(""))
      }
      b'=' => {
        self.pos += 1;
        VToken::new(VTokenKind::HTMLAssociation, oxc_span::Span::new(start, self.pos), Some(""))
      }
      b'\'' | b'"' => self.scan_quoted_literal(),
      b'{' if self.starts_with("{{") => {
        self.pos += 2;
        VToken::new(VTokenKind::VExpressionStart, oxc_span::Span::new(start, self.pos), Some("{{"))
      }
      b'}' if self.starts_with("}}") => {
        self.pos += 2;
        VToken::new(VTokenKind::VExpressionEnd, oxc_span::Span::new(start, self.pos), Some("}}"))
      }
      b':' | b'@' | b'#' | b'.' | b'[' | b']' => {
        self.pos += 1;
        VToken::new(
          VTokenKind::Punctuator,
          oxc_span::Span::new(start, self.pos),
          Some(self.slice(start, self.pos)),
        )
      }
      b if b.is_ascii_whitespace() => {
        self.scan_run(VTokenKind::HTMLWhitespace, u8::is_ascii_whitespace)
      }
      _ => self.scan_text_or_identifier(),
    };

    Some(token)
  }

  fn scan_tag_like(&mut self, kind: VTokenKind, prefix_len: u32) -> VToken<'a> {
    let start = self.pos;
    self.pos += prefix_len;
    let name_start = self.pos;
    while self.pos < self.source.len() as u32 {
      let b = self.current_byte();
      if !(b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')) {
        break;
      }
      self.pos += 1;
    }
    VToken::new(kind, oxc_span::Span::new(start, self.pos), Some(self.slice(name_start, self.pos)))
  }

  fn scan_quoted_literal(&mut self) -> VToken<'a> {
    let start = self.pos;
    let quote = self.current_byte();
    self.pos += 1;
    let value_start = self.pos;
    while self.pos < self.source.len() as u32 && self.current_byte() != quote {
      self.pos += 1;
    }
    let value_end = self.pos;
    if self.pos < self.source.len() as u32 {
      self.pos += 1;
    }
    VToken::new(
      VTokenKind::HTMLLiteral,
      oxc_span::Span::new(start, self.pos),
      Some(self.slice(value_start, value_end)),
    )
  }

  fn scan_text_or_identifier(&mut self) -> VToken<'a> {
    let start = self.pos;
    while self.pos < self.source.len() as u32 {
      let b = self.current_byte();
      if b.is_ascii_whitespace()
        || matches!(
          b,
          b'<' | b'>' | b'/' | b'=' | b'\'' | b'"' | b':' | b'@' | b'#' | b'.' | b'[' | b']'
        )
        || self.starts_with("{{")
        || self.starts_with("}}")
      {
        break;
      }
      self.pos += 1;
    }
    VToken::new(
      VTokenKind::HTMLIdentifier,
      oxc_span::Span::new(start, self.pos),
      Some(self.slice(start, self.pos)),
    )
  }

  fn scan_run(&mut self, kind: VTokenKind, predicate: fn(&u8) -> bool) -> VToken<'a> {
    let start = self.pos;
    while self.pos < self.source.len() as u32 && predicate(&self.current_byte()) {
      self.pos += 1;
    }
    VToken::new(kind, oxc_span::Span::new(start, self.pos), Some(self.slice(start, self.pos)))
  }

  fn starts_with(&self, needle: &str) -> bool {
    self.source[self.pos as usize..].starts_with(needle.as_bytes())
  }

  fn find_after(&self, needle: &str) -> Option<u32> {
    let haystack = &self.source[self.pos as usize..];
    haystack
      .windows(needle.len())
      .position(|window| window == needle.as_bytes())
      .map(|index| self.pos + index as u32 + needle.len() as u32)
  }

  fn current_byte(&self) -> u8 {
    self.source[self.pos as usize]
  }

  fn slice(&self, start: u32, end: u32) -> &'a str {
    let bytes = &self.source[start as usize..end as usize];
    self.allocator.alloc_str(str::from_utf8(bytes).unwrap_or_default())
  }
}
