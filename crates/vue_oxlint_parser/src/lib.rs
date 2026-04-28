//! Rust port of [vue-eslint-parser]'s SFC + template AST.
//!
//! [vue-eslint-parser]: https://github.com/vuejs/vue-eslint-parser
//!
//! ## Allocator design
//!
//! The crate is intentionally allocator-aware: a `vue_allocator` owns all `V*`
//! nodes (Vue-side AST), while a separate `js_allocator` owns the `oxc_ast`
//! `Program` produced from each `<script>` block. Splitting the two arenas
//! keeps the future `vue_oxlint_jsx` migration possible without forcing the
//! Vue AST and the JS AST to share a lifetime.
//!
//! Both AST sides serialise to JSON via `serde`; the JS side is bridged
//! through `oxc_estree` so its JSON shape stays ESTree-compatible.

#![deny(clippy::all)]

pub mod ast;
pub mod expr;
pub mod parser;
pub mod sfc;
pub mod template;

pub use oxc_diagnostics::OxcDiagnostic;
pub use parser::{ParseOptions, ParsedSfc, parse, parse_to_json};
