//! `vue_oxlint_parser` — first-party Vue SFC parser for the toolkit.
//!
//! See `rfcs/vue-oxlint-parser.md` for the design.

pub mod ast;
pub mod lexer;
pub mod parser;

pub use parser::{VueParseConfig, VueParser, VueParserReturn};
