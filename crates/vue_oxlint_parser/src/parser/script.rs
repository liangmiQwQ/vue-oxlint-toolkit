//! `<script>` / `<script setup>` handling.
//!
//! Phase 3 will port the relevant utilities from `vue_oxlint_jsx`'s
//! `parser::script` and `parser::modules`:
//!
//! - resolving `lang="ts"` / `lang="tsx"` into a `SourceType`
//! - guarding against multiple `<script>` / `<script setup>` blocks with
//!   conflicting `SourceType`s
//! - feeding script body slices into `oxc_parser` via the wrap-and-reset
//!   trick
//! - merging the resulting [`oxc_syntax::module_record::ModuleRecord`]s
//! - relocating `oxc_ast::Comment`s onto
//!   [`crate::ast::VueSingleFileComponent::script_comments`]
//!
//! Phase 4 will then call into these helpers as the recursive-descent parser
//! crosses each `<script>` block.
