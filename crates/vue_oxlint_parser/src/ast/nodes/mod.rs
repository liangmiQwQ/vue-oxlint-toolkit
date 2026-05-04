//! ## Ast Nodes Rules
//! All ast nodes should have `span: Span` field.
//! All ast nodes should implement `Debug` and `ESTree` (injects type, range, start, end) trait.

mod attribute;
mod directive;
mod elements;
mod javascript;

pub use attribute::*;
pub use directive::*;
pub use elements::*;
pub use javascript::*;
