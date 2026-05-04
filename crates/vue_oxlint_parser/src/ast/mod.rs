use oxc_allocator::Vec as ArenaVec;
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
