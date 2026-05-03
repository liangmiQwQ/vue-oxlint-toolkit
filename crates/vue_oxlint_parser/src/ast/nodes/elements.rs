use oxc_allocator::Vec;
use oxc_span::Span;

use crate::ast::{VAttribute, nodes::javascript::VInterpolation};

pub enum VNode<'a> {
  Element(VElement<'a>),
  Text(VText<'a>),
  Comment(VComment<'a>),
  Interpolation(VInterpolation<'a>),
}

pub struct VElement<'a> {
  pub name: &'a str,
  pub raw_name: &'a str,
  pub start_tag: VStartTag<'a>,
  pub children: Vec<'a, VNode<'a>>,
  pub end_tag: Option<VEndTag>,
  pub span: Span,
}

pub struct VStartTag<'a> {
  pub attributes: Vec<'a, VAttribute<'a>>,
  pub self_closing: bool,
  pub span: Span,
}

pub struct VEndTag {
  pub span: Span,
}

pub struct VText<'a> {
  pub text: &'a str,
  pub span: Span,
}

/// This won't be serialized, will just simply skip to follow vue-eslint-parser's behavior.
pub struct VComment<'a> {
  pub value: &'a str,
  pub span: Span,
}
