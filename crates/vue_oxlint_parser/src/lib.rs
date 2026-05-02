//! `vue_oxlint_parser` — first-party Vue SFC parser for the toolkit.
//!
//! See `rfcs/vue-oxlint-parser.md` for the design. This crate is being built
//! out in phases:
//!
//! - **Phase 1 (this commit)**: V-tree AST, parser/lexer module skeleton,
//!   public surface. Parsing logic is `todo!()`.
//! - Phase 2: high-compatibility template lexer + tokens.
//! - Phase 3: `<script>` / `<script setup>` utilities ported from
//!   `vue_oxlint_jsx`.
//! - Phase 4: recursive-descent parser implementation.

pub mod ast;
pub mod lexer;
pub mod parser;

pub use parser::{VueParseConfig, VueParser, VueParserReturn};
