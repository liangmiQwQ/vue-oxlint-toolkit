use oxc_allocator::{Box, Vec};
use oxc_estree::{ESTree, JsonSafeString, Serializer, StructSerializer};
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

/// `VComment` won't be serialized as normal ast node, will just simply skip to follow vue-eslint-parser's behavior.
#[derive(Debug)]
pub struct VComment<'a> {
  /// `HTMLBogusComment`, or `HTMLComment`
  pub r#type: &'static str,
  pub value: &'a str,
  pub span: Span,
}

impl ESTree for VNode<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    match self {
      VNode::Element(elem) => elem.serialize(serializer),
      VNode::Text(text) => text.serialize(serializer),
      VNode::Comment(_) => (),
      VNode::Interpolation(interpolation) => interpolation.serialize(serializer),
      VNode::PureScript(script) => script.serialize(serializer),
    }
  }
}

impl ESTree for VElement<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VElement"));
    state.serialize_field("name", &self.name);
    state.serialize_field("rawName", &self.raw_name);
    state.serialize_field("startTag", &self.start_tag);
    state.serialize_field("children", &self.children);
    state.serialize_field("endTag", &self.end_tag);
    state.serialize_field("variables", &self.variables);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VStartTag<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VStartTag"));
    state.serialize_field("attributes", &self.attributes);
    state.serialize_field("selfClosing", &self.self_closing);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VEndTag {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VEndTag"));
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VText<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VText"));
    state.serialize_field("text", &self.text);
    state.serialize_span(self.span);
    state.end();
  }
}
