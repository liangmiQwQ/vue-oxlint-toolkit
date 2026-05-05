use oxc_allocator::Vec as ArenaVec;
use oxc_ast::Comment;
use oxc_estree::{ESTree, JsonSafeString, Serializer, StructSerializer};
use oxc_span::{SourceType, Span};

mod bindings;
mod comments;
mod nodes;
pub mod token;

pub use bindings::*;
pub use nodes::*;

use crate::ast::{comments::ESTreeComment, token::SerializableToken};

/// The parsed Vue SFC.
///
/// `children` is a flat list of top-level SFC nodes (e.g. `<template>`,
/// `<script>`, `<style>`, plus any whitespace / comments between them).
///
/// Will be serialization into  `VueSingleFileComponent` object, **NOT `ESLintProgram`**.
pub struct VueSingleFileComponent<'a, 'b> {
  pub source_text: &'b str,
  pub script_comments: ArenaVec<'a, Comment>,
  pub template_comments: ArenaVec<'a, VComment<'a>>,
  /// Only for serialization use
  /// Corresponding: `ReturnValue<typeof await('vue-eslint-parser')>['tokens']`
  ///
  /// Including: `<script setup>` `</script>` as punctuators
  /// JavaScript tokens in `<script>` tag. (**Not** including script in template)
  ///
  /// This field should be filled while calling `oxc_parse` function while parse `<script>` tag
  /// `<script setup>` and `<script>` tokens are also added before or after this.
  pub(crate) script_tokens: ArenaVec<'a, SerializableToken<'a, 'b>>,
  /// Only for serialization use
  /// Corresponding: `ReturnValue<typeof await('vue-eslint-parser')>['templateBody']['tokens']`
  ///
  /// Including: the whole SFC's token.
  /// Replace script in template tokens to JavaScript tokens.
  /// Script tokens in `<script>` tag is in `script_tokens` above, they should be stored as `HTMLRawText` here.
  ///
  /// This field should be filled when doing `Lexer::next_token()`
  pub(crate) template_tokens: ArenaVec<'a, SerializableToken<'a, 'b>>,
  pub children: ArenaVec<'a, VNode<'a, 'b>>,
  pub source_type: Option<SourceType>,
  pub span: Span,
}

impl<'a, 'b> ESTree for VueSingleFileComponent<'a, 'b>
where
  'b: 'a,
{
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VueSingleFileComponent"));
    state.serialize_field("children", &self.children);

    // Process comments (oxc do not serialize comments by default)
    let script_comments: &[ESTreeComment] = &self
      .script_comments
      .iter()
      .map(|comment| ESTreeComment::from_oxc_comment(comment, self.source_text))
      .collect::<Vec<ESTreeComment>>();
    state.serialize_field("script_comments", &script_comments);

    let template_comments: &[ESTreeComment] = &self
      .template_comments
      .iter()
      .map(ESTreeComment::from_v_comment)
      .collect::<Vec<ESTreeComment>>();
    state.serialize_field("template_comments", &template_comments);

    state.serialize_field("scriptTokens", &self.script_tokens);
    state.serialize_field("templateTokens", &self.template_tokens);
    state.serialize_field("source_type", &self.source_type.map(SourceType::module_kind));
    state.serialize_span(self.span);
    state.end();
  }
}
