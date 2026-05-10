#![deny(clippy::all)]

mod diagnostics;
mod parse;
mod source_text;
mod transform;

pub use parse::native_parse;
pub use transform::native_transform_jsx;
