use oxc_allocator::{Box, Vec};
use oxc_span::Span;

use crate::ast::{
  bindings::Variable,
  nodes::{
    attribute::VAttribute,
    javascript::{VInterpolation, VPureScript},
  },
};

#[derive(Debug)]
pub enum VNode<'a, 'b> {
  Element(Box<'a, VElement<'a, 'b>>),
  Text(Box<'a, VText<'a>>),
  Comment(Box<'a, VComment<'a>>),
  Interpolation(Box<'a, VInterpolation<'a, 'b>>),
  PureScript(Box<'a, VPureScript<'b>>),
}

#[derive(Debug)]
pub struct VElement<'a, 'b> {
  pub name: &'a str,
  pub raw_name: &'a str,
  pub start_tag: VStartTag<'a, 'b>,
  pub children: Vec<'a, VNode<'a, 'b>>,
  pub end_tag: Option<VEndTag>,
  pub variables: Vec<'a, Variable<'a>>,
  pub span: Span,
}

#[derive(Debug)]
pub struct VStartTag<'a, 'b> {
  pub attributes: Vec<'a, VAttribute<'a, 'b>>,
  pub self_closing: bool,
  pub span: Span,
}

#[derive(Debug)]
pub struct VEndTag {
  pub span: Span,
}

#[derive(Debug)]
pub struct VText<'a> {
  pub text: &'a str,
  pub span: Span,
}

/// This won't be serialized, will just simply skip to follow vue-eslint-parser's behavior.
#[derive(Debug)]
pub struct VComment<'a> {
  pub value: &'a str,
  pub span: Span,
}
