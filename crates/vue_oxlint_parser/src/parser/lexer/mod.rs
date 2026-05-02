//! Low-level source scanner for the Vue SFC parser.
//!
//! This module contains byte-level scanning helpers that are used by the
//! higher-level element and attribute parsers. All methods are implemented
//! on `Parser` so that scanner state (`pos`, `source_text`) is shared with
//! the rest of the parser — the same pattern used by `vue_oxlint_jsx`.

use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span;

use crate::parser::Parser;

impl<'a> Parser<'a> {
  /// Current byte at `pos`, or `None` at EOF.
  #[must_use]
  pub fn current_byte(&self) -> Option<u8> {
    self.source_text.as_bytes().get(self.pos).copied()
  }

  /// Whether the scanner has consumed all input.
  #[must_use]
  pub const fn is_eof(&self) -> bool {
    self.pos >= self.source_text.len()
  }

  /// Peek `offset` bytes ahead of `pos` without advancing.
  #[must_use]
  pub fn peek_byte(&self, offset: usize) -> Option<u8> {
    self.source_text.as_bytes().get(self.pos + offset).copied()
  }

  /// Return `true` if `source_text[pos..]` starts with `s`.
  #[must_use]
  pub fn matches_at(&self, pos: usize, s: &str) -> bool {
    self.source_text.as_bytes().get(pos..pos + s.len()) == Some(s.as_bytes())
  }

  /// Return `true` if the current position starts with `s`.
  #[must_use]
  pub fn matches(&self, s: &str) -> bool {
    self.matches_at(self.pos, s)
  }

  /// Advance `pos` by `n` bytes.
  pub const fn advance(&mut self, n: usize) {
    self.pos += n;
  }

  /// Consume one byte and advance `pos`. Returns `None` at EOF.
  pub fn consume_byte(&mut self) -> Option<u8> {
    let b = self.current_byte();
    if b.is_some() {
      self.pos += 1;
    }
    b
  }

  /// Current position as `u32` (for building `Span`s).
  #[must_use]
  pub const fn pos_u32(&self) -> u32 {
    self.pos as u32
  }

  /// Return a `&str` slice `source_text[start..end]`.
  #[must_use]
  pub fn slice(&self, start: u32, end: u32) -> &'a str {
    &self.source_text[start as usize..end as usize]
  }

  /// Skip ASCII whitespace (`' '`, `'\t'`, `'\n'`, `'\r'`).
  pub fn skip_whitespace(&mut self) {
    while let Some(b) = self.current_byte() {
      if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
        self.pos += 1;
      } else {
        break;
      }
    }
  }

  /// Record a recoverable parse error.
  pub fn push_error(&mut self, err: OxcDiagnostic) {
    self.errors.push(err);
  }

  /// Return a `Span` from `start` to the current position.
  #[must_use]
  pub const fn span_from(&self, start: u32) -> Span {
    Span::new(start, self.pos_u32())
  }
}
