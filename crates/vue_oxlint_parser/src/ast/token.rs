use crate::lexer::VToken;
use oxc_estree::{ESTree, Serializer};

#[derive(Debug)]
pub enum SerializableToken {
  VToken(VToken),
  /// It should be already serialized, it is a part of a JSON array
  /// e.g. `{"type":"Identifier","start": 12,"end":13}`
  ///
  /// (serilizing oxc tokens is really really hard, need a lot of metadatas)
  OxcToken(String),
}

impl From<VToken> for SerializableToken {
  fn from(value: VToken) -> Self {
    Self::VToken(value)
  }
}

impl From<String> for SerializableToken {
  fn from(value: String) -> Self {
    Self::OxcToken(value)
  }
}

/// For internal uses, usually we won't use this struct directly
/// Only calls this in a `ArenaVec<'_, SerializableToken>`
impl ESTree for SerializableToken {
  fn serialize<S: Serializer>(&self, mut serializer: S) {
    match self {
      Self::OxcToken(string) if !string.is_empty() => {
        serializer.buffer_mut().print_str(format!(",{string}"));
      }
      Self::VToken(token) => {
        todo!()
      }
      _ => (),
    }
  }
}
