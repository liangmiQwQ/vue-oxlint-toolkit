# Vue Oxlint Toolkit Agent Guidance

## Project

This is an experimental toolkit providing high-performance Vue linting support for [Oxlint](https://oxc.rs). Oxlint will provide language plugin features in the future and this is an early exploration of this approach.

The core idea: Walk on two legs.

- We parse a Vue file into a `vue-eslint-parser` compatible AST, then load bindings, which is for template linting and existing Vue ESLint plugin.
- Transform the generated Vue SFC into an Oxc compatible JS/TS program and generate source_text, which is for script linting and run the Rust based rules on Oxlint side.

## Tooling

Tasks are managed through `just` (see `justfile`). JS tooling is [Vite+](https://viteplus.dev/) (`vp`/`vpr`/`vpx`), Rust is a Cargo workspace. I recommend using `just` to run command for verify or bundle process in this project. Do not use `npm` or `pnpm` directly, use `vp` instead.

Common commands:

- `just build` — `cargo build` plus `vpr build` (builds the napi binding + JS bundle).
- `just test` — runs `just build` first, then `cargo test --all-features --workspace` and `vp test`.
- `just lint` — `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --check`, `vp check`.
- `just fix` — `cargo fmt`, `cargo fix`, `vp check --fix`.
- `just ready` — full pre-PR check (clean tree → lint → build → test → clean tree). Run before submitting PRs; CI runs the equivalent.
- `just bench` — Criterion benchmarks in `benchmark/`.

## Project architecture

- Parser: `crates/vue_oxlint_parser`, which includes a tokenizer (lexer) and parser, generate a custom AST (VueSingleFileComponent) which implemented ESTree trait, it is the underlying parser.
- Jsx: `crates/vue_oxlint_jsx`, which receives the AST returned by parser, and emit an Oxc JSX/TSX program, it also supports emitting codegen result with volar mapping.
- Toolkit: `packages/vue-oxlint-toolkit`, a rs-napi package + crate, mainly for data trasnfering, including transform parser's ast into a vue-eslint-parser compatible AST.

For now, as the parser crate is still working in progress, jsx crate is still depending on `vue-compiler-core`, and most of the logic in toolkit package / crate is missing.

For how to get a `vue-eslint-parser` compatible AST, we should divide the work correctly to avoid cross-boundary processing. parser crates should handle serialization and inner node's structure and tokens, toolkit package should handle root level ast rebuild, and js only-metadata injecting. You shouldn't add things like generating `scriptBody` in parser crate, you should also never add things like call `@typescript-eslint/parser` in toolkit side.

Some signs of behavior exceeding boundaries:

1. If you find toolkit needs to read `source_text` after getting parser's AST for parsing use
2. If you find toolkit has a separate error define / process logic besides Oxc's diagnostics and Raw errors.
3. If you find `@typescript-eslint/parser` or `vue-eslint-parser` is called outside of tests.
4. If you find toolkit is modifying generated AST by type (Except top-level nodes and BigInt).

## Conventions

- Comments / PR titles follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, etc.). The repo squash-merges PRs. You should always use Conventional Commits format as PR title even if you are using codex-app connector.
- If you are modifying an existing PR / branch, prefer commit directly to avoid force pushing.
- Run `just ready` after your do changes and commits, it will run build, test, format check automatically.
- Consider to update AGENTS.md to sync after you made change (ATTENTION: it's `update`, means adjust or delete outdated things, but not `add things into`).
