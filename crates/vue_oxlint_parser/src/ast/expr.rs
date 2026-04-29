//! On-demand expression parsing for `VExpressionContainer`.
//!
//! TEMPORARILY DISABLED: all oxc-parser-driven serde paths are blanked out
//! so that no `ESTree` AST is produced during serialization.  The public
//! helpers below always return `None`, which causes the `expression` field
//! to be serialized as `null`.

use serde_json::value::RawValue;

use crate::ast::Span;

/// Wrap a `v-on` directive value.
#[must_use]
pub const fn parse_v_on_to_raw(
  _text: &str,
  _span: Span,
  _source_type: oxc_span::SourceType,
) -> Option<Box<RawValue>> {
  None
}

/// Wrap a `v-slot` (or `slot-scope`) directive value.
#[must_use]
pub const fn parse_v_slot_to_raw(
  _text: &str,
  _span: Span,
  _source_type: oxc_span::SourceType,
) -> Option<Box<RawValue>> {
  None
}

/// Wrap a `v-for` directive value.
#[must_use]
pub const fn parse_v_for_to_raw(
  _text: &str,
  _span: Span,
  _source_type: oxc_span::SourceType,
) -> Option<Box<RawValue>> {
  None
}

/// Build an `Identifier`-shaped JSON node directly.
#[must_use]
pub const fn synthetic_identifier_raw(_name: &str, _span: Span) -> Option<Box<RawValue>> {
  None
}

/// Parse `text` as a JS expression and return its `ESTree` JSON.
#[must_use]
pub const fn parse_expression_to_raw(
  _text: &str,
  _base: u32,
  _source_type: oxc_span::SourceType,
) -> Option<Box<RawValue>> {
  None
}
