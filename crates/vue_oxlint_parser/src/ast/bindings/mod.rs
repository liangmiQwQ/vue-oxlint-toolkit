//! Structs defined in this mod aren't V* nodes, it is just a helper struct to store binding-related things.
//!
//! For these structs, we should always use `'b` lifetime and Box, to avoid cloning nodes

use oxc_allocator::Box;
use oxc_ast::ast::IdentifierReference;
use oxc_estree::{ESTree, StructSerializer};

#[derive(Debug)]
pub struct Reference<'b> {
  pub id: Box<'b, IdentifierReference<'b>>,
  pub mode: &'static str,
}

#[derive(Debug)]
pub struct Variable<'b> {
  pub id: Box<'b, IdentifierReference<'b>>,
  pub kind: &'static str,
}

impl ESTree for Reference<'_> {
  fn serialize<S: oxc_estree::Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("id", &self.id);
    state.serialize_field("mode", &self.mode);
    state.end();
  }
}

impl ESTree for Variable<'_> {
  fn serialize<S: oxc_estree::Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("id", &self.id);
    state.serialize_field("kind", &self.kind);
    state.end();
  }
}
