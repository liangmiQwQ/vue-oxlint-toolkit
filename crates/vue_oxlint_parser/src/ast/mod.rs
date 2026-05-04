use oxc_allocator::Vec as ArenaVec;
use oxc_estree::{ESTree, JsonSafeString, Serializer, StructSerializer};
use oxc_span::SourceType;

mod bindings;
mod nodes;

pub use bindings::*;
pub use nodes::*;

/// The parsed Vue SFC.
///
/// `children` is a flat list of top-level SFC nodes (e.g. `<template>`,
/// `<script>`, `<style>`, plus any whitespace / comments between them).
///
/// Will be serialization into  `VueSingleFileComponent` object, **NOT `ESLintProgram`**.
pub struct VueSingleFileComponent<'a, 'b> {
  pub children: ArenaVec<'a, VNode<'a, 'b>>,
  pub script_comments: ArenaVec<'a, oxc_ast::Comment>,
  pub template_comments: ArenaVec<'a, ()>,
  pub source_type: SourceType,
}

impl ESTree for VueSingleFileComponent<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VueSingleFileComponent"));
    state.serialize_field("children", &self.children);
    // state.serialize_field("script_comments", &self.script_comments);
    // state.serialize_field("template_comments", &self.template_comments);
    state.serialize_field("source_type", &self.source_type.module_kind());
    state.end();
  }
}
