# Vue Oxlint Toolkit Agent Guidance

## Project

This is an experimental toolkit providing high-performance Vue linting support for [Oxlint](https://oxc.rs). Oxlint will provide language plugin features in the future and this is an early exploration of this approach.

The core idea: parse a Vue SFC and emit an equivalent JS/TS program that the standard `oxc_parser` AST/codegen tools can consume, so existing Oxlint rules can lint any related scripts in Vue.

## Tooling

Tasks are managed through `just` (see `justfile`). JS tooling is [Vite+](https://viteplus.dev/) (`vp`/`vpr`/`vpx`), read /vite-plus skill to learn more about it, Rust is a Cargo workspace. I recommend using `just` to run command for verify or bundle process in this project. Do not use `npm` or `pnpm` directly as we use Vite+.

Common commands:

- `just build` — `cargo build` plus `vpr build` (builds the napi binding + JS bundle).
- `just test` — runs `just build` first, then `cargo test --all-features --workspace` and `vp test`.
- `just lint` — `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --check`, `vp check`.
- `just fix` — `cargo fmt`, `cargo fix`, `vp check --fix`.
- `just ready` — full pre-PR check (clean tree → lint → build → test → clean tree). Run before submitting PRs; CI runs the equivalent.
- `just bench` — Criterion benchmarks in `benchmark/`.

## Conventions

- Comments / PR titles follow Conventional Commits (`feat:`, `fix:`, `refactor:`, `chore:`, etc.). The repo squash-merges PRs. You should always use Conventional Commits format as PR title even if you are using codex-app connector.
- Workspace clippy lints: `all`/`pedantic`/`nursery` at warn, with `cast_possible_truncation` and `too_many_lines` allowed.
- Rust formatting via `.rustfmt.toml` (run `just fmt`). JS formatting/linting via `vp check`.
- Run `just ready` after your do changes and commits.
- Consder to update (ATTENTION: it's `update` not `add`) AGENTS.md to sync after you made change.
