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

- Parser: `crates/vue_oxlint_parser`, which includes a tokenizer (lexer) and parser, generate a custom AST (VueSingleFileComponent) which implemented ESTree trait, it is the underlying parser for generating vue-eslint-parser compatible ast and transformed js ast.
- Jsx: `crates/vue_oxlint_jsx`, which receives the AST returned by parser, and emit an Oxc JSX/TSX program, it also supports emitting codegen result with volar mapping.
- Toolkit: `packages/vue-oxlint-toolkit`, a rs-napi package + crate, mainly for data trasnfering and defining the Oxlint plugin, including control the ast + source_type generation pipeline and the process which rebuild the vue-eslint-parser compatible ast from the parser's AST. This is the main npm package for external dependencies.

For now, as the parser crate is still working in progress, jsx crate is still depending on `vue-compiler-core`, and most of the logic in toolkit package / crate is missing.

## Conventions

- Comments / PR titles follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, etc.). The repo squash-merges PRs. You should always use Conventional Commits format as PR title even if you are using codex-app connector.
- If you are modifying an existing PR / branch, prefer commit directly to avoid force pushing.
- Run `just ready` after your do changes and commits, it will run build, test, format check automatically.
- Consider to update AGENTS.md to sync after you made change (ATTENTION: it's `update`, means adjust or delete outdated things, but not `add things into`).
