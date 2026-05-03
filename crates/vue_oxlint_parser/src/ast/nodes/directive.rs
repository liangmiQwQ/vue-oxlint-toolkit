//! All the directives defined there will be serialized into `VAttribute` struct with `{ directive: true }`

use crate::ast::nodes::{
  attribute::VIdentifier,
  javascript::{
    VDirectiveArgumentExpression, VDirectiveExpression, VForExpression, VOnExpression,
    VSlotExpression,
  },
};
use oxc_allocator::{Box, Vec};
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
