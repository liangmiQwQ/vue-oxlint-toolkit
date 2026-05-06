//! Structs defined in this mod aren't V* nodes, it is just a helper struct to store binding-related things.
//!
//! For these structs, we should always use `'b` lifetime and Box, to avoid cloning nodes
use oxc_estree::{ESTree, JsonSafeString, StructSerializer};
use oxc_span::Span;

#[derive(Debug)]
pub struct Reference<'b> {
  pub name: &'b str,
  pub span: Span,
  pub mode: &'static str,
}

#[derive(Debug)]
pub struct Variable<'b> {
  pub name: &'b str,
  pub span: Span,
  pub kind: &'static str,
}

/// Private struct, avoid useless fields being appended into Reference and Variable
#[derive(Debug)]
struct Identifier<'b> {
  name: &'b str,
  span: Span,
}

impl ESTree for Identifier<'_> {
  fn serialize<S: oxc_estree::Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("Identifier"));
    state.serialize_field("name", &self.name);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for Reference<'_> {
  fn serialize<S: oxc_estree::Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("id", &Identifier { name: self.name, span: self.span });
    state.serialize_field("mode", &self.mode);
    state.end();
  }
}

impl ESTree for Variable<'_> {
  fn serialize<S: oxc_estree::Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("id", &Identifier { name: self.name, span: self.span });
    state.serialize_field("kind", &self.kind);
    state.end();
  }
}
