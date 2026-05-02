//! AST types for Vue SFC parsing.

use oxc_allocator::Vec as ArenaVec;
use oxc_ast::ast::{BindingPattern, Expression, FormalParameters, Program, Statement};
use oxc_diagnostics::OxcDiagnostic;
use oxc_span::{SourceType, Span};
use oxc_syntax::module_record::ModuleRecord;
use rustc_hash::FxHashSet;

/// Top-level Vue SFC AST node
pub struct VueSingleFileComponent<'a> {
  /// SFC tags as a flat children list
  pub children: Vec<VNode<'a>>,
  /// ONLY comments from `<script>` / `<script setup>` bodies
  pub script_comments: Vec<oxc_ast::Comment>,
  pub irregular_whitespaces: Box<[Span]>,
  pub clean_spans: FxHashSet<Span>,
  pub module_record: ModuleRecord<'a>,
  /// Derived from `<script (setup) lang>` attribute
  pub source_type: SourceType,
  pub errors: Vec<OxcDiagnostic>,
  /// Unrecoverable parse failure, like `oxc_parser`
  pub panicked: bool,
}

/// A V-tree node
pub enum VNode<'a> {
  Element(VElement<'a>),
  Text(VText),
  Comment(VComment),
  Interpolation(VInterpolation<'a>),
  CData(VCData),
}

/// An HTML/Vue element node
pub struct VElement<'a> {
  pub start_tag: VStartTag<'a>,
  pub end_tag: Option<VEndTag>,
  pub children: Vec<VNode<'a>>,
  pub span: Span,
  /// Parsed JS program for `<script>` elements
  pub program: Option<Program<'a>>,
}

/// A text node
pub struct VText {
  /// Raw source text
  pub raw: String,
  /// Decoded text (entity-decoded; currently same as raw for simplicity)
  pub value: String,
  pub span: Span,
}

/// An HTML comment `<!-- ... -->`
pub struct VComment {
  pub value: String,
  pub span: Span,
}

/// A mustache interpolation `{{ expr }}`
pub struct VInterpolation<'a> {
  pub expression: Option<Expression<'a>>,
  pub span: Span,
}

/// A CDATA section `<![CDATA[...]]>`
pub struct VCData {
  pub value: String,
  pub span: Span,
}

/// Opening tag of an element
pub struct VStartTag<'a> {
  /// Span of the tag name in the source
  pub name_span: Span,
  pub attributes: Vec<VAttrOrDirective<'a>>,
  pub self_closing: bool,
  pub span: Span,
}

/// Closing tag of an element
pub struct VEndTag {
  pub span: Span,
}

/// Either a plain attribute or a Vue directive
pub enum VAttrOrDirective<'a> {
  Attribute(VAttribute),
  Directive(VDirective<'a>),
}

/// A plain HTML attribute `name="value"`
pub struct VAttribute {
  pub name: String,
  pub name_span: Span,
  pub value: Option<VAttributeValue>,
  pub span: Span,
}

/// The value part of an attribute
pub struct VAttributeValue {
  pub raw: String,
  /// The span of the raw value content (without quotes)
  pub span: Span,
}

/// A Vue directive (v-*, :, @, #)
pub struct VDirective<'a> {
  /// Full directive name, e.g. `v-bind`, `v-for`, `v-on`, etc.
  pub name: DirectiveName,
  /// Directive argument, e.g. `:class` has arg `class`
  pub argument: Option<DirectiveArgument>,
  /// Directive modifiers
  pub modifiers: Vec<String>,
  /// The raw value string from the attribute
  pub value_raw: Option<String>,
  /// Value span (content, without quotes)
  pub value_span: Option<Span>,
  /// Parsed directive expression (for most directives)
  pub expression: Option<DirectiveExpression<'a>>,
  pub span: Span,
}

/// The name of a directive
pub enum DirectiveName {
  /// `v-for`
  For,
  /// `v-if`
  If,
  /// `v-else-if`
  ElseIf,
  /// `v-else`
  Else,
  /// `v-show`
  Show,
  /// `v-model`
  Model,
  /// `v-on` or `@evt`
  On,
  /// `v-bind` or `:prop`
  Bind,
  /// `v-slot` or `#slot`
  Slot,
  /// Any other directive by name
  Custom(String),
}

/// Argument to a directive (static or dynamic)
pub enum DirectiveArgument {
  Static(String, Span),
  /// Dynamic argument `:[expr]`
  Dynamic(String, Span),
}

/// The parsed expression(s) from a directive value
pub enum DirectiveExpression<'a> {
  /// A single expression (most directives)
  Expression(Expression<'a>),
  /// `v-for` directive
  For(VForDirective<'a>),
  /// `v-slot` directive
  Slot(VSlotDirective<'a>),
  /// `v-on` with statement-list body
  On(Vec<Statement<'a>>),
}

/// Parsed `v-for` directive
pub struct VForDirective<'a> {
  /// Left-hand side binding patterns `(item, index, ...)`
  pub left: ArenaVec<'a, BindingPattern<'a>>,
  /// Right-hand side expression `list`
  pub right: Expression<'a>,
}

/// Parsed `v-slot` directive
pub struct VSlotDirective<'a> {
  /// Slot params `(props)` - the parsed formal parameters
  pub params: Option<FormalParameters<'a>>,
}
