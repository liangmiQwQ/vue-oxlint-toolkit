//! Tokens emitted by the Vue template lexer.
//!
//! The intent is to be compatible with `vue-eslint-parser`'s token output, so
//! that the toolkit's `ESTree` adapter can include them in `Program.tokens`.
//!
//! Phase 2 of the RFC will flesh this out and implement the lexer that
//! produces it.

use oxc_span::Span;

/// A single template-side token.
///
/// Spans are in original SFC byte-offset space.
//
// NOTE: This is a placeholder shape. Phase 2 will:
//   - finalise the variant set against `vue-eslint-parser`'s token kinds
//   - decide whether to pack into `u128` like `oxc_parser::Token` for parity
//     with the script-side tokens
#[derive(Debug, Clone, Copy)]
pub struct VToken {
  pub kind: VTokenKind,
  pub span: Span,
}

/// Template-side token kinds.
///
/// This enum is intentionally not yet exhaustive — phase 2 will expand it to
/// match `vue-eslint-parser`'s token kinds (the `HTML*` family, plus
/// `VExpressionStart` / `VExpressionEnd`, `Punctuator`, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VTokenKind {
  /// Placeholder for the not-yet-defined variants.
  Placeholder,
}
