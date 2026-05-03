//! Vue template lexer.
//!
//! HTML5-aware tokenizer that mirrors `vue-eslint-parser`'s behaviour:
//!
//! - Raw-text mode for `<script>`, `<style>`, `<xmp>`, `<noframes>`,
//!   `<noscript>`, `<noembed>`, `<iframe>`, `<plaintext>` — only the matching
//!   close tag terminates the body.
//! - RCDATA mode for `<textarea>` and `<title>` — the body is text but
//!   character references resolve.
//! - Foreign-content mode for `<svg>` / `<math>` — `<![CDATA[ ... ]]>` is
//!   recognised inside.
//! - `v-pre` mode where `{{` / `}}` is treated as text rather than as
//!   interpolation delimiters.
//!
//! The mode is set explicitly by the parser via [`Lexer::set_mode`] when it
//! crosses element boundaries — the lexer does not infer it from the tag
//! name on its own. This matches how `vue-eslint-parser` drives its
//! intermediate tokenizer.
//!
//! Spans are all in original SFC byte-offset space.

mod tokens;

pub use tokens::{VToken, VTokenKind};

use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span;

/// Tokenizer mode. The parser flips this when it crosses element boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexerMode {
  /// Default mode — recognises tags, comments, character references, etc.
  Data,
  /// `<script>`, `<style>`, `<xmp>`, etc. Only `</tag>` closes the run.
  RawText,
  /// `<textarea>`, `<title>` — character references resolved, but no tags.
  RcData,
  /// `<svg>` / `<math>` — `<![CDATA[ ... ]]>` is recognised as data.
  Foreign,
  /// Inside a `v-pre` subtree — directives & interpolation are not
  /// recognised, but tags are.
  VPre,
}

impl LexerMode {
  const fn allow_interpolation(self) -> bool {
    matches!(self, Self::Data | Self::Foreign)
  }
}

/// Vue template lexer.
///
/// Produces [`VToken`]s in source order with original-source spans.
pub struct Lexer<'a> {
  allocator: &'a Allocator,
  source: &'a [u8],
  /// Current scan position, byte offset into [`Self::source`].
  pos: u32,
  mode: LexerMode,
  /// When in [`LexerMode::RawText`] / [`LexerMode::RcData`] the parser
  /// communicates which close tag terminates the body via this field.
  /// Stored lower-case ASCII.
  raw_close_tag: Option<&'a str>,
  tokens: ArenaVec<'a, VToken>,
  errors: Vec<OxcDiagnostic>,
}

impl<'a> Lexer<'a> {
  #[must_use]
  pub fn new(allocator: &'a Allocator, source_text: &'a str) -> Self {
    Self {
      allocator,
      source: source_text.as_bytes(),
      pos: 0,
      mode: LexerMode::Data,
      raw_close_tag: None,
      tokens: ArenaVec::new_in(allocator),
      errors: Vec::new(),
    }
  }

  #[must_use]
  pub const fn mode(&self) -> LexerMode {
    self.mode
  }

  pub const fn set_mode(&mut self, mode: LexerMode) {
    self.mode = mode;
  }

  /// Set the close tag (lowercase ASCII) for [`LexerMode::RawText`] or
  /// [`LexerMode::RcData`]. Cleared when the matching `</tag>` is consumed.
  pub const fn set_raw_close_tag(&mut self, tag: Option<&'a str>) {
    self.raw_close_tag = tag;
  }

  #[must_use]
  pub const fn position(&self) -> u32 {
    self.pos
  }

  /// Drive the lexer until EOF, pushing every token into the internal buffer.
  ///
  /// Mode transitions inside a structural parse will normally be driven by
  /// the parser via [`Self::next_token`] + [`Self::set_mode`]; this helper
  /// is convenient for tests and for the toolkit's "tokens-only" path.
  pub fn lex_all(&mut self) {
    while self.next_token().is_some() {}
  }

  /// Advance the lexer by one token, pushing it into the internal buffer
  /// and returning a copy.
  pub fn next_token(&mut self) -> Option<VToken> {
    if self.pos as usize >= self.source.len() {
      return None;
    }
    // Each helper pushes every token it produces (including its own
    // primary token) and returns the primary token for the caller's
    // convenience.
    match self.mode {
      LexerMode::RawText => self.lex_raw_text(),
      LexerMode::RcData => self.lex_rcdata(),
      LexerMode::Data | LexerMode::Foreign | LexerMode::VPre => self.lex_data(),
    }
  }

  /// Lex one token in the data state.
  fn lex_data(&mut self) -> Option<VToken> {
    let start = self.pos;
    let b = self.peek_byte()?;

    if b == b'<' {
      return Some(self.lex_lt(start));
    }

    // `{{` — interpolation start, only outside v-pre / rcdata / rawtext.
    if self.mode.allow_interpolation() && b == b'{' && self.peek_byte_at(1) == Some(b'{') {
      self.pos += 2;
      return Some(self.emit(VTokenKind::VExpressionStart, start));
    }

    // `}}` — interpolation end. We emit it from the data state too; the
    // parser is responsible for pairing it with a preceding start.
    if self.mode.allow_interpolation() && b == b'}' && self.peek_byte_at(1) == Some(b'}') {
      self.pos += 2;
      return Some(self.emit(VTokenKind::VExpressionEnd, start));
    }

    // `<![CDATA[` only allowed in foreign content.
    if matches!(self.mode, LexerMode::Foreign) && self.starts_with(b"<![CDATA[") {
      return Some(self.lex_cdata(start));
    }

    // Plain text run — scan to the next significant byte.
    let mut p = self.pos as usize;
    let allow_interp = self.mode.allow_interpolation();
    while p < self.source.len() {
      let c = self.source[p];
      if c == b'<' {
        break;
      }
      if allow_interp && c == b'{' && self.source.get(p + 1) == Some(&b'{') {
        break;
      }
      if allow_interp && c == b'}' && self.source.get(p + 1) == Some(&b'}') {
        break;
      }
      p += 1;
    }
    self.pos = u32::try_from(p).unwrap_or(u32::MAX);
    Some(self.emit(VTokenKind::HTMLText, start))
  }

  /// Lex everything starting at `<` in the data state.
  fn lex_lt(&mut self, start: u32) -> VToken {
    debug_assert_eq!(self.peek_byte(), Some(b'<'));

    // `</...`
    if self.peek_byte_at(1) == Some(b'/') {
      // `</` followed by ASCII alpha is a real end tag.
      if matches!(self.peek_byte_at(2), Some(c) if c.is_ascii_alphabetic()) {
        self.pos += 2;
        let tok = self.emit(VTokenKind::HTMLEndTagOpen, start);
        self.lex_tag_name();
        self.lex_tag_internals();
        return tok;
      }
      // `</>` or `</1` etc. — bogus comment per HTML5.
      return self.lex_bogus_comment(start);
    }

    // `<!`
    if self.peek_byte_at(1) == Some(b'!') {
      if self.starts_with(b"<!--") {
        return self.lex_comment(start);
      }
      if self.starts_with(b"<![CDATA[") && matches!(self.mode, LexerMode::Foreign) {
        return self.lex_cdata(start);
      }
      return self.lex_bogus_comment(start);
    }

    // `<?` — bogus comment in HTML.
    if self.peek_byte_at(1) == Some(b'?') {
      return self.lex_bogus_comment(start);
    }

    // `<` followed by ASCII alpha is a real start tag.
    if matches!(self.peek_byte_at(1), Some(c) if c.is_ascii_alphabetic()) {
      self.pos += 1;
      let tok = self.emit(VTokenKind::HTMLTagOpen, start);
      self.lex_tag_name();
      self.lex_tag_internals();
      return tok;
    }

    // Stray `<` — emit as text.
    self.pos += 1;
    self.emit(VTokenKind::HTMLText, start)
  }

  /// Lex the tag name immediately after `<` / `</`. Pushes one
  /// [`VTokenKind::HTMLIdentifier`] if any name characters are present.
  fn lex_tag_name(&mut self) {
    let start = self.pos;
    while let Some(c) = self.peek_byte() {
      if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b':' | b'.') {
        self.pos += 1;
      } else {
        break;
      }
    }
    if self.pos > start {
      let tok = VToken::new(VTokenKind::HTMLIdentifier, Span::new(start, self.pos));
      self.tokens.push(tok);
    }
  }

  /// Lex everything between the tag name and the closing `>` / `/>`.
  ///
  /// Pushes whitespace, attribute-name identifiers, `=`, attribute-value
  /// literals, and finally the close token.
  fn lex_tag_internals(&mut self) {
    loop {
      let start = self.pos;
      let Some(b) = self.peek_byte() else {
        // EOF inside a tag — recover silently; the parser will error.
        self.errors.push(OxcDiagnostic::error("eof-in-tag").with_label(Span::new(start, start)));
        return;
      };

      // Whitespace.
      if matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0C) {
        while matches!(self.peek_byte(), Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0C)) {
          self.pos += 1;
        }
        let tok = VToken::new(VTokenKind::HTMLWhitespace, Span::new(start, self.pos));
        self.tokens.push(tok);
        continue;
      }

      // `/>`
      if b == b'/' && self.peek_byte_at(1) == Some(b'>') {
        self.pos += 2;
        self.emit(VTokenKind::HTMLSelfClosingTagClose, start);
        return;
      }

      // `>`
      if b == b'>' {
        self.pos += 1;
        self.emit(VTokenKind::HTMLTagClose, start);
        return;
      }

      // `=`
      if b == b'=' {
        self.pos += 1;
        self.emit(VTokenKind::HTMLAssociation, start);
        continue;
      }

      // Quoted attribute value.
      if b == b'"' || b == b'\'' {
        self.lex_quoted_value(b);
        continue;
      }

      // Attribute / directive name (incl. `:foo`, `@foo`, `#foo`, `.foo`,
      // `v-foo:bar.mod`). We stop at whitespace, `=`, `/`, `>` and emit
      // the run as a single identifier token. The parser splits the
      // structure further; we only need the boundary tokens to match
      // vue-eslint-parser's stream.
      if b == b':' || b == b'@' || b == b'#' || b == b'.' {
        // Emit the prefix as Punctuator, then continue with the name.
        self.pos += 1;
        self.emit(VTokenKind::Punctuator, start);
        self.lex_attribute_name();
        continue;
      }

      // Stray `/` (not `/>`) — skip; HTML5 treats this as a parse error.
      if b == b'/' {
        self.pos += 1;
        self.errors.push(
          OxcDiagnostic::error("unexpected-solidus-in-tag").with_label(Span::new(start, self.pos)),
        );
        continue;
      }

      // Anything else — start of an attribute name.
      self.lex_attribute_name();
    }
  }

  fn lex_attribute_name(&mut self) {
    let start = self.pos;
    while let Some(c) = self.peek_byte() {
      if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0C | b'=' | b'>' | b'/' | b'"' | b'\'') {
        break;
      }
      // Directive separators end the current name segment so the parser
      // can re-lex them as `Punctuator` on the next round.
      if c == b':' || c == b'.' {
        break;
      }
      self.pos += 1;
    }
    if self.pos > start {
      let tok = VToken::new(VTokenKind::HTMLIdentifier, Span::new(start, self.pos));
      self.tokens.push(tok);
    }
  }

  /// Lex a quoted attribute value run starting at the opening quote.
  ///
  /// Emits a single [`VTokenKind::HTMLLiteral`] covering the *content*
  /// (excluding quotes) — matching `vue-eslint-parser`. The quote bytes
  /// are still consumed but produce no separate token; the parser tracks
  /// quote positions via the surrounding spans.
  fn lex_quoted_value(&mut self, quote: u8) {
    debug_assert_eq!(self.peek_byte(), Some(quote));
    self.pos += 1;
    let value_start = self.pos;
    let value_end = memchr::memchr(quote, &self.source[self.pos as usize..])
      .map_or(self.source.len(), |i| self.pos as usize + i);
    self.pos = u32::try_from(value_end).unwrap_or(u32::MAX);
    if self.pos > value_start {
      let tok = VToken::new(VTokenKind::HTMLLiteral, Span::new(value_start, self.pos));
      self.tokens.push(tok);
    }
    if self.peek_byte() == Some(quote) {
      self.pos += 1;
    } else {
      self.errors.push(
        OxcDiagnostic::error("eof-in-attribute-value").with_label(Span::new(value_start, self.pos)),
      );
    }
  }

  /// Lex `<!-- ... -->` and emit a single comment token.
  fn lex_comment(&mut self, start: u32) -> VToken {
    debug_assert!(self.starts_with(b"<!--"));
    self.pos += 4;
    let body = &self.source[self.pos as usize..];
    let end_off = memchr::memmem::find(body, b"-->");
    let end = end_off.map_or(self.source.len(), |i| self.pos as usize + i + 3);
    self.pos = u32::try_from(end).unwrap_or(u32::MAX);
    if end_off.is_none() {
      self
        .errors
        .push(OxcDiagnostic::error("eof-in-comment").with_label(Span::new(start, self.pos)));
    }
    self.emit(VTokenKind::HTMLComment, start)
  }

  /// Lex a bogus comment starting at `<`. Terminated at the next `>` or EOF.
  fn lex_bogus_comment(&mut self, start: u32) -> VToken {
    self.pos += 1;
    let end = memchr::memchr(b'>', &self.source[self.pos as usize..])
      .map_or(self.source.len(), |i| self.pos as usize + i + 1);
    self.pos = u32::try_from(end).unwrap_or(u32::MAX);
    self.emit(VTokenKind::HTMLBogusComment, start)
  }

  /// Lex a CDATA section. Caller ensures we're in foreign content.
  fn lex_cdata(&mut self, start: u32) -> VToken {
    debug_assert!(self.starts_with(b"<![CDATA["));
    self.pos += 9;
    let body = &self.source[self.pos as usize..];
    let end =
      memchr::memmem::find(body, b"]]>").map_or(self.source.len(), |i| self.pos as usize + i + 3);
    self.pos = u32::try_from(end).unwrap_or(u32::MAX);
    self.emit(VTokenKind::HTMLCDataText, start)
  }

  /// Raw-text mode body lexer: scans up to the configured close tag.
  fn lex_raw_text(&mut self) -> Option<VToken> {
    let start = self.pos;
    let close = self.raw_close_tag.unwrap_or("");
    let body_end = if close.is_empty() {
      self.source.len()
    } else {
      find_close_tag(&self.source[self.pos as usize..], close.as_bytes())
        .map_or(self.source.len(), |i| self.pos as usize + i)
    };
    if body_end > self.pos as usize {
      self.pos = u32::try_from(body_end).unwrap_or(u32::MAX);
      return Some(self.emit(VTokenKind::HTMLRawText, start));
    }
    // Body is empty — fall through to lex `</tag>` immediately.
    self.lex_data()
  }

  /// RCDATA mode body lexer: same as raw text for tokenisation purposes,
  /// only differs at the entity-decoding stage which lives outside the
  /// lexer.
  fn lex_rcdata(&mut self) -> Option<VToken> {
    let start = self.pos;
    let close = self.raw_close_tag.unwrap_or("");
    let body_end = if close.is_empty() {
      self.source.len()
    } else {
      find_close_tag(&self.source[self.pos as usize..], close.as_bytes())
        .map_or(self.source.len(), |i| self.pos as usize + i)
    };
    if body_end > self.pos as usize {
      self.pos = u32::try_from(body_end).unwrap_or(u32::MAX);
      return Some(self.emit(VTokenKind::HTMLRCDataText, start));
    }
    self.lex_data()
  }

  /// Convenience: lex an entire raw-text body terminated by `</close_tag>`
  /// (case-insensitive) and return the body span. Does not consume the
  /// closing tag itself; the parser flips back to [`LexerMode::Data`] and
  /// processes it normally.
  pub fn lex_raw_text_until(&mut self, close_tag: &str) -> Span {
    let start = self.pos;
    let end = find_close_tag(&self.source[self.pos as usize..], close_tag.as_bytes())
      .map_or(self.source.len(), |i| self.pos as usize + i);
    self.pos = u32::try_from(end).unwrap_or(u32::MAX);
    Span::new(start, self.pos)
  }

  /// Take all tokens collected so far, leaving the lexer empty.
  pub fn take_tokens(&mut self) -> ArenaVec<'a, VToken> {
    std::mem::replace(&mut self.tokens, ArenaVec::new_in(self.allocator))
  }

  /// Drain all errors collected so far.
  pub fn take_errors(&mut self) -> Vec<OxcDiagnostic> {
    std::mem::take(&mut self.errors)
  }

  // ---- low-level byte helpers ----

  fn peek_byte(&self) -> Option<u8> {
    self.source.get(self.pos as usize).copied()
  }

  fn peek_byte_at(&self, offset: usize) -> Option<u8> {
    self.source.get(self.pos as usize + offset).copied()
  }

  fn starts_with(&self, needle: &[u8]) -> bool {
    self
      .source
      .get(self.pos as usize..self.pos as usize + needle.len())
      .is_some_and(|s| s == needle)
  }

  /// Build a token spanning `start..self.pos`, push it to the buffer, and
  /// return it. Helpers should always go through this so source-order is
  /// preserved.
  fn emit(&mut self, kind: VTokenKind, start: u32) -> VToken {
    let tok = VToken::new(kind, Span::new(start, self.pos));
    self.tokens.push(tok);
    tok
  }
}

/// Find the byte offset of `</close_tag` in `haystack`, matching the tag name
/// case-insensitively. Returns `None` if not found.
///
/// HTML5 requires the character following the close-tag name to be
/// whitespace, `>`, or `/` for it to terminate raw-text — otherwise the
/// `<` is treated as data.
fn find_close_tag(haystack: &[u8], close_tag: &[u8]) -> Option<usize> {
  let mut search_from = 0;
  while let Some(off) = memchr::memchr(b'<', &haystack[search_from..]) {
    let i = search_from + off;
    let after_lt = i + 1;
    if haystack.get(after_lt) != Some(&b'/') {
      search_from = after_lt;
      continue;
    }
    let name_start = after_lt + 1;
    let name_end = name_start + close_tag.len();
    let name_slice = haystack.get(name_start..name_end)?;
    if !name_slice.eq_ignore_ascii_case(close_tag) {
      search_from = after_lt;
      continue;
    }
    let terminator = haystack.get(name_end).copied();
    if matches!(terminator, Some(b' ' | b'\t' | b'\n' | b'\r' | 0x0C | b'>' | b'/') | None) {
      return Some(i);
    }
    search_from = after_lt;
  }
  None
}

#[cfg(test)]
mod tests {
  use super::*;

  fn kinds(src: &str) -> Vec<VTokenKind> {
    let alloc = Allocator::default();
    let mut lex = Lexer::new(&alloc, src);
    lex.lex_all();
    lex.tokens.iter().map(|t| t.kind).collect()
  }

  fn slices<'a>(src: &'a str, alloc: &'a Allocator) -> Vec<(VTokenKind, &'a str)> {
    let mut lex = Lexer::new(alloc, src);
    lex.lex_all();
    lex.tokens.iter().map(|t| (t.kind, &src[t.span.start as usize..t.span.end as usize])).collect()
  }

  #[test]
  fn plain_text() {
    assert_eq!(kinds("hello"), vec![VTokenKind::HTMLText]);
  }

  #[test]
  fn simple_start_tag() {
    let alloc = Allocator::default();
    assert_eq!(
      slices("<div>", &alloc),
      vec![
        (VTokenKind::HTMLTagOpen, "<"),
        (VTokenKind::HTMLIdentifier, "div"),
        (VTokenKind::HTMLTagClose, ">"),
      ],
    );
  }

  #[test]
  fn self_closing() {
    let alloc = Allocator::default();
    assert_eq!(
      slices("<br/>", &alloc),
      vec![
        (VTokenKind::HTMLTagOpen, "<"),
        (VTokenKind::HTMLIdentifier, "br"),
        (VTokenKind::HTMLSelfClosingTagClose, "/>"),
      ],
    );
  }

  #[test]
  fn end_tag() {
    let alloc = Allocator::default();
    assert_eq!(
      slices("</div>", &alloc),
      vec![
        (VTokenKind::HTMLEndTagOpen, "</"),
        (VTokenKind::HTMLIdentifier, "div"),
        (VTokenKind::HTMLTagClose, ">"),
      ],
    );
  }

  #[test]
  fn attribute_quoted() {
    let alloc = Allocator::default();
    assert_eq!(
      slices(r#"<a href="x">"#, &alloc),
      vec![
        (VTokenKind::HTMLTagOpen, "<"),
        (VTokenKind::HTMLIdentifier, "a"),
        (VTokenKind::HTMLWhitespace, " "),
        (VTokenKind::HTMLIdentifier, "href"),
        (VTokenKind::HTMLAssociation, "="),
        (VTokenKind::HTMLLiteral, "x"),
        (VTokenKind::HTMLTagClose, ">"),
      ],
    );
  }

  #[test]
  fn directive_shorthand() {
    let alloc = Allocator::default();
    let toks = slices(r#"<x :foo="1" @bar="2" #s />"#, &alloc);
    let kinds: Vec<_> = toks.iter().map(|(k, _)| *k).collect();
    assert_eq!(
      kinds,
      vec![
        VTokenKind::HTMLTagOpen,
        VTokenKind::HTMLIdentifier, // x
        VTokenKind::HTMLWhitespace,
        VTokenKind::Punctuator,     // :
        VTokenKind::HTMLIdentifier, // foo
        VTokenKind::HTMLAssociation,
        VTokenKind::HTMLLiteral,
        VTokenKind::HTMLWhitespace,
        VTokenKind::Punctuator,     // @
        VTokenKind::HTMLIdentifier, // bar
        VTokenKind::HTMLAssociation,
        VTokenKind::HTMLLiteral,
        VTokenKind::HTMLWhitespace,
        VTokenKind::Punctuator,     // #
        VTokenKind::HTMLIdentifier, // s
        VTokenKind::HTMLWhitespace,
        VTokenKind::HTMLSelfClosingTagClose,
      ],
    );
  }

  #[test]
  fn interpolation() {
    let alloc = Allocator::default();
    assert_eq!(
      slices("a {{ x }} b", &alloc),
      vec![
        (VTokenKind::HTMLText, "a "),
        (VTokenKind::VExpressionStart, "{{"),
        (VTokenKind::HTMLText, " x "),
        (VTokenKind::VExpressionEnd, "}}"),
        (VTokenKind::HTMLText, " b"),
      ],
    );
  }

  #[test]
  fn interpolation_off_in_v_pre() {
    let alloc = Allocator::default();
    let mut lex = Lexer::new(&alloc, "{{ x }}");
    lex.set_mode(LexerMode::VPre);
    lex.lex_all();
    let kinds: Vec<_> = lex.tokens.iter().map(|t| t.kind).collect();
    assert_eq!(kinds, vec![VTokenKind::HTMLText]);
  }

  #[test]
  fn html_comment() {
    let alloc = Allocator::default();
    assert_eq!(slices("<!-- hi -->", &alloc), vec![(VTokenKind::HTMLComment, "<!-- hi -->")],);
  }

  #[test]
  fn unterminated_comment_recovers() {
    let alloc = Allocator::default();
    let mut lex = Lexer::new(&alloc, "<!-- oops");
    lex.lex_all();
    assert_eq!(
      lex.tokens.iter().map(|t| t.kind).collect::<Vec<_>>(),
      vec![VTokenKind::HTMLComment],
    );
    assert_eq!(lex.errors.len(), 1);
  }

  #[test]
  fn raw_text_mode_script() {
    let alloc = Allocator::default();
    let src = "let x = 1 < 2; if (x) { /* </ noise */ }</script>";
    let mut lex = Lexer::new(&alloc, src);
    lex.set_mode(LexerMode::RawText);
    lex.set_raw_close_tag(Some("script"));
    lex.next_token().unwrap(); // raw text body
    let body = lex.tokens[0];
    assert_eq!(body.kind, VTokenKind::HTMLRawText);
    assert_eq!(
      &src[body.span.start as usize..body.span.end as usize],
      "let x = 1 < 2; if (x) { /* </ noise */ }",
    );
    // Switch back to data so the closing tag tokenises normally.
    lex.set_mode(LexerMode::Data);
    lex.set_raw_close_tag(None);
    lex.lex_all();
    let later: Vec<_> = lex.tokens.iter().skip(1).map(|t| t.kind).collect();
    assert_eq!(
      later,
      vec![VTokenKind::HTMLEndTagOpen, VTokenKind::HTMLIdentifier, VTokenKind::HTMLTagClose,],
    );
  }

  #[test]
  fn raw_text_close_tag_case_insensitive() {
    let alloc = Allocator::default();
    let mut lex = Lexer::new(&alloc, "body</STYLE>");
    lex.set_mode(LexerMode::RawText);
    lex.set_raw_close_tag(Some("style"));
    lex.next_token().unwrap();
    assert_eq!(lex.tokens[0].kind, VTokenKind::HTMLRawText);
    assert_eq!(
      &"body</STYLE>"[lex.tokens[0].span.start as usize..lex.tokens[0].span.end as usize],
      "body",
    );
  }

  #[test]
  fn raw_text_does_not_match_partial_tag() {
    let alloc = Allocator::default();
    let mut lex = Lexer::new(&alloc, "x</scripty></script>");
    lex.set_mode(LexerMode::RawText);
    lex.set_raw_close_tag(Some("script"));
    lex.next_token().unwrap();
    let body =
      &"x</scripty></script>"[lex.tokens[0].span.start as usize..lex.tokens[0].span.end as usize];
    assert_eq!(body, "x</scripty>");
  }

  #[test]
  fn rcdata_textarea() {
    let alloc = Allocator::default();
    let mut lex = Lexer::new(&alloc, "hi</textarea>");
    lex.set_mode(LexerMode::RcData);
    lex.set_raw_close_tag(Some("textarea"));
    lex.next_token().unwrap();
    assert_eq!(lex.tokens[0].kind, VTokenKind::HTMLRCDataText);
  }

  #[test]
  fn cdata_only_in_foreign() {
    let alloc = Allocator::default();
    // In the data state CDATA is a bogus comment.
    let mut lex = Lexer::new(&alloc, "<![CDATA[hi]]>");
    lex.lex_all();
    assert_eq!(lex.tokens[0].kind, VTokenKind::HTMLBogusComment);

    // In foreign content it's recognised.
    let mut lex = Lexer::new(&alloc, "<![CDATA[hi]]>");
    lex.set_mode(LexerMode::Foreign);
    lex.lex_all();
    assert_eq!(lex.tokens[0].kind, VTokenKind::HTMLCDataText);
  }

  #[test]
  fn stray_lt_is_text() {
    let alloc = Allocator::default();
    assert_eq!(
      slices("a < b", &alloc),
      vec![(VTokenKind::HTMLText, "a "), (VTokenKind::HTMLText, "<"), (VTokenKind::HTMLText, " b"),],
    );
  }

  #[test]
  fn bogus_comment_questionmark() {
    let alloc = Allocator::default();
    assert_eq!(slices("<?xml?>", &alloc), vec![(VTokenKind::HTMLBogusComment, "<?xml?>")],);
  }
}
