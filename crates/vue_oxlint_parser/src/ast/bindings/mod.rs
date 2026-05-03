//! Structs defined in this mod aren't V* nodes, it is just a helper struct to store binding-related things.
//!
//! For these structs, we should always use `'b` lifetime and Box, to avoid cloning nodes

use oxc_allocator::Box;
use oxc_ast::ast::IdentifierReference;

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
