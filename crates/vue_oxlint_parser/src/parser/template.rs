//! `<template>` body handling.
//!
//! Phase 4 will implement the recursive-descent parser that turns lexer
//! tokens into the V-tree (`VElement`, `VText`, `VComment`, `VInterpolation`,
//! `VCData`), including:
//!
//! - HTML5 open/close-tag matching with implied closes (e.g. `<p>` autoclose)
//! - void / self-closing element handling
//! - mode switches for `<script>` / `<style>` / `<textarea>` / `<title>` and
//!   foreign content (`<svg>`, `<math>`)
//! - `v-pre` subtree skipping for directive parsing
//! - attribute / directive parsing, including dispatching embedded JS
//!   regions (`v-bind`, `v-if`, `v-for`, `v-slot`, `v-on`, `{{ … }}`) to
//!   `oxc_parser` via the wrap-and-reset trick on the `'b` arena
//! - non-HTML preprocessor diagnostics for `<template lang="pug">` etc.
