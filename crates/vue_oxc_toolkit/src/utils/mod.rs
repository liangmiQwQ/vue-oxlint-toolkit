//! Adapter helpers for bridging `vize_armature`'s AST to oxc's JSX AST.
//!
//! Vize's nodes carry source locations and structural data tailored to Vue
//! template processing; the helpers in this module translate those into the
//! span semantics oxc expects. Anything that exists purely to work around a
//! vize quirk lives here so the parser code stays readable.

mod text;
mod vize;

pub use text::{kebab_to_case, parse_v_for_alias};
pub use vize::{
  AttributeExt, DirectiveExt, ElementExt, VizeSpan, element_close_span, is_dynamic_arg,
};
