use oxc_ast::ast::IdentifierReference;

/// This is not a Vue node type, it is just a helper struct to store references.
#[derive(Debug)]
pub struct Reference<'a> {
  pub id: IdentifierReference<'a>,
  pub mode: &'static str,
}
