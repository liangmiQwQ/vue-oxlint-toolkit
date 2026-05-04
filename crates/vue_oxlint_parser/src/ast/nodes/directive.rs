//! All the directives defined there will be serialized into `VAttribute` struct with `{ directive: true }`

use crate::ast::nodes::{
  attribute::VIdentifier,
  javascript::{
    VDirectiveArgumentExpression, VDirectiveExpression, VForExpression, VOnExpression,
    VSlotExpression,
  },
};
use oxc_allocator::{Box, Vec};
use oxc_estree::{ESTree, JsonSafeString, Serializer, StructSerializer};
use oxc_span::Span;

/// For normal directives, like `v-bind`, `v-model`, `v-if`.
#[derive(Debug)]
pub struct VDirective<'a, 'b> {
  pub key: VDirectiveKey<'a, 'b>,
  pub value: VDirectiveExpression<'a, 'b>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VOnDirective<'a, 'b> {
  pub key: VDirectiveKey<'a, 'b>,
  pub value: VOnExpression<'b>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VSlotDirective<'a, 'b> {
  pub key: VDirectiveKey<'a, 'b>,
  pub value: VSlotExpression<'b>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VForDirective<'a, 'b> {
  pub key: VDirectiveKey<'a, 'b>,
  pub value: VForExpression<'b>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VDirectiveKey<'a, 'b> {
  pub name: &'a VIdentifier<'a>,
  pub argument: VDirectiveArgument<'a, 'b>,
  pub modifiers: Vec<'a, VIdentifier<'a>>,
  pub span: Span,
}

#[derive(Debug)]
pub enum VDirectiveArgument<'a, 'b> {
  VDirectiveArgument(Box<'a, VDirectiveArgumentExpression<'a, 'b>>),
  VIdentifier(Box<'a, VIdentifier<'a>>),
}

impl ESTree for VDirective<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VAttribute"));
    state.serialize_field("directive", &true);
    state.serialize_field("key", &self.key);
    state.serialize_field("value", &self.value);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VOnDirective<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VAttribute"));
    state.serialize_field("directive", &true);
    state.serialize_field("key", &self.key);
    state.serialize_field("value", &self.value);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VSlotDirective<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VAttribute"));
    state.serialize_field("directive", &true);
    state.serialize_field("key", &self.key);
    state.serialize_field("value", &self.value);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VForDirective<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VAttribute"));
    state.serialize_field("directive", &true);
    state.serialize_field("key", &self.key);
    state.serialize_field("value", &self.value);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VDirectiveKey<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("name", &self.name);
    state.serialize_field("argument", &self.argument);
    state.serialize_field("modifiers", &self.modifiers);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VDirectiveArgument<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    match self {
      VDirectiveArgument::VDirectiveArgument(expr) => expr.serialize(serializer),
      VDirectiveArgument::VIdentifier(ident) => ident.serialize(serializer),
    }
  }
}
