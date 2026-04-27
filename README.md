# vue-oxlint-toolkit

A Rust + Node.js monorepo providing Vue SFC parsing and linting infrastructure built on [OXC](https://oxc.rs/).

## Workspace

| Crate / Package | Description |
|---|---|
| [`vue_oxlint_parser`](crates/vue_oxlint_parser) | Rust port of [vue-eslint-parser](https://github.com/vuejs/vue-eslint-parser) — Vue SFC AST types and parser |
| [`vue_oxlint_jsx`](crates/vue_oxlint_jsx) | Transforms Vue SFCs into an OXC-compatible JSX/TSX `Program` for linting |
| [`@vue-oxlint/plugin`](packages/vue-oxlint-plugin) | napi-rs bindings exposing the parser to [oxlint](https://oxc.rs/docs/guide/usage/linter.html) |

## Quick Start

### Rust

```toml
[dependencies]
vue_oxlint_jsx = "0.11"
```

```rust
use oxc_allocator::Allocator;
use vue_oxlint_jsx::VueOxcParser;

let allocator = Allocator::default();
let source = r#"
<template><div>{{ msg }}</div></template>
<script setup>
const msg = 'hello';
</script>
"#;

let ret = VueOxcParser::new(&allocator, source).parse();
assert!(!ret.panicked);
```

### Node.js (oxlint plugin)

```bash
vp install @vue-oxlint/plugin
```

## Development

### Prerequisites

- Rust (see `rust-toolchain.toml`)
- [`just`](https://github.com/casey/just)
- Node.js ≥ 18 + [`vp`](https://vite.plus) (Vite+ CLI)

### Rust Tasks

```bash
just test      # Run all tests
just lint      # Clippy + cargo-shear
just fmt       # rustfmt + dprint
just bench     # Benchmarks
just coverage  # LLVM coverage report
```

### JS Tasks

```bash
vp install                                 # Install Node.js dependencies
vp run --filter @vue-oxlint/plugin build   # Build the napi-rs native module + JS wrapper
vp test                                    # Run vitest suite
```

## Credits

Includes a fork of [vue-oxc-parser](https://github.com/zhiyuanzmj/vue-oxc-parser) by zhiyuanzmj.
Depends on [vue-compiler-rs](https://github.com/HerringtonDarkholme/vue-compiler) for Vue template parsing.

## License

[MIT](./LICENSE)
