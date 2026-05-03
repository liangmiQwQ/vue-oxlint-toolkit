use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{Expression, FormalParameters, Program, Statement};
use oxc_span::Span;

pub mod bindings;
pub mod nodes;
/// Root of a parsed Vue SFC.
///
/// `children` is a flat list of top-level SFC nodes (e.g. `<template>`,
/// `<script>`, `<style>`, plus any whitespace / comments between them).
pub struct VueSingleFileComponent<'a, 'b> {
  pub children: ArenaVec<'a, VNode<'a, 'b>>,
  /// Comments collected from `<script>` / `<script setup>` bodies.
  ///
  /// HTML `<!-- -->` comments live as [`VComment`] nodes in the tree and are
  /// **not** flattened here.
  pub script_comments: ArenaVec<'a, oxc_ast::Comment>,
  pub source_type: oxc_span::SourceType,
}

/// A node in the V-tree.
pub enum VNode<'a, 'b> {
  Element(&'a VElement<'a, 'b>),
  Text(VText<'a>),
  Comment(VComment<'a>),
  Interpolation(VInterpolation<'b>),
  CData(VCData<'a>),
}

impl VNode<'_, '_> {
  #[must_use]
  pub const fn span(&self) -> Span {
    match self {
      Self::Element(e) => e.span,
      Self::Text(t) => t.span,
      Self::Comment(c) => c.span,
      Self::Interpolation(i) => i.span,
      Self::CData(c) => c.span,
    }
  }
}

/// `<tag ...> ... </tag>` (or self-closing).
pub struct VElement<'a, 'b> {
  pub start_tag: VStartTag<'a, 'b>,
  /// `None` for self-closing or void elements.
  pub end_tag: Option<VEndTag>,
  pub children: ArenaVec<'a, VNode<'a, 'b>>,
  /// Parsed JavaScript program for `<script>` / `<script setup>`.
  pub script: Option<VScript<'b>>,
  pub span: Span,
}

pub struct VScript<'b> {
  pub kind: VScriptKind,
  pub body_span: Span,
  pub program: Program<'b>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VScriptKind {
  Script,
  Setup,
}

pub struct VStartTag<'a, 'b> {
  /// Span of the tag name only (`div` in `<div ...>`).
  pub name_span: Span,
  pub attributes: ArenaVec<'a, VAttributeOrDirective<'a, 'b>>,
  pub self_closing: bool,
  /// Span of the entire start tag, including `<` and `>`.
  pub span: Span,
}

pub struct VEndTag {
  /// Span of the tag name only (`div` in `</div>`).
  pub name_span: Span,
  /// Span of the entire end tag, including `</` and `>`.
  pub span: Span,
}

pub enum VAttributeOrDirective<'a, 'b> {
  Attribute(VAttribute<'a>),
  Directive(VDirective<'a, 'b>),
}

impl VAttributeOrDirective<'_, '_> {
  #[must_use]
  pub const fn span(&self) -> Span {
    match self {
      Self::Attribute(a) => a.span,
      Self::Directive(d) => d.span,
    }
  }
}

/// A plain (non-directive) HTML attribute, e.g. `id="foo"`.
pub struct VAttribute<'a> {
  pub key: VAttributeKey<'a>,
  pub value: Option<VAttributeValue<'a>>,
  pub span: Span,
}

pub struct VAttributeKey<'a> {
  pub name: &'a str,
  pub span: Span,
}

/// An HTML attribute value, with both raw and decoded forms.
pub struct VAttributeValue<'a> {
  /// Raw text between the quotes (or unquoted), as it appears in source.
  pub raw: &'a str,
  /// Decoded value (HTML entities resolved). Allocated in `'a` if decoding
  /// produced a different string from `raw`.
  pub value: &'a str,
  /// Span covering the value, including surrounding quotes if any.
  pub span: Span,
  pub quote: VQuote,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VQuote {
  Double,
  Single,
  Unquoted,
}

/// A Vue directive: `v-name:arg.mod1.mod2="expr"`, or its shorthand forms
/// (`:`, `@`, `#`, `.`).
pub struct VDirective<'a, 'b> {
  pub key: VDirectiveKey<'a, 'b>,
  pub value: Option<VDirectiveValue<'a, 'b>>,
  pub span: Span,
}

pub struct VDirectiveKey<'a, 'b> {
  /// Directive name without `v-` prefix (e.g. `bind`, `on`, `for`, `slot`).
  pub name: VDirectiveName<'a>,
  /// `:arg` part. May be a static identifier or a dynamic `[expr]` argument.
  pub argument: Option<VDirectiveArgument<'a, 'b>>,
  /// `.mod` parts.
  pub modifiers: ArenaVec<'a, VDirectiveModifier<'a>>,
  /// Span covering the entire key (name + argument + modifiers), excluding
  /// `=` and the value.
  pub span: Span,
}

pub struct VDirectiveName<'a> {
  pub name: &'a str,
  pub span: Span,
}

pub struct VDirectiveArgument<'a, 'b> {
  pub raw: &'a str,
  pub kind: VDirectiveArgumentKind,
  /// Parsed expression inside `[arg]` for dynamic directive arguments.
  pub expression: Option<&'b Expression<'b>>,
  pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VDirectiveArgumentKind {
  /// `v-bind:foo`
  Static,
  /// `v-bind:[foo]`
  Dynamic,
}

pub struct VDirectiveModifier<'a> {
  pub name: &'a str,
  pub span: Span,
}

/// Directive value, with its parsed JavaScript form attached.
///
/// The exact JS shape depends on the directive kind — see the table in the
/// RFC. Spans on the embedded JS nodes refer to original SFC byte offsets.
pub struct VDirectiveValue<'a, 'b> {
  pub raw: &'a str,
  pub span: Span,
  pub quote: VQuote,
  pub expression: VDirectiveExpression<'a, 'b>,
}

pub enum VDirectiveExpression<'a, 'b> {
  /// Generic expression (`v-bind`, `v-if`, `v-show`, `v-model`, `:foo`, …).
  Expression(&'b Expression<'b>),
  /// `v-for="(item, index) in items"`.
  VFor(VForDirective<'b>),
  /// `v-slot:name="(props)"`.
  VSlot(VSlotDirective<'b>),
  /// `v-on:evt="…"` / `@evt="…"`. Body is parsed as a statement list.
  VOn(VOnExpression<'a, 'b>),
}

pub struct VForDirective<'b> {
  pub left: &'b FormalParameters<'b>,
  pub right: &'b Expression<'b>,
}

pub struct VSlotDirective<'b> {
  pub params: &'b FormalParameters<'b>,
}

pub struct VOnExpression<'a, 'b> {
  pub statements: ArenaVec<'a, Statement<'b>>,
}

/// `{{ expression }}`.
pub struct VInterpolation<'b> {
  pub expression: &'b Expression<'b>,
  pub span: Span,
}

/// Plain text node.
pub struct VText<'a> {
  /// Raw text as it appears in source (including HTML entities).
  pub raw: &'a str,
  /// HTML-entity-decoded text.
  pub value: &'a str,
  pub span: Span,
}

/// `<!-- comment -->`.
pub struct VComment<'a> {
  /// Comment body, excluding `<!--` and `-->`.
  pub value: &'a str,
  pub span: Span,
}

/// `<![CDATA[ ... ]]>` — only valid in foreign content (`<svg>`, `<math>`).
pub struct VCData<'a> {
  pub value: &'a str,
  pub span: Span,
}
