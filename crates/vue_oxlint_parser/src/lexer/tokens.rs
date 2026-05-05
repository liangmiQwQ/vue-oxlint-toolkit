use oxc_estree::{ESTree, Serializer, StructSerializer};
use oxc_span::Span;

/// A single template-side token.
#[derive(Debug, Clone, Copy)]
pub struct VToken<'b> {
  pub kind: VTokenKind,
  pub span: Span,
  pub value: Option<&'b str>,
}

impl ESTree for VToken<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", self.kind.as_str());
    state.serialize_field("value", &self.value());
    if self.kind == VTokenKind::Punctuator {
      state.serialize_field("__templatePunctuator", &true);
    }
    state.serialize_span(self.span);
    state.end();
  }
}

impl<'b> VToken<'b> {
  #[must_use]
  pub const fn new(kind: VTokenKind, span: Span, value: Option<&'b str>) -> Self {
    Self { kind, span, value }
  }

  const fn value(&self) -> Option<&str> {
    match (self.kind, self.value) {
      (
        VTokenKind::HTMLAssociation
        | VTokenKind::HTMLTagClose
        | VTokenKind::HTMLSelfClosingTagClose,
        None,
      ) => Some(""),
      (VTokenKind::VExpressionStart, None) => Some("{{"),
      (VTokenKind::VExpressionEnd, None) => Some("}}"),
      _ => self.value,
    }
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
  /// `<!-- ... -->`
  HTMLComment,
  /// bogus declaration / comment text
  HTMLBogusComment,

  // Directive Related
  /// e.g. `:`, `@`, `#` of (:class @click #default)
  Punctuator,
}

impl VTokenKind {
  const fn as_str(&self) -> &str {
    match self {
      Self::HTMLTagOpen => "HTMLTagOpen",
      Self::HTMLTagClose => "HTMLTagClose",
      Self::HTMLEndTagOpen => "HTMLEndTagOpen",
      Self::HTMLSelfClosingTagClose => "HTMLSelfClosingTagClose",
      Self::HTMLIdentifier => "HTMLIdentifier",
      Self::HTMLAssociation => "HTMLAssociation",
      Self::HTMLLiteral => "HTMLLiteral",
      Self::HTMLText => "HTMLText",
      Self::HTMLWhitespace => "HTMLWhitespace",
      Self::HTMLRCDataText => "HTMLRCDataText",
      Self::HTMLRawText => "HTMLRawText",
      Self::HTMLCDataText => "HTMLCDataText",
      Self::VExpressionStart => "VExpressionStart",
      Self::VExpressionEnd => "VExpressionEnd",
      Self::HTMLComment => "HTMLComment",
      Self::HTMLBogusComment => "HTMLBogusComment",
      Self::Punctuator => "Punctuator",
    }
  }
}
