use crate::lexer::VToken;
use oxc_estree::{ESTree, Serializer};

/// Two lifetime
/// - `'a` is for the lifetime of the `vue_allocator`, we storage the serialized oxc tokens in it (to make js_allocator clean)
/// - `'b` is for the lifetime of the VToken's value reference (which is a slice of source text)
#[derive(Debug)]
pub enum SerializableToken<'a, 'b> {
  VToken(VToken<'b>),
  /// It should be already serialized, it is a part of a JSON array
  /// e.g. `{"type":"Identifier","start": 12,"end":13}`
  ///
  /// (serilizing oxc tokens is really really hard, need a lot of metadatas like program which is missing)
  /// Even if it is a script-related token, we still allocate it into vue_allocator to keep js_allocator clean.
  OxcToken(&'a str),
}

impl<'a, 'b> From<VToken<'b>> for SerializableToken<'a, 'b> {
  fn from(value: VToken<'b>) -> Self {
    Self::VToken(value)
  }
}

impl<'a> From<&'a str> for SerializableToken<'a, '_> {
  fn from(value: &'a str) -> Self {
    Self::OxcToken(value.into())
  }
}

/// For internal uses, usually we won't use this struct directly
/// Only calls this in a `ArenaVec<'_, SerializableToken>`
impl ESTree for SerializableToken<'_, '_> {
  fn serialize<S: Serializer>(&self, mut serializer: S) {
    match self {
      Self::OxcToken(str) if !str.is_empty() => {
        serializer.buffer_mut().print_char(',');
        serializer.buffer_mut().print_str(str);
      }
      Self::VToken(token) => token.serialize(serializer),
      _ => (),
    }
  }
}
