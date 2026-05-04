use crate::lexer::VToken;
use oxc_estree::{ESTree, Serializer};

#[derive(Debug)]
pub enum SerializableToken<'b> {
  VToken(VToken<'b>),
  /// It should be already serialized, it is a part of a JSON array
  /// e.g. `{"type":"Identifier","start": 12,"end":13}`
  ///
  /// (serilizing oxc tokens is really really hard, need a lot of metadatas)
  OxcToken(&'b str),
}

impl<'b> From<VToken<'b>> for SerializableToken<'b> {
  fn from(value: VToken<'b>) -> Self {
    Self::VToken(value)
  }
}

impl<'a> From<&'a str> for SerializableToken<'a> {
  fn from(value: &'a str) -> Self {
    Self::OxcToken(value)
  }
}

/// For internal uses, usually we won't use this struct directly
/// Only calls this in a `ArenaVec<'_, SerializableToken>`
impl<'a> ESTree for SerializableToken<'a> {
  fn serialize<S: Serializer>(&self, mut serializer: S) {
    match self {
      Self::OxcToken(str) if !str.is_empty() => {
        serializer.buffer_mut().print_char(',');
        serializer.buffer_mut().print_str(*str);
      }
      Self::VToken(token) => token.serialize(serializer),
      _ => (),
    }
  }
}
