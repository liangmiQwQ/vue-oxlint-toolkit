use oxc_allocator::Vec;
use oxc_span::Span;

use crate::ast::VAttribute;

pub enum VNode<'a> {
  Element(VElement<'a>),
  Text(VText<'a>),
  Comment(VComment<'a>),
  Interpolation(VInterpolation<'a>),
  CData(VCData<'a>),
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
