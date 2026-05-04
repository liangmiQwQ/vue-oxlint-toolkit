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
  // HTML Tokens, which produce by lexer
  /// e.g. <
  HTMLTagOpen,
  /// e.g. >
  HTMLTagClose,
  /// e.g. </
  HTMLEndTagOpen,
  /// e.g. />
  HTMLSelfClosingTagClose,
  /// e.g. div, button
  HTMLIdentifier,
  /// e.g. =
  HTMLAssociation,
  /// e.g. "hello", 'hello'
  HTMLLiteral,
  /// Plain text
  HTMLText,
  /// Whitespace in template
  HTMLWhitespace,
  /// RCDATA text (e.g. <title> internal)
  HTMLRCDataText,
  /// Raw text (e.g. <script>、<style> internal)
  HTMLRawText,
  /// Data in <![CDATA[...]]>
  HTMLCDataText,
  /// {{
  VExpressionStart,
  /// }}
  VExpressionEnd,

  // Directive Related
  /// e.g. `:`, `@`, `#` of (:class @click #default)
  Punctuator,
}

impl VTokenKind {
  fn as_str(&self) -> &str {
    match self {
      VTokenKind::HTMLTagOpen => "HTMLTagOpen",
      VTokenKind::HTMLTagClose => "HTMLTagClose",
      VTokenKind::HTMLEndTagOpen => "HTMLEndTagOpen",
      VTokenKind::HTMLSelfClosingTagClose => "HTMLSelfClosingTagClose",
      VTokenKind::HTMLIdentifier => "HTMLIdentifier",
      VTokenKind::HTMLAssociation => "HTMLAssociation",
      VTokenKind::HTMLLiteral => "HTMLLiteral",
      VTokenKind::HTMLText => "HTMLText",
      VTokenKind::HTMLWhitespace => "HTMLWhitespace",
      VTokenKind::HTMLRCDataText => "HTMLRCDataText",
      VTokenKind::HTMLRawText => "HTMLRawText",
      VTokenKind::HTMLCDataText => "HTMLCDataText",
      VTokenKind::VExpressionStart => "VExpressionStart",
      VTokenKind::VExpressionEnd => "VExpressionEnd",
      VTokenKind::Punctuator => "Punctuator",
    }
  }
}
