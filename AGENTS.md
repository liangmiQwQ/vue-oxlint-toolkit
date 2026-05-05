# Vue Oxlint Toolkit Agent Guidance

## Project

This is an experimental toolkit providing high-performance Vue linting support for [Oxlint](https://oxc.rs). Oxlint will provide language plugin features in the future and this is an early exploration of this approach.

The core idea: parse a Vue SFC and emit an equivalent JS/TS program that the standard `oxc_parser` AST/codegen tools can consume, so existing Oxlint rules can lint any related scripts in Vue.

## Tooling

Tasks are managed through `just` (see `justfile`). JS tooling is [Vite+](https://viteplus.dev/) (`vp`/`vpr`/`vpx`), Rust is a Cargo workspace. I highly recommend using `just` to run any command in this project. Do not use `npm` or `pnpm` directly as we use Vite+.

Common commands:

- `just build` — `cargo build` plus `vpr build` (builds the napi binding + JS bundle).
- `just test` — runs `just build` first, then `cargo test --all-features --workspace` and `vp test`.
- `just lint` — `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --check`, `vp check`.
- `just fix` — `cargo fmt`, `cargo fix`, `vp check --fix`.
- `just ready` — full pre-PR check (clean tree → lint → build → test → clean tree). Run before submitting PRs; CI runs the equivalent.
- `just bench` — Criterion benchmarks in `benchmark/`.

## Workspace layout

Cargo workspace members live in `crates/*`, `packages/*`, and `benchmark/`.

- **`crates/vue_oxlint_jsx`** — the active crate. Parses a Vue SFC and produces either a JS/TS AST (`VueJsxParser`) or generated source text (`VueJsxCodegen`).
  - `parser/` — One of the most core part, including SFC tokenization, script/template handling, module-record building, irregular-whitespace tracking, and Vue-specific element handlers (`elements/v_for.rs`, `v_if.rs`, `v_slot.rs`, `directive.rs`).
  - `codegen/` — wraps `parser` and transform the ast into source_text, producing `VueJsxCodegenReturn { source_text, source_type, comments, irregular_whitespaces, errors, panicked }`.
  - `codegen/oxc/` — vendored fork of `oxc_codegen`, scoped to the Vue crate so codegen changes can be made locally without waiting on upstream releases. The local fork intentionally omits sourcemap support.
  - `test/` — `test_ast!` and `test_module_record!` macros driving snapshot tests in `test/snapshots/`. `test_ast!` runs both AST and codegen checks on each fixture.
  - Public API is intentionally narrow: `VueJsxParser`/`VueJsxParserReturn` + `VueJsxCodegen`/`VueJsxCodegenReturn` (re-exported from `lib.rs`).

- **`crates/vue_oxlint_parser`** — in-progress Rust port of `vue-eslint-parser`.
  - `ast.rs` — canonical V-tree surface (`VueSingleFileComponent`, `VElement`, directive/value nodes, embedded-JS attachment points).
  - `lexer/` — first-party HTML/Vue template tokenizer, split by mode (`data.rs`, `tag.rs`, `text.rs`, `utils.rs`, `tokens.rs`) and covering raw-text/RCDATA/foreign-content/v-pre handling plus vue-eslint-parser-compatible token kinds.
  - `parser/mod.rs` — two-allocator `VueParser` parse entry point and parse return surface.
  - `parser/parse/` — token-stream-driven recursive parser split by responsibility: children/text/comment handling, element parsing, attributes/directives, embedded expression parsing, variables, and shared utilities.
  - `parser/oxc_parse.rs` — wrapped `oxc_parser` calls for script bodies, script comment/token collection, diagnostics, and clean-span tracking.

- **`packages/vue-oxlint-toolkit`** — published npm package `vue-oxlint-toolkit`.
  - `src/lib.rs` — napi-rs cdylib exposing `transformJsx(source)` and `parseVue(source)`, converting Rust parser/codegen results to N-API types.
  - `js/` — JS wrapper split into public exports (`index.ts`), native parse/transform adapters, UTF-8 to UTF-16 location helpers, shared types, and the Vue AST adapter (`vue-ast.ts`). It returns `@oxlint/plugins`-shaped `Comment`/`Diagnostic`/`Range` objects and adapts serialized Vue SFC AST JSON into an ESLint-style `Program`.
  - Built with `napi build` (`build:debug` also runs `vp pack` to produce the JS bundle).

- **`benchmark/`** — Criterion benches over `small.vue`, `medium.vue`, `large.vue`.

## Architectural notes for changes in `vue_oxlint_jsx`

- `ParserImpl` (in `parser/mod.rs`) owns an arena allocator, the original SFC source, and a mutable copy of the source bytes (`mut_ptr_source_text`) which Vue-specific transformations rewrite in place before passing the result to `oxc_parser`. Spans on the resulting AST refer to _original SFC_ offsets, not the rewritten buffer.
- A `ScriptBlock` is collected for the global `<script>` and the `<script setup>` block independently (`global` and `setup`); they're stitched into a single `Program` during parse.
- Comments and irregular whitespaces are tracked separately because they're stripped/relocated during the SFC→JS rewrite but must be reported back with original-source spans.
- The codegen entry point (`VueJsxCodegen`) drops the parser allocator before returning — only owned data is exposed. Use `VueJsxParser` instead if you need the AST itself.
- **Clean-set mapping**: `ParserImpl` maintains a `clean_spans: FxHashSet<Span>` of spans that come directly from the original source (top-level statements and directives from each `<script>` block). `Codegen` receives this set via `with_clean_spans()` and, before printing any node, checks if its span is clean. Clean nodes are emitted verbatim from `program.source_text` with a single mapping entry; dirty nodes use the normal per-child traversal. This eliminates overlapping mapping entries for unchanged script content and preserves original whitespace/formatting.

## Vue/SFC parsing context

The current parsing still happens in `vue_oxlint_jsx`, powered by `vue-compiler-core`. But we plan to move it to the standalone crate `crates/vue_oxlint_parser` for the united parsing logic in the future.

## Conventions

- Comments / PR titles follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, etc.). The repo squash-merges PRs. You should always use Conventional Commits format as PR title even if you are using codex-app connector.
- Workspace clippy lints: `all`/`pedantic`/`nursery` at warn, with `cast_possible_truncation` and `too_many_lines` allowed.
- Rust formatting via `.rustfmt.toml` (run `just fmt`). JS formatting/linting via `vp check`.
- Run `just ready` after your do changes and commits.
- Consder to update (ATTENTION: it's `update` not `add`) AGENTS.md to sync after you made change.
