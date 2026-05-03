//! Structs defined in this mod aren't V* nodes, it is just a helper struct to store binding-related things.
//!
//! Although we use two-allocator design, we still should use 'a here, as its a clone of the identifier reference

use oxc_ast::ast::IdentifierReference;

#[derive(Debug)]
pub struct Reference<'a> {
  pub id: IdentifierReference<'a>,
  pub mode: &'static str,
}

#[derive(Debug)]
pub struct Variable<'a> {
  pub id: IdentifierReference<'a>,
  pub kind: &'static str,
}
