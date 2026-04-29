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

#![allow(unused_doc_comments)]

use oxc_allocator::{Box as ArenaBox, Vec as ArenaVec};
use oxc_span::SourceType;
use oxc_str::Str;

mod expr;
use serde::Serialize;

pub use oxc_span::Span;

/// Define a Vue AST node struct with an auto-filled `r#type` field and a
/// `new` constructor.
///
/// ```ignore
/// define_vue_node! {
///     #[derive(Debug, Serialize)]
///     pub struct VText<'a> {
///         pub range: Span,
///         pub value: Str<'a>,
///     }
///     pub fn new(range: Span, value: Str<'a>);
/// }
/// ```
/// The parameters of `new` must match the field names and order (excluding
/// `r#type`). The macro fills `r#type: stringify!(StructName)` automatically.
macro_rules! define_vue_node {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident $(<$lt:lifetime>)? {
            $($(#[$field_meta:meta])* $field_vis:vis $field:ident: $ty:ty),* $(,)?
        }
        $new_vis:vis fn new($($param:ident: $param_ty:ty),* $(,)?);
    ) => {
        $(#[$meta])*
        $vis struct $name $(<$lt>)? {
            #[serde(rename = "type")]
            pub r#type: &'static str,
            $($(#[$field_meta])* $field_vis $field: $ty),*
        }

        impl $(<$lt>)? $name $(<$lt>)? {
            #[must_use]
            $new_vis fn new($($param: $param_ty),*) -> Self {
                Self {
                    r#type: stringify!($name),
                    $($param),*
                }
            }
        }
    };
}

/// Top-level result of parsing a `.vue` SFC.
///
/// Note: `script_program` and other `oxc_ast` contents are *not* included in
/// this struct — they live in the JS allocator and are serialized separately.
/// See `parser::ParsedSfc`.
define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VDocumentFragment<'a> {
        pub range: Span,
        pub children: ArenaVec<'a, VRootChild<'a>>,
    }
    pub fn new(range: Span, children: ArenaVec<'a, VRootChild<'a>>);
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
pub enum VElementChild<'a> {
  #[serde(rename = "VElement")]
  Element(ArenaBox<'a, VElement<'a>>),
  #[serde(rename = "VText")]
  Text(ArenaBox<'a, VText<'a>>),
  #[serde(rename = "VExpressionContainer")]
  ExpressionContainer(ArenaBox<'a, VExpressionContainer<'a>>),
}

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VElement<'a> {
        pub range: Span,
        pub name: Str<'a>,
        pub raw_name: Str<'a>,
        pub namespace: VNamespace,
        pub start_tag: ArenaBox<'a, VStartTag<'a>>,
        pub end_tag: Option<ArenaBox<'a, VEndTag>>,
        pub children: ArenaVec<'a, VElementChild<'a>>,
    }
    pub fn new(
        range: Span,
        name: Str<'a>,
        raw_name: Str<'a>,
        namespace: VNamespace,
        start_tag: ArenaBox<'a, VStartTag<'a>>,
        end_tag: Option<ArenaBox<'a, VEndTag>>,
        children: ArenaVec<'a, VElementChild<'a>>,
    );
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

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VStartTag<'a> {
        pub range: Span,
        pub self_closing: bool,
        pub attributes: ArenaVec<'a, VAttribute<'a>>,
    }
    pub fn new(range: Span, self_closing: bool, attributes: ArenaVec<'a, VAttribute<'a>>);
}

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VEndTag {
        pub range: Span,
    }
    pub fn new(range: Span);
}

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VText<'a> {
        pub range: Span,
        pub value: Str<'a>,
    }
    pub fn new(range: Span, value: Str<'a>);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum VExprKind {
  Default,
  VOn,
  VSlot,
  VFor,
}

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VExpressionContainer<'a> {
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
        /// Parser mode for the embedded JS/TS source.
        #[serde(skip)]
        pub source_type: SourceType,
    }
    pub fn new(
        range: Span,
        raw_expression: Str<'a>,
        expression_range: Span,
        raw: bool,
        synthetic_identifier: bool,
        kind: VExprKind,
        source_type: SourceType,
    );
}

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VAttribute<'a> {
        pub range: Span,
        pub directive: bool,
        pub key: ArenaBox<'a, VAttributeKey<'a>>,
        pub value: Option<ArenaBox<'a, VAttributeValue<'a>>>,
    }
    pub fn new(
        range: Span,
        directive: bool,
        key: ArenaBox<'a, VAttributeKey<'a>>,
        value: Option<ArenaBox<'a, VAttributeValue<'a>>>,
    );
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum VAttributeKey<'a> {
  Identifier(VIdentifier<'a>),
  Directive(VDirectiveKey<'a>),
}

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VIdentifier<'a> {
        pub range: Span,
        pub name: Str<'a>,
        pub raw_name: Str<'a>,
    }
    pub fn new(range: Span, name: Str<'a>, raw_name: Str<'a>);
}

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VDirectiveKey<'a> {
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
    pub fn new(
        range: Span,
        name: VIdentifier<'a>,
        argument: Option<VDirectiveKeyArgument<'a>>,
        modifiers: ArenaVec<'a, VIdentifier<'a>>,
        raw: Str<'a>,
    );
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

define_vue_node! {
    #[derive(Debug, Serialize)]
    pub struct VLiteral<'a> {
        pub range: Span,
        pub value: Str<'a>,
    }
    pub fn new(range: Span, value: Str<'a>);
}
