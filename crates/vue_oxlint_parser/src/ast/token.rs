use oxc_estree::{ESTree, Serializer};
use oxc_estree_tokens::to_estree_tokens_json;
use oxc_parser::Token;

use crate::lexer::VToken;

#[derive(Debug)]
pub enum SerializableToken {
  VToken(VToken),
  OxcToken(Token),
}

impl From<VToken> for SerializableToken {
  fn from(value: VToken) -> Self {
    Self::VToken(value)
  }
}

impl From<Token> for SerializableToken {
  fn from(value: Token) -> Self {
    Self::OxcToken(value)
  }
}

impl ESTree for SerializableToken {
  fn serialize<S: Serializer>(&self, mut serializer: S) {
    match self {
      Self::OxcToken(token) => todo!(),
      Self::VToken(token) => {
        let mut buffer = serializer.buffer_mut();
        buffer.print_str(to_estree_tokens_json(
          tokens,
          program,
          source_text,
          span_converter,
          options,
        ))
      }
    }
  }
}
