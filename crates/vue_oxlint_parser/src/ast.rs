//! V* AST node types — the Vue-side AST produced by the template parser.
//!
//! Modeled after vue-eslint-parser's AST. Nodes are arena-allocated in the
//! "Vue allocator" (`oxc_allocator::Allocator`) so `Box<'a, _>`/`Vec<'a, _>`
//! borrow into a single bump arena. Strings use `oxc_str::Str<'a>` (a
//! `repr(transparent)` wrapper over `&'a str`) which serializes as the bare
//! string. The slices borrow from the original SFC source which the caller
//! is required to keep alive for the lifetime of the AST.
//!
//! All nodes derive `serde::Serialize` so the entire tree can be exported
//! to JSON for transfer across the napi boundary.

use oxc_allocator::{Box as ArenaBox, Vec as ArenaVec};
use oxc_str::Str;
use serde::Serialize;

/// Byte-offset span (UTF-8 byte indices into the original SFC source).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct Span {
  pub start: u32,
  pub end: u32,
}

impl Span {
  #[must_use]
  pub const fn new(start: u32, end: u32) -> Self {
    Self { start, end }
  }
}

/// Top-level result of parsing a `.vue` SFC.
///
/// Note: `script_program` and other `oxc_ast` contents are *not* included in
/// this struct — they live in the JS allocator and are serialized separately.
/// See `parser::ParsedSfc`.
#[derive(Debug, Serialize)]
pub struct VDocumentFragment<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  pub children: ArenaVec<'a, VRootChild<'a>>,
}

impl<'a> VDocumentFragment<'a> {
  #[must_use]
  pub const fn new(range: Span, children: ArenaVec<'a, VRootChild<'a>>) -> Self {
    Self { r#type: "VDocumentFragment", range, children }
  }
}

/// Children of `VDocumentFragment` — top-level SFC blocks plus surrounding
/// whitespace/text nodes.
#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum VRootChild<'a> {
  Element(ArenaBox<'a, VElement<'a>>),
  Text(ArenaBox<'a, VText<'a>>),
}

#[derive(Debug, Serialize)]
pub struct VElement<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  pub name: Str<'a>,
  pub raw_name: Str<'a>,
  pub namespace: VNamespace,
  pub start_tag: ArenaBox<'a, VStartTag<'a>>,
  pub end_tag: Option<ArenaBox<'a, VEndTag>>,
  pub children: ArenaVec<'a, VElementChild<'a>>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub enum VNamespace {
  #[serde(rename = "html")]
  Html,
  #[serde(rename = "svg")]
  Svg,
  #[serde(rename = "mathml")]
  MathMl,
}

#[derive(Debug, Serialize)]
pub struct VStartTag<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  pub self_closing: bool,
  pub attributes: ArenaVec<'a, VAttribute<'a>>,
}

#[derive(Debug, Serialize)]
pub struct VEndTag {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
}

#[derive(Debug, Serialize)]
pub enum VElementChild<'a> {
  #[serde(rename = "VElement")]
  Element(ArenaBox<'a, VElement<'a>>),
  #[serde(rename = "VText")]
  Text(ArenaBox<'a, VText<'a>>),
  #[serde(rename = "VExpressionContainer")]
  ExpressionContainer(ArenaBox<'a, VExpressionContainer<'a>>),
}

#[derive(Debug, Serialize)]
pub struct VText<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  pub value: Str<'a>,
}

#[derive(Debug)]
pub struct VExpressionContainer<'a> {
  pub r#type: &'static str,
  pub range: Span,
  /// Raw expression source between the delimiters (`{{` / `}}` for mustache,
  /// or the attribute value source for directives).
  pub raw_expression: Str<'a>,
  /// Span of the inner expression source (excluding mustache delimiters).
  pub expression_range: Span,
  /// `true` when this container holds a `v-for` or otherwise non-expression
  /// payload that the simple parser does not analyse beyond text capture.
  pub raw: bool,
  /// When `true`, emit a synthetic `Identifier` `ESTree` node spanning
  /// `expression_range` whose `name` is `raw_expression`, instead of running
  /// the JS parser. Used for `v-bind` same-name shorthand where the argument
  /// text may not be a valid JS identifier (e.g. `:aria-label`).
  pub synthetic_identifier: bool,
  /// Kind of expression to produce. Default is a single JS expression; the
  /// other variants wrap the parsed body in a Vue-specific synthetic node
  /// (`VOnExpression` / `VSlotScopeExpression` / `VForExpression` /
  /// `VFilterSequenceExpression`) the way upstream `vue-eslint-parser` does.
  pub kind: VExprKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VExprKind {
  Default,
  VOn,
  VSlot,
  VFor,
}

impl Serialize for VExpressionContainer<'_> {
  fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;
    let expr = if self.raw {
      None
    } else if self.synthetic_identifier {
      crate::expr::synthetic_identifier_raw(self.raw_expression.as_ref(), self.expression_range)
    } else {
      match self.kind {
        VExprKind::Default => crate::expr::parse_expression_to_raw(
          self.raw_expression.as_ref(),
          self.expression_range.start,
        ),
        VExprKind::VOn => {
          crate::expr::parse_v_on_to_raw(self.raw_expression.as_ref(), self.expression_range)
        }
        VExprKind::VSlot => {
          crate::expr::parse_v_slot_to_raw(self.raw_expression.as_ref(), self.expression_range)
        }
        VExprKind::VFor => {
          crate::expr::parse_v_for_to_raw(self.raw_expression.as_ref(), self.expression_range)
        }
      }
    };
    let mut s = ser.serialize_struct("VExpressionContainer", 6)?;
    s.serialize_field("type", &self.r#type)?;
    s.serialize_field("range", &self.range)?;
    s.serialize_field("raw_expression", self.raw_expression.as_ref())?;
    s.serialize_field("expression_range", &self.expression_range)?;
    s.serialize_field("raw", &self.raw)?;
    match &expr {
      Some(v) => s.serialize_field("expression", v.as_ref())?,
      None => s.serialize_field("expression", &Option::<()>::None)?,
    }
    s.end()
  }
}

#[derive(Debug, Serialize)]
pub struct VAttribute<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  pub directive: bool,
  pub key: ArenaBox<'a, VAttributeKey<'a>>,
  pub value: Option<ArenaBox<'a, VAttributeValue<'a>>>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum VAttributeKey<'a> {
  Identifier(VIdentifier<'a>),
  Directive(VDirectiveKey<'a>),
}

#[derive(Debug, Serialize)]
pub struct VIdentifier<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  pub name: Str<'a>,
  pub raw_name: Str<'a>,
}

#[derive(Debug, Serialize)]
pub struct VDirectiveKey<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  /// Directive name as it appears in source — for shorthands this is the
  /// literal prefix (`:`, `@`, `#`); otherwise the full `v-foo` form.
  pub name: VIdentifier<'a>,
  /// Argument node — a static `VIdentifier` or, for dynamic arguments
  /// (`:[expr]`), a `VExpressionContainer` wrapping the bracketed expression.
  pub argument: Option<VDirectiveKeyArgument<'a>>,
  pub modifiers: ArenaVec<'a, VIdentifier<'a>>,
  /// Raw source text of the whole key (e.g. `v-bind:foo.sync`, `:foo`,
  /// `@click.stop`, `#default`).
  pub raw: Str<'a>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum VDirectiveKeyArgument<'a> {
  Identifier(VIdentifier<'a>),
  Expression(VExpressionContainer<'a>),
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum VAttributeValue<'a> {
  Literal(VLiteral<'a>),
  Expression(VExpressionContainer<'a>),
}

#[derive(Debug, Serialize)]
pub struct VLiteral<'a> {
  #[serde(rename = "type")]
  pub r#type: &'static str,
  pub range: Span,
  pub value: Str<'a>,
}
