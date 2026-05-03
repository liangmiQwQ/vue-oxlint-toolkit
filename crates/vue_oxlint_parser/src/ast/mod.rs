use crate::ast::nodes::elements::VNode;
use oxc_allocator::Vec as ArenaVec;
use oxc_span::SourceType;

pub mod bindings;
pub mod nodes;

/// The parsed Vue SFC.
///
/// `children` is a flat list of top-level SFC nodes (e.g. `<template>`,
/// `<script>`, `<style>`, plus any whitespace / comments between them).
pub struct VueSingleFileComponent<'a> {
  pub children: ArenaVec<'a, VNode<'a>>,
  pub script_comments: ArenaVec<'a, oxc_ast::Comment>,
  pub template_comments: ArenaVec<'a, ()>,
  pub source_type: SourceType,
}
