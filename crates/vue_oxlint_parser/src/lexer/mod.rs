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

mod cursor;
mod scan;
mod tokens;

pub use tokens::{VToken, VTokenKind};

use oxc_allocator::Allocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LexerMode {
  Data,
  RcData,
  RawText,
  ForeignContent,
  VPre,
}

/// Vue template lexer.
///
/// Produces [`VToken`]s in source order with original-source spans.
#[allow(dead_code)]
pub struct Lexer<'a> {
  allocator: &'a Allocator,
  source: &'a [u8],
  pos: u32,
  mode: LexerMode,
  mode_end_tag: Option<&'a str>,
  in_tag: bool,
}

#[allow(dead_code)]
impl<'a> Lexer<'a> {
  #[must_use]
  pub const fn new(allocator: &'a Allocator, source_text: &'a str) -> Self {
    Self {
      allocator,
      source: source_text.as_bytes(),
      pos: 0,
      mode: LexerMode::Data,
      mode_end_tag: None,
      in_tag: false,
    }
  }

  pub const fn set_mode(&mut self, mode: LexerMode) {
    self.mode = mode;
    self.mode_end_tag = None;
  }

  pub const fn set_mode_until(&mut self, mode: LexerMode, end_tag: &'a str) {
    self.mode = mode;
    self.mode_end_tag = Some(end_tag);
  }

  pub fn next_token(&mut self) -> Option<VToken<'a>> {
    if self.pos >= self.source.len() as u32 {
      return None;
    }

    if let Some(token) = self.scan_mode_text() {
      return Some(token);
    }

    let start = self.pos;
    let token = match self.current_byte() {
      b'<' if self.starts_with("<!--") => self.scan_html_comment(),
      b'<' if self.starts_with("</") && self.is_tag_name_start(2) => {
        self.scan_tag_like(VTokenKind::HTMLEndTagOpen, 2)
      }
      b'<' if self.starts_with("<![CDATA[") && self.mode == LexerMode::ForeignContent => {
        self.scan_cdata_text()
      }
      b'<' if self.starts_with("<!") => self.scan_bogus_comment(),
      b'<' if self.is_tag_name_start(1) => self.scan_tag_like(VTokenKind::HTMLTagOpen, 1),
      b'/' if self.starts_with("/>") => {
        self.pos += 2;
        self.in_tag = false;
        VToken::new(
          VTokenKind::HTMLSelfClosingTagClose,
          oxc_span::Span::new(start, self.pos),
          Some(""),
        )
      }
      b'>' => {
        self.pos += 1;
        self.in_tag = false;
        VToken::new(VTokenKind::HTMLTagClose, oxc_span::Span::new(start, self.pos), Some(""))
      }
      b'=' => {
        self.pos += 1;
        VToken::new(VTokenKind::HTMLAssociation, oxc_span::Span::new(start, self.pos), Some(""))
      }
      b'\'' | b'"' => self.scan_quoted_literal(),
      b'{' if !self.in_tag && self.mode != LexerMode::VPre && self.starts_with("{{") => {
        self.pos += 2;
        VToken::new(VTokenKind::VExpressionStart, oxc_span::Span::new(start, self.pos), Some("{{"))
      }
      b'}' if !self.in_tag && self.mode != LexerMode::VPre && self.starts_with("}}") => {
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

  fn is_tag_name_start(&self, offset: u32) -> bool {
    let pos = self.pos + offset;
    if pos >= self.source.len() as u32 {
      return false;
    }
    let byte = self.source[pos as usize];
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b':')
  }
}

#[cfg(test)]
mod test {
  use oxc_allocator::Allocator;

  use super::{Lexer, LexerMode, VTokenKind};

  #[test]
  fn emits_comment_and_bogus_comment_tokens() {
    let allocator = Allocator::new();
    let mut lexer = Lexer::new(&allocator, "<!-- hi --><!bogus>");

    let comment = lexer.next_token().unwrap();
    assert_eq!(comment.kind, VTokenKind::HTMLComment);
    assert_eq!(comment.value, Some(" hi "));

    let bogus = lexer.next_token().unwrap();
    assert_eq!(bogus.kind, VTokenKind::HTMLBogusComment);
    assert_eq!(bogus.value, Some("bogus"));
  }

  #[test]
  fn emits_cdata_and_rcdata_tokens() {
    let allocator = Allocator::new();
    let mut cdata_lexer = Lexer::new(&allocator, "<![CDATA[ hello ]]>");
    cdata_lexer.set_mode(LexerMode::ForeignContent);

    let cdata = cdata_lexer.next_token().unwrap();
    assert_eq!(cdata.kind, VTokenKind::HTMLCDataText);
    assert_eq!(cdata.value, Some(" hello "));

    let mut rcdata_lexer = Lexer::new(&allocator, "a &amp; b");
    rcdata_lexer.set_mode(LexerMode::RcData);

    assert_eq!(rcdata_lexer.next_token().unwrap().kind, VTokenKind::HTMLRCDataText);
    assert_eq!(rcdata_lexer.next_token().unwrap().kind, VTokenKind::HTMLWhitespace);
    let amp = rcdata_lexer.next_token().unwrap();
    assert_eq!(amp.kind, VTokenKind::HTMLRCDataText);
    assert_eq!(amp.value, Some("&"));
  }

  #[test]
  fn raw_text_only_stops_at_matching_close_tag() {
    let allocator = Allocator::new();
    let mut lexer = Lexer::new(&allocator, "a <div> b </script>");
    lexer.set_mode_until(LexerMode::RawText, "</script");

    assert_eq!(lexer.next_token().unwrap().kind, VTokenKind::HTMLRawText);
    assert_eq!(lexer.next_token().unwrap().kind, VTokenKind::HTMLWhitespace);
    let raw = lexer.next_token().unwrap();
    assert_eq!(raw.kind, VTokenKind::HTMLRawText);
    assert_eq!(raw.value, Some("<div>"));
    assert_eq!(lexer.next_token().unwrap().kind, VTokenKind::HTMLWhitespace);
    assert_eq!(lexer.next_token().unwrap().kind, VTokenKind::HTMLRawText);
    assert_eq!(lexer.next_token().unwrap().kind, VTokenKind::HTMLWhitespace);
    assert_eq!(lexer.next_token().unwrap().kind, VTokenKind::HTMLEndTagOpen);
  }

  #[test]
  fn v_pre_treats_interpolation_delimiters_as_text() {
    let allocator = Allocator::new();
    let mut lexer = Lexer::new(&allocator, "{{ msg }}");
    lexer.set_mode(LexerMode::VPre);

    let token = lexer.next_token().unwrap();
    assert_eq!(token.kind, VTokenKind::HTMLText);
    assert_eq!(token.value, Some("{{"));
  }
}
