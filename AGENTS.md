# Vue Oxlint Toolkit Agent Guide

## Repository purpose

Experimental toolkit that brings high-performance Vue SFC support to the [Oxc](https://oxc.rs) / [oxlint](https://oxc.rs/docs/guide/usage/linter) ecosystem. The work is split into two parallel Rust ports of the upstream Vue parser stack, plus a Node.js binding that exposes the result as JSON.

## Tasks (use `just`)

Tasks are managed through `justfile`. Run `just` to list them. The most common ones:

- `just lint` — `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `vp check`.
- `just fix` / `just fmt` — auto-fix Rust + JS, run `cargo fmt`.
- `just test` — `cargo test --all-features --workspace` and `vp test` (Vitest via vite-plus).
- `just build` — `cargo build` and `vpr build` (builds the napi addon and the JS package).
- `just bench` — `cargo bench -p benchmark`.
- `just ready` — pre-flight before pushing: requires a clean tree, then runs lint, fix, build, test, and reverifies the tree is clean.

JS package management uses `Vite+` (workspace defined in `pnpm-workspace.yaml`, catalogs for shared versions). The `vite` and `vitest` packages are overridden to the `@voidzero-dev/vite-plus-*` builds — this is intentional by vite-plus, do not "fix" it.

Do not run `pnpm` or `npm` directly. Use `vp run <command>` instead.

## Architecture

Three layers, stacked. Lower layers do not know about upper layers.

- `crates/vue_oxlint_parser` — Rust port of `vue-eslint-parser`. SFC-level parsing: splits a `.vue` file into `<template>`, `<script>`, `<style>`, and custom blocks, parses the template into a Vue AST, and delegates JS/TS inside `<script>` to `oxc_parser`. Foundation crate; depends only on `oxc_*`.
- `crates/vue_oxlint_jsx` — Rust port of the Vue template-to-JSX transform. Consumes the template AST produced by `vue_oxlint_parser` and emits an `oxc_ast` JSX program that oxlint's JS rules can lint as if it were a regular `.jsx` file.
- `packages/vue-oxlint-toolkit` — Node.js package. Wraps both crates through a napi addon (`bindings/`, `build.rs`) and exposes the result to JS (`src/`, `js/`). This is the integration point oxlint plugs into to gain Vue SFC support.

Data flow: `.vue` source → `vue_oxlint_parser` (SFC + template AST + script AST) → `vue_oxlint_jsx` (template AST → JSX AST) → napi addon → `vue-oxlint-toolkit` JS API → oxlint.

Both crates exist on purpose; they are not duplicates of each other. Keep the dependency direction one-way: `vue_oxlint_jsx` depends on `vue_oxlint_parser`, never the reverse.

## Conventions

- `oxc_*` deps are pinned with `>=0.127.0` in the workspace `Cargo.toml` — bump together.
- `.vue` files are just for testing, not a part of project production code.
- Conventional Commits are required for PR titles (see `CONTRIBUTING.md`).

Keep AGENTS.md updated with the project codebase. Consider if there is need to modify AGENTS.md after your changes. Don't store meaningless things like project structure or project status in AGENTS.md.

Never use emoji no matter where.

Keep code functional. Never use classes. Write simple code and make function reusable if possible. Use Unix philosophy to design your code (Every function should only do one thing and should not be too long or complex).

Run the `just build && just fix` and `just test` to you make sure your changes won't break the project.
