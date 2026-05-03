//! Tokens emitted by the Vue template lexer.
//!
//! The variant set mirrors `vue-eslint-parser`'s intermediate-tokenizer output
//! so that the toolkit's `ESTree` adapter can include them in `Program.tokens`
//! verbatim. Token spans are in original SFC byte-offset space.

use oxc_span::Span;

/// A single template-side token.
#[derive(Debug, Clone, Copy)]
pub struct VToken {
  pub kind: VTokenKind,
  pub span: Span,
}

impl VToken {
  #[must_use]
  pub const fn new(kind: VTokenKind, span: Span) -> Self {
    Self { kind, span }
  }
}

/// Template-side token kinds.
///
/// Names mirror `vue-eslint-parser`'s `Token["type"]` strings: when the
/// adapter on the toolkit side serialises tokens it can map these 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VTokenKind {
  /// `<` — opening of a start tag, immediately followed by an
  /// [`HTMLIdentifier`](Self::HTMLIdentifier) for the tag name.
  HTMLTagOpen,
  /// `</` — opening of an end tag.
  HTMLEndTagOpen,
  /// `>` — closing of a start or end tag.
  HTMLTagClose,
  /// `/>` — self-closing tag terminator.
  HTMLSelfClosingTagClose,
  /// A tag name or attribute / directive name.
  HTMLIdentifier,
  /// `=` between an attribute name and value.
  HTMLAssociation,
  /// A quoted or unquoted attribute value (content only, quotes excluded
  /// — quote characters live as part of the surrounding span on the parser
  /// side, matching `vue-eslint-parser`).
  HTMLLiteral,
  /// Whitespace inside a tag (between attributes, around `=`, etc.).
  HTMLWhitespace,
  /// Plain text run in the data state.
  HTMLText,
  /// Body of an `<![CDATA[ ... ]]>` section (foreign content only).
  HTMLCDataText,
  /// Body of a `<script>` / `<style>` / `<xmp>` element — the raw text mode.
  HTMLRawText,
  /// Body of a `<textarea>` / `<title>` element — the RCDATA mode.
  HTMLRCDataText,
  /// `<!-- ... -->` — single token covering open, body, and close.
  HTMLComment,
  /// `<!foo>` / `</...>` malformed — single bogus-comment token.
  HTMLBogusComment,
  /// `{{` — opening of a Vue interpolation.
  VExpressionStart,
  /// `}}` — closing of a Vue interpolation.
  VExpressionEnd,
  /// `:`, `.`, `#`, `@`, `*` — directive shorthand / separator punctuation.
  Punctuator,
}
