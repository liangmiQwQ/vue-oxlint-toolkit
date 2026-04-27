#![deny(clippy::all)]

use napi_derive::napi;

/// Parse a Vue SFC source string and return the AST as a JS object.
/// This is a placeholder — implement by calling vue_oxlint_parser::VueParser.
#[napi]
pub fn parse(_source: String) -> String {
    todo!("wire up vue_oxlint_parser::VueParser")
}
