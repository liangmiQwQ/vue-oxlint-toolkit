use oxc_allocator::Box;
use oxc_estree::{ESTree, JsonSafeString, Serializer, StructSerializer};
use oxc_span::Span;

use crate::ast::nodes::directive::{VDirective, VForDirective, VOnDirective, VSlotDirective};

/// All the things inside this enum will be serialized into `VAttribute` struct.
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

impl ESTree for VAttribute<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    match self {
      VAttribute::VPureAttribute(attr) => attr.serialize(serializer),
      VAttribute::VDirective(dir) => dir.serialize(serializer),
      VAttribute::VOnDirective(dir) => dir.serialize(serializer),
      VAttribute::VSlotDirective(dir) => dir.serialize(serializer),
      VAttribute::VForDirective(dir) => dir.serialize(serializer),
    }
  }
}

impl ESTree for VPureAttribute<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VAttribute"));
    state.serialize_field("directive", &false);
    state.serialize_field("key", &self.key);
    state.serialize_field("value", &self.value);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VIdentifier<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VIdentifier"));
    state.serialize_field("name", &self.name);
    state.serialize_field("rawName", &self.raw_name);
    state.serialize_span(self.span);
    state.end();
  }
}

impl ESTree for VLiteral<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VLiteral"));
    state.serialize_field("value", &self.value);
    state.serialize_span(self.span);
    state.end();
  }
}
