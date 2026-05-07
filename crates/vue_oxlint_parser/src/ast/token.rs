use crate::lexer::{VToken, VTokenKind};
use oxc_estree::{ESTree, Serializer};
use oxc_span::Span;

/// Two lifetime
/// - `'a` is for the lifetime of the `vue_allocator`, we storage the serialized oxc tokens in it (to make `js_allocator` clean)
/// - `'b` is for the lifetime of the `VToken`'s value reference (which is a slice of source text)
#[derive(Debug)]
pub(crate) enum SerializableToken<'a, 'b> {
  VToken(VToken<'b>),
  Synthetic(VToken<'static>),
  /// It should be already serialized, it is a part of a JSON array
  /// e.g. `{"type":"Identifier","start": 12,"end":13}`
  ///
  /// (serilizing oxc tokens is really really hard, need a lot of metadatas like program which is missing)
  /// Even if it is a script-related token, we still allocate it into `vue_allocator` to kee`js_allocator`or clean.
  OxcToken(&'a str),
}

impl SerializableToken<'_, '_> {
  #[must_use]
  pub(crate) const fn punctuator(span: Span, value: &'static str) -> Self {
    Self::Synthetic(VToken::new(VTokenKind::Punctuator, span, value))
  }

  #[must_use]
  pub(crate) const fn script_tag(span: Span) -> Self {
    Self::Synthetic(VToken::new(VTokenKind::Punctuator, span, "<script>"))
  }

  #[must_use]
  pub(crate) const fn script_end_tag(span: Span) -> Self {
    Self::Synthetic(VToken::new(VTokenKind::Punctuator, span, "</script>"))
  }
}

impl<'b> From<VToken<'b>> for SerializableToken<'_, 'b> {
  fn from(value: VToken<'b>) -> Self {
    Self::VToken(value)
  }
}

impl<'a> From<&'a str> for SerializableToken<'a, '_> {
  fn from(value: &'a str) -> Self {
    Self::OxcToken(value)
  }
}

/// For internal uses, usually we won't use this struct directly
/// Only calls this in a `ArenaVec<'_, SerializableToken>`
impl ESTree for SerializableToken<'_, '_> {
  fn serialize<S: Serializer>(&self, mut serializer: S) {
    #[allow(
      clippy::match_same_arms,
      clippy::match_wildcard_for_single_variants,
      reason = "Synthetic and source tokens carry different value lifetimes."
    )]
    match self {
      Self::OxcToken(str) if !str.is_empty() => {
        serializer.buffer_mut().print_str(str);
      }
      Self::Synthetic(token) => token.serialize(serializer),
      Self::VToken(token) => token.serialize(serializer),
      _ => (),
    }
  }
}
