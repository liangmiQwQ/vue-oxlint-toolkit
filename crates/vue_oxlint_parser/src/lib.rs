//! `vue_oxlint_parser` — first-party Vue SFC parser.
//!
//! Parses a Vue Single-File Component and produces a [`VueSingleFileComponent`] AST.

#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(clippy::module_name_repetitions)]

pub mod ast;
pub mod irregular_whitespaces;
pub mod parser;

pub use ast::*;

use oxc_allocator::Allocator;

/// Public return type from [`parse_sfc`].
pub struct VueSfcParserReturn<'a> {
  pub sfc: VueSingleFileComponent<'a>,
}

/// Parse a Vue SFC source string and return the AST.
#[must_use]
pub fn parse_sfc<'a>(allocator: &'a Allocator, source_text: &'a str) -> VueSfcParserReturn<'a> {
  let sfc = parser::parse_impl(allocator, source_text);
  VueSfcParserReturn { sfc }
}

#[cfg(test)]
mod tests {
  use super::*;
  use oxc_allocator::Allocator;

  #[test]
  fn test_basic_sfc() {
    let allocator = Allocator::default();
    let src = "<template><div>hello</div></template>";
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked);
    assert_eq!(ret.sfc.children.len(), 1);
  }

  #[test]
  fn test_empty_sfc() {
    let allocator = Allocator::default();
    let src = "";
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked);
    assert_eq!(ret.sfc.children.len(), 0);
  }

  #[test]
  fn test_script_only() {
    let allocator = Allocator::default();
    let src = "<script>\nconst x = 1;\n</script>";
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
    assert_eq!(ret.sfc.children.len(), 1);
  }

  #[test]
  fn test_script_and_template() {
    let allocator = Allocator::default();
    let src = r#"<script>
export default {};
</script>
<template>
  <div>Hello World</div>
</template>"#;
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
    // Should have 2 children: script + template (plus maybe whitespace text nodes)
    assert!(ret.sfc.children.len() >= 2);
  }

  #[test]
  fn test_comment_node() {
    let allocator = Allocator::default();
    let src = "<!-- this is a comment --><template></template>";
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked);
    // first child should be a comment
    match &ret.sfc.children[0] {
      VNode::Comment(c) => assert_eq!(c.value.trim(), "this is a comment"),
      _ => panic!("Expected comment node"),
    }
  }

  #[test]
  fn test_self_closing_element() {
    let allocator = Allocator::default();
    let src = "<template><img src=\"test.png\" /></template>";
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
  }

  #[test]
  fn test_interpolation() {
    let allocator = Allocator::default();
    let src = "<template><div>{{ message }}</div></template>";
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
  }

  #[test]
  fn test_directive_v_if() {
    let allocator = Allocator::default();
    let src = r#"<template><div v-if="show">hello</div></template>"#;
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
  }

  #[test]
  fn test_directive_v_for() {
    let allocator = Allocator::default();
    let src = r#"<template><div v-for="item in items">{{ item }}</div></template>"#;
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
  }

  #[test]
  fn test_script_setup() {
    let allocator = Allocator::default();
    let src = r#"<script setup>
import { ref } from 'vue';
const count = ref(0);
</script>
<template>
  <div>{{ count }}</div>
</template>"#;
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
  }

  #[test]
  fn test_typescript_script() {
    let allocator = Allocator::default();
    let src = r#"<script lang="ts">
interface Foo { bar: string }
export default {} as Foo;
</script>"#;
    let ret = parse_sfc(&allocator, src);
    assert!(!ret.sfc.panicked, "errors: {:?}", ret.sfc.errors);
  }

  #[test]
  fn test_irregular_whitespaces() {
    let allocator = Allocator::default();
    let src = "<template>\u{000B}</template>";
    let ret = parse_sfc(&allocator, src);
    assert_eq!(ret.sfc.irregular_whitespaces.len(), 1);
    assert_eq!(ret.sfc.irregular_whitespaces[0].start, 10);
  }
}
