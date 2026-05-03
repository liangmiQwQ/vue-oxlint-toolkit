use oxc_allocator::Box;
use oxc_span::Span;

use crate::ast::nodes::directive::{VDirective, VForDirective, VOnDirective, VSlotDirective};

#[derive(Debug)]
pub enum VAttribute<'a, 'b> {
  VPureAttribute(Box<'a, VPureAttribute<'a>>),
  VDirective(Box<'a, VDirective<'a, 'b>>),
  VOnDirective(Box<'a, VOnDirective<'a, 'b>>),
  VSlotDirective(Box<'a, VSlotDirective<'a, 'b>>),
  VForDirective(Box<'a, VForDirective<'a, 'b>>),
}

#[derive(Debug)]
pub struct VPureAttribute<'a> {
  pub key: VIdentifier<'a>,
  pub value: Option<VLiteral<'a>>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VIdentifier<'a> {
  pub name: &'a str,
  pub raw_name: &'a str,
  pub span: Span,
}

#[derive(Debug)]
pub struct VLiteral<'a> {
  pub value: &'a str,
  pub span: Span,
}
