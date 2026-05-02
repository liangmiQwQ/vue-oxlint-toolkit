//! Vue template lexer.
//!
//! Phase 2 of the RFC will implement this. For now the module exposes only
//! the [`VToken`] / [`VTokenKind`] shapes and a [`Lexer`] skeleton whose
//! methods are all `todo!()`.
//!
//! The lexer will be HTML5-aware (raw-text for `<script>`, `<style>`,
//! `<textarea>`, `<title>`; foreign content for `<svg>`, `<math>`) and will
//! honour `v-pre`, mirroring `vue-eslint-parser`.

mod tokens;

pub use tokens::{VToken, VTokenKind};

use oxc_allocator::{Allocator, Vec as ArenaVec};
use oxc_diagnostics::OxcDiagnostic;
use oxc_span::Span;

/// HTML5 tokenizer modes.
///
/// The lexer switches between these as it crosses element boundaries (e.g.
/// entering `<script>` switches to [`LexerMode::RawText`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LexerMode {
  /// Default mode — recognises tags, comments, character references, etc.
  Data,
  /// `<script>`, `<style>`, `<xmp>`, etc. — only the closing tag terminates.
  RawText,
  /// `<textarea>`, `<title>` — character references resolved, but no tags.
  RcData,
  /// `<svg>`, `<math>` — foreign content rules (CDATA allowed, etc.).
  Foreign,
  /// Inside a `v-pre` subtree — directives are not recognised, but tags are.
  VPre,
}

/// Vue template lexer.
///
/// The lexer produces [`VToken`]s in source order, with spans in the original
/// SFC byte-offset space.
pub struct Lexer<'a> {
  allocator: &'a Allocator,
  #[expect(dead_code, reason = "phase 2 will scan this")]
  source_text: &'a str,
  mode: LexerMode,
  tokens: ArenaVec<'a, VToken>,
  errors: Vec<OxcDiagnostic>,
}

impl<'a> Lexer<'a> {
  #[must_use]
  pub fn new(allocator: &'a Allocator, source_text: &'a str) -> Self {
    Self {
      allocator,
      source_text,
      mode: LexerMode::Data,
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

  /// Advance the lexer by one token, pushing it into the internal buffer.
  ///
  /// Returns the token kind for the parser's lookahead, or `None` at EOF.
  pub fn next_token(&mut self) -> Option<VToken> {
    todo!("phase 2: implement template tokenization")
  }

  /// Lex a contiguous run of raw text terminated by `</{tag}>` (case-insensitive).
  ///
  /// Used for `<script>` / `<style>` bodies, where the parser will hand off
  /// the resulting span to `oxc_parser` rather than tokenising the contents.
  pub fn lex_raw_text_until(&mut self, _close_tag: &str) -> Span {
    todo!("phase 2: implement raw-text scanning")
  }

  /// Consume and return all tokens collected so far, leaving the lexer empty.
  pub fn take_tokens(&mut self) -> ArenaVec<'a, VToken> {
    std::mem::replace(&mut self.tokens, ArenaVec::new_in(self.allocator))
  }

  /// Drain all errors collected so far.
  pub fn take_errors(&mut self) -> Vec<OxcDiagnostic> {
    std::mem::take(&mut self.errors)
  }
}
