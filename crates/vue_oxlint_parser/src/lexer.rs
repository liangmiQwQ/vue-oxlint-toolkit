//! Vue template lexer.
//!
//! The lexer is mode-driven: the parser sets [`LexMode`] before each call to
//! [`Lexer::next`], which pulls a single token from the source. Modes mirror
//! the HTML5 tokenizer states this codebase needs:
//!
//! - `Data` — default content; recognizes tags, comments, and mustaches.
//! - `InTag` — inside a start tag; emits attributes and the closing `>`/`/>`.
//! - `RawText` — `<script>` / `<style>` body; one big text blob until the
//!   matching end tag.
//! - `RcData` — `<textarea>` body; text and mustaches but no nested elements.
//!
//! End tags (`</name>`) are emitted as a single `EndTag` token regardless of
//! mode, because the parser always wants to know about them whole.

use crate::ast::Span;
use crate::source::Source;
use crate::token::{LexMode, Token, TokenKind};

pub struct Lexer<'a> {
  src: Source<'a>,
  mode: LexMode<'a>,
}

impl<'a> Lexer<'a> {
  #[must_use]
  pub const fn new(text: &'a str) -> Self {
    Self { src: Source::new(text), mode: LexMode::Data }
  }

  pub const fn set_mode(&mut self, mode: LexMode<'a>) {
    self.mode = mode;
  }

  #[must_use]
  pub const fn mode(&self) -> LexMode<'a> {
    self.mode
  }

  #[must_use]
  pub const fn pos(&self) -> u32 {
    self.src.pos()
  }

  pub const fn seek(&mut self, pos: u32) {
    self.src.seek(pos);
  }

  #[allow(clippy::should_implement_trait)]
  pub fn next(&mut self) -> Token<'a> {
    match self.mode {
      LexMode::Data => self.next_data(),
      LexMode::InTag => self.next_in_tag(),
      LexMode::AttrValueUnquoted => self.next_attr_value_unquoted(),
      LexMode::RawText { name } => self.next_raw_text(name),
      LexMode::RcData { name } => self.next_rcdata(name),
    }
  }

  fn next_data(&mut self) -> Token<'a> {
    if self.src.is_eof() {
      return self.eof();
    }
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len() as u32;

    // Mustache.
    if self.src.starts_with(b"{{") {
      return self.lex_mustache();
    }

    if self.src.peek() == Some(b'<') && lo + 1 < len {
      let next = bytes[(lo + 1) as usize];
      if next == b'/' {
        return self.lex_end_tag();
      }
      if next == b'!' {
        return self.lex_bang();
      }
      if next == b'?' {
        return self.lex_processing_instruction();
      }
      if next.is_ascii_alphabetic() {
        return self.lex_tag_open();
      }
    }

    self.lex_text_until_special()
  }

  fn next_in_tag(&mut self) -> Token<'a> {
    self.skip_ascii_whitespace();
    if self.src.is_eof() {
      // Tolerate EOF in start tag.
      return Token { span: self.span_at_cursor(), kind: TokenKind::TagEnd };
    }
    let lo = self.src.pos();
    match self.src.peek() {
      Some(b'/') if self.src.peek_at(1) == Some(b'>') => {
        self.src.advance(2);
        Token { span: Span::new(lo, lo + 2), kind: TokenKind::TagSelfClose }
      }
      Some(b'>') => {
        self.src.advance(1);
        Token { span: Span::new(lo, lo + 1), kind: TokenKind::TagEnd }
      }
      Some(b'=') => {
        self.src.advance(1);
        Token { span: Span::new(lo, lo + 1), kind: TokenKind::AttrEq }
      }
      Some(b'"' | b'\'') => self.lex_attr_value_quoted(),
      _ => self.lex_attr_name_or_unquoted_value(),
    }
  }

  fn next_attr_value_unquoted(&mut self) -> Token<'a> {
    // Caller has already consumed the `=` and any whitespace; we still
    // skip leading whitespace defensively because the parser drops back
    // into this mode after `AttrEq` without explicitly trimming.
    self.skip_ascii_whitespace();
    if self.src.is_eof() {
      return self.eof();
    }
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    // If the value is quoted, defer to the quoted-value path so quotes
    // come out correctly.
    if matches!(bytes.get(lo as usize), Some(b'"' | b'\'')) {
      return self.lex_attr_value_quoted();
    }
    // If the next char is already a tag terminator, return TagEnd /
    // SelfClose so the parser can see "key= >" as an empty value.
    match bytes.get(lo as usize) {
      Some(b'>') => {
        self.src.advance(1);
        return Token { span: Span::new(lo, lo + 1), kind: TokenKind::TagEnd };
      }
      Some(b'/') if bytes.get((lo + 1) as usize) == Some(&b'>') => {
        self.src.advance(2);
        return Token { span: Span::new(lo, lo + 2), kind: TokenKind::TagSelfClose };
      }
      _ => {}
    }
    let mut p = lo as usize;
    while p < len {
      let b = bytes[p];
      if b.is_ascii_whitespace() || b == b'>' {
        break;
      }
      if b == b'/' && p + 1 < len && bytes[p + 1] == b'>' {
        break;
      }
      p += 1;
    }
    self.src.seek(p as u32);
    let text = std::str::from_utf8(&bytes[lo as usize..p]).unwrap_or("");
    Token {
      span: Span::new(lo, p as u32),
      kind: TokenKind::AttrValue { value: text, quote: None, inner_span: Span::new(lo, p as u32) },
    }
  }

  fn next_raw_text(&mut self, name: &str) -> Token<'a> {
    if self.src.is_eof() {
      return self.eof();
    }
    if self.matches_end_tag(name) {
      return self.lex_end_tag();
    }
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let mut p = lo as usize;
    let len = bytes.len();
    while p < len {
      if bytes[p] == b'<' {
        // Check for matching end tag.
        let saved = self.src.pos();
        self.src.seek(p as u32);
        let m = self.matches_end_tag(name);
        self.src.seek(saved);
        if m {
          break;
        }
      }
      p += 1;
    }
    self.src.seek(p as u32);
    Token {
      span: Span::new(lo, p as u32),
      kind: TokenKind::Text { text: std::str::from_utf8(&bytes[lo as usize..p]).unwrap_or("") },
    }
  }

  fn next_rcdata(&mut self, name: &str) -> Token<'a> {
    if self.src.is_eof() {
      return self.eof();
    }
    if self.matches_end_tag(name) {
      return self.lex_end_tag();
    }
    if self.src.starts_with(b"{{") {
      return self.lex_mustache();
    }
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    let mut p = lo as usize;
    while p < len {
      if bytes[p] == b'<' {
        let saved = self.src.pos();
        self.src.seek(p as u32);
        let m = self.matches_end_tag(name);
        self.src.seek(saved);
        if m {
          break;
        }
      }
      if bytes[p] == b'{' && p + 1 < len && bytes[p + 1] == b'{' {
        break;
      }
      p += 1;
    }
    self.src.seek(p as u32);
    Token {
      span: Span::new(lo, p as u32),
      kind: TokenKind::Text { text: std::str::from_utf8(&bytes[lo as usize..p]).unwrap_or("") },
    }
  }

  fn lex_text_until_special(&mut self) -> Token<'a> {
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    let mut p = lo as usize;
    while p < len {
      let b = bytes[p];
      if b == b'<' || (b == b'{' && p + 1 < len && bytes[p + 1] == b'{') {
        break;
      }
      p += 1;
    }
    if p == lo as usize {
      // Defensive: a stray `<` without a recognized follow-up. Consume one
      // byte as text so we make progress.
      p += 1;
    }
    self.src.seek(p as u32);
    Token {
      span: Span::new(lo, p as u32),
      kind: TokenKind::Text { text: std::str::from_utf8(&bytes[lo as usize..p]).unwrap_or("") },
    }
  }

  fn lex_mustache(&mut self) -> Token<'a> {
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    self.src.advance(2); // `{{`
    let expr_lo = self.src.pos();
    let mut p = expr_lo as usize;
    while p + 1 < len && !(bytes[p] == b'}' && bytes[p + 1] == b'}') {
      p += 1;
    }
    if p + 1 >= len {
      // Unterminated mustache: rewind and emit a single `{` as text.
      self.src.seek(lo + 1);
      return Token {
        span: Span::new(lo, lo + 1),
        kind: TokenKind::Text {
          text: std::str::from_utf8(&bytes[lo as usize..(lo + 1) as usize]).unwrap_or(""),
        },
      };
    }
    let expr_hi = p as u32;
    self.src.seek(expr_hi + 2);
    Token {
      span: Span::new(lo, self.src.pos()),
      kind: TokenKind::Mustache {
        expr: std::str::from_utf8(&bytes[expr_lo as usize..expr_hi as usize]).unwrap_or(""),
        expr_span: Span::new(expr_lo, expr_hi),
      },
    }
  }

  fn lex_tag_open(&mut self) -> Token<'a> {
    let lo = self.src.pos();
    self.src.advance(1); // `<`
    let name_lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    let mut p = name_lo as usize;
    while p < len && is_tag_name_part(bytes[p]) {
      p += 1;
    }
    if p == name_lo as usize {
      // Not a real tag — emit one byte of text and let the parser keep going.
      self.src.seek(lo + 1);
      return Token { span: Span::new(lo, lo + 1), kind: TokenKind::Text { text: "<" } };
    }
    self.src.seek(p as u32);
    let name = std::str::from_utf8(&bytes[name_lo as usize..p]).unwrap_or("");
    Token {
      span: Span::new(lo, p as u32),
      kind: TokenKind::TagOpen { name, name_span: Span::new(name_lo, p as u32) },
    }
  }

  fn lex_end_tag(&mut self) -> Token<'a> {
    let lo = self.src.pos();
    self.src.advance(2); // `</`
    let name_lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    let mut p = name_lo as usize;
    while p < len && is_tag_name_part(bytes[p]) {
      p += 1;
    }
    let name_hi = p as u32;
    // Skip whitespace and consume up to and including `>`.
    while p < len && bytes[p] != b'>' {
      p += 1;
    }
    if p < len {
      p += 1;
    }
    self.src.seek(p as u32);
    let name = std::str::from_utf8(&bytes[name_lo as usize..name_hi as usize]).unwrap_or("");
    Token {
      span: Span::new(lo, p as u32),
      kind: TokenKind::EndTag { name, name_span: Span::new(name_lo, name_hi) },
    }
  }

  fn lex_bang(&mut self) -> Token<'a> {
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    if self.src.starts_with(b"<!--") {
      self.src.advance(4);
      let body_lo = self.src.pos();
      let mut p = body_lo as usize;
      while p + 2 < len && &bytes[p..p + 3] != b"-->" {
        p += 1;
      }
      if p + 2 < len {
        p += 3;
      } else {
        // Unclosed comment: bail to next `>` for minimal recovery.
        let mut q = body_lo as usize;
        while q < len && bytes[q] != b'>' {
          q += 1;
        }
        if q < len {
          q += 1;
        }
        p = q;
      }
      self.src.seek(p as u32);
      return Token { span: Span::new(lo, p as u32), kind: TokenKind::Comment };
    }
    if self.src.starts_with(b"<![CDATA[") {
      self.src.advance(9);
      let mut p = self.src.pos() as usize;
      while p + 2 < len && &bytes[p..p + 3] != b"]]>" {
        p += 1;
      }
      if p + 2 < len {
        p += 3;
      } else {
        p = len;
      }
      self.src.seek(p as u32);
      return Token { span: Span::new(lo, p as u32), kind: TokenKind::Cdata };
    }
    // Bogus bang — `<!DOCTYPE ...>` or `<!whatever>`.
    self.src.advance(2);
    let mut p = self.src.pos() as usize;
    while p < len && bytes[p] != b'>' {
      p += 1;
    }
    if p < len {
      p += 1;
    }
    self.src.seek(p as u32);
    Token { span: Span::new(lo, p as u32), kind: TokenKind::Bang }
  }

  fn lex_processing_instruction(&mut self) -> Token<'a> {
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    self.src.advance(2); // `<?`
    let mut p = self.src.pos() as usize;
    while p + 1 < len && !(bytes[p] == b'?' && bytes[p + 1] == b'>') {
      p += 1;
    }
    if p + 1 < len {
      p += 2;
    } else {
      p = len;
    }
    self.src.seek(p as u32);
    Token { span: Span::new(lo, p as u32), kind: TokenKind::ProcessingInstruction }
  }

  fn lex_attr_name_or_unquoted_value(&mut self) -> Token<'a> {
    // Names use the same byte set as start-tag attributes in the original
    // implementation: anything that isn't whitespace, `=`, `>`, or `/>` start.
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    let mut p = lo as usize;
    while p < len {
      let b = bytes[p];
      if b.is_ascii_whitespace() || b == b'=' || b == b'>' {
        break;
      }
      if b == b'/' && p + 1 < len && bytes[p + 1] == b'>' {
        break;
      }
      p += 1;
    }
    if p == lo as usize {
      // Shouldn't happen — caller filtered known delimiters — but make
      // forward progress regardless.
      p += 1;
    }
    self.src.seek(p as u32);
    let text = std::str::from_utf8(&bytes[lo as usize..p]).unwrap_or("");
    Token { span: Span::new(lo, p as u32), kind: TokenKind::AttrName { name: text } }
  }

  fn lex_attr_value_quoted(&mut self) -> Token<'a> {
    let lo = self.src.pos();
    let bytes = self.src.bytes();
    let len = bytes.len();
    let quote = bytes[lo as usize];
    let inner_lo = lo + 1;
    let mut p = inner_lo as usize;
    while p < len && bytes[p] != quote {
      p += 1;
    }
    let inner_hi = p as u32;
    let closed = p < len;
    let span_hi = if closed { (p + 1) as u32 } else { p as u32 };
    self.src.seek(span_hi);
    let value = std::str::from_utf8(&bytes[inner_lo as usize..inner_hi as usize]).unwrap_or("");
    Token {
      span: Span::new(lo, span_hi),
      kind: TokenKind::AttrValue {
        value,
        quote: Some(quote),
        inner_span: Span::new(inner_lo, inner_hi),
      },
    }
  }

  fn skip_ascii_whitespace(&mut self) {
    let bytes = self.src.bytes();
    let mut p = self.src.pos() as usize;
    while p < bytes.len() && bytes[p].is_ascii_whitespace() {
      p += 1;
    }
    self.src.seek(p as u32);
  }

  fn matches_end_tag(&self, name: &str) -> bool {
    let bytes = self.src.bytes();
    let pos = self.src.pos() as usize;
    if !bytes[pos..].starts_with(b"</") {
      return false;
    }
    let after = pos + 2;
    let nb = name.as_bytes();
    if after + nb.len() > bytes.len() {
      return false;
    }
    if !bytes[after..after + nb.len()].eq_ignore_ascii_case(nb) {
      return false;
    }
    matches!(bytes.get(after + nb.len()), Some(b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n') | None)
  }

  const fn span_at_cursor(&self) -> Span {
    let p = self.src.pos();
    Span::new(p, p)
  }

  const fn eof(&self) -> Token<'a> {
    Token { span: self.span_at_cursor(), kind: TokenKind::Eof }
  }
}

/// Bytes that may appear in a Vue tag name. Includes `:` and `.` which Vue
/// permits in component names.
#[must_use]
pub const fn is_tag_name_part(b: u8) -> bool {
  b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b':' || b == b'.'
}

#[cfg(test)]
#[allow(clippy::option_if_let_else)]
mod tests {
  use super::*;

  fn collect(src: &str) -> Vec<String> {
    let mut lex = Lexer::new(src);
    let mut out = Vec::new();
    loop {
      let tok = lex.next();
      let label = match tok.kind {
        TokenKind::TagOpen { name, .. } => format!("TagOpen({name})"),
        TokenKind::AttrName { name } => format!("AttrName({name})"),
        TokenKind::AttrEq => "AttrEq".into(),
        TokenKind::AttrValue { value, quote, .. } => match quote {
          Some(q) => format!("AttrValue({}{value}{})", q as char, q as char),
          None => format!("AttrValue({value})"),
        },
        TokenKind::TagSelfClose => "TagSelfClose".into(),
        TokenKind::TagEnd => "TagEnd".into(),
        TokenKind::EndTag { name, .. } => format!("EndTag({name})"),
        TokenKind::Text { text } => format!("Text({text:?})"),
        TokenKind::Mustache { expr, .. } => format!("Mustache({expr:?})"),
        TokenKind::Comment => "Comment".into(),
        TokenKind::Cdata => "Cdata".into(),
        TokenKind::Bang => "Bang".into(),
        TokenKind::ProcessingInstruction => "PI".into(),
        TokenKind::Eof => break,
      };
      out.push(label);
      // For TagOpen, follow with InTag mode tokens until end of tag.
      if matches!(tok.kind, TokenKind::TagOpen { .. }) {
        lex.set_mode(LexMode::InTag);
        loop {
          let t = lex.next();
          let l = match t.kind {
            TokenKind::AttrName { name } => format!("AttrName({name})"),
            TokenKind::AttrEq => "AttrEq".into(),
            TokenKind::AttrValue { value, quote, .. } => match quote {
              Some(q) => format!("AttrValue({}{value}{})", q as char, q as char),
              None => format!("AttrValue({value})"),
            },
            TokenKind::TagSelfClose => {
              out.push("TagSelfClose".into());
              break;
            }
            TokenKind::TagEnd => {
              out.push("TagEnd".into());
              break;
            }
            TokenKind::Eof => break,
            _ => unreachable!("unexpected in-tag token: {:?}", t.kind),
          };
          out.push(l);
        }
        lex.set_mode(LexMode::Data);
      }
    }
    out
  }

  #[test]
  fn simple_tag() {
    let toks = collect("<div class=\"a\">hi</div>");
    assert_eq!(
      toks,
      vec![
        "TagOpen(div)",
        "AttrName(class)",
        "AttrEq",
        "AttrValue(\"a\")",
        "TagEnd",
        "Text(\"hi\")",
        "EndTag(div)",
      ]
    );
  }

  #[test]
  fn mustache_in_text() {
    let toks = collect("hi {{ x }} bye");
    assert_eq!(toks, vec!["Text(\"hi \")", "Mustache(\" x \")", "Text(\" bye\")"]);
  }

  #[test]
  fn self_closing() {
    let toks = collect("<br/>");
    assert_eq!(toks, vec!["TagOpen(br)", "TagSelfClose"]);
  }

  #[test]
  fn comment_token() {
    let toks = collect("<!-- x --><p/>");
    assert_eq!(toks, vec!["Comment", "TagOpen(p)", "TagSelfClose"]);
  }
}
