use crate::lexer::VToken;
use oxc_estree::{ESTree, Serializer};

/// Two lifetime
/// - `'a` is for the lifetime of the `vue_allocator`, we storage the serialized oxc tokens in it (to make `js_allocator` clean)
/// - `'b` is for the lifetime of the `VToken`'s value reference (which is a slice of source text)
#[derive(Debug)]
pub(crate) enum SerializableToken<'a, 'b> {
  VToken(VToken<'b>),
  /// It should be already serialized, it is a part of a JSON array
  /// e.g. `{"type":"Identifier","start": 12,"end":13}`
  ///
  /// (serilizing oxc tokens is really really hard, need a lot of metadatas like program which is missing)
  /// Even if it is a script-related token, we still allocate it into `vue_allocator` to kee`js_allocator`or clean.
  OxcToken(&'a str),
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
    #[allow(clippy::match_wildcard_for_single_variants)]
    match self {
      Self::OxcToken(str) if !str.is_empty() => {
        serializer.buffer_mut().print_str(str);
      }
      Self::VToken(token) => token.serialize(serializer),
      _ => (),
    }
  }
}
