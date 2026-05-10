#![deny(clippy::all)]

mod source_text;
mod transform;

pub use transform::transform_jsx;
