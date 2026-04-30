mod codegen;
mod parser;

#[cfg(test)]
mod test;

pub use crate::codegen::{CodegenMode, VueOxcCodegen, VueOxcCodegenReturn};
pub use crate::parser::{VueOxcParser, VueParserReturn};
