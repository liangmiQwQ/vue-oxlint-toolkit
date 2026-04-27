# @vue-oxlint/plugin

> Native oxlint plugin for Vue SFCs, powered by `vue_oxlint_parser` via [napi-rs](https://napi.rs/).

## Install

```bash
npm install @vue-oxlint/plugin
# or
vp install @vue-oxlint/plugin
```

## Usage

```js
import plugin from '@vue-oxlint/plugin';
```

## Building from source

Requires Rust and [`@napi-rs/cli`](https://napi.rs/docs/introduction/getting-started).

```bash
# Debug build
vp run build:debug

# Release build
vp run build
```

## License

[MIT](../../LICENSE)
