# vue_oxlint_jsx

Transforms Vue Single File Components into an [OXC](https://oxc.rs/)-compatible JSX/TSX `Program` for code linting.

## Features

- **Full SFC parsing** — `<template>`, `<script>`, and `<script setup>` blocks; composition and options API; TypeScript.
- **JSX/TSX output** — Vue templates are lowered to JSX/TSX within a standard OXC `Program`, enabling deep semantic analysis without a separate Vue-specific analysis pass.
- **Linter metadata** — produces `module_record` and `irregular_whitespaces` required by `oxc_linter`.
- **Single-pass** — template transformation completes in one traversal.
- **Error collection** — aggregates errors from both `vue-compiler-core` and `oxc_parser`; mirrors `oxc_parser`'s `panicked` logic.

## Usage

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
assert!(ret.errors.is_empty());
```

## Credits

Includes a fork of [vue-oxc-parser](https://github.com/zhiyuanzmj/vue-oxc-parser) by zhiyuanzmj.
Depends on [vue-compiler-rs](https://github.com/HerringtonDarkholme/vue-compiler) for Vue template parsing.

## License

[MIT](../../LICENSE)
