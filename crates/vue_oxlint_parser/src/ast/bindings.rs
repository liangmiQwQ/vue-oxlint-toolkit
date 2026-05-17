//! Structs defined in this mod aren't V* nodes, it is just a helper struct to store binding-related things.
//!
//! For these structs, we should always use `'b` lifetime and Box, to avoid cloning nodes
use oxc_allocator::{Allocator, Vec};
use oxc_ast::ast::{BindingIdentifier, Expression, FormalParameters, IdentifierReference};
use oxc_ast_visit::Visit;
use oxc_estree::{ESTree, StructSerializer};

#[derive(Debug)]
pub struct Reference<'b> {
  pub id: &'b IdentifierReference<'b>,
  pub mode: &'static str,
}

#[derive(Debug)]
pub struct Variable<'b> {
  pub id: &'b BindingIdentifier<'b>,
  pub kind: &'static str,
}

impl ESTree for Reference<'_> {
  fn serialize<S: oxc_estree::Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("id", &self.id);
    state.serialize_field("mode", &self.mode);
    state.serialize_field("isValueReference", &true);
    state.serialize_field("isTypeReference", &false);
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

pub fn collect_expression_references<'store, 'ast>(
  allocator: &'store Allocator,
  expression: &Expression<'ast>,
) -> Vec<'store, Reference<'ast>> {
  let mut collector = ReferenceCollector { references: Vec::new_in(allocator) };
  collector.visit_expression(expression);
  collector.references
}

pub fn collect_parameter_variables<'store, 'ast>(
  allocator: &'store Allocator,
  params: &FormalParameters<'ast>,
  kind: &'static str,
) -> Vec<'store, Variable<'ast>> {
  let mut collector = VariableCollector { variables: Vec::new_in(allocator), kind };
  collector.visit_formal_parameters(params);
  collector.variables
}

struct ReferenceCollector<'store, 'ast> {
  references: Vec<'store, Reference<'ast>>,
}

impl<'ast> Visit<'ast> for ReferenceCollector<'_, 'ast> {
  fn visit_identifier_reference(&mut self, ident: &IdentifierReference<'ast>) {
    let id = self.alloc(ident);
    self.references.push(Reference { id, mode: "r" });
  }
}

struct VariableCollector<'store, 'ast> {
  variables: Vec<'store, Variable<'ast>>,
  kind: &'static str,
}

impl<'ast> Visit<'ast> for VariableCollector<'_, 'ast> {
  fn visit_binding_identifier(&mut self, ident: &BindingIdentifier<'ast>) {
    let id = self.alloc(ident);
    self.variables.push(Variable { id, kind: self.kind });
  }
}
