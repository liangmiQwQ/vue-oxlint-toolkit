# vue_oxlint_parser

Rust port of [vue-eslint-parser](https://github.com/vuejs/vue-eslint-parser).

Defines Vue SFC AST node types (`VElement`, `VAttribute`, `VDirective`, `VText`, etc.) and a `VueParser` that produces them from raw `.vue` source text.

The [`vue_oxlint_jsx`](../vue_oxlint_jsx) crate consumes this AST and converts it into an OXC-compatible `Program` for downstream linting.

## Status

Work in progress — AST types are defined, parser implementation is in progress.

## License

[MIT](../../LICENSE)
