# vue_oxlint_parser

A Vue parser generated [AST](https://github.com/vuejs/vue-eslint-parser/blob/master/docs/ast.md) for linting purpose.

## Feature

-

Rust port of [vue-eslint-parser](https://github.com/vuejs/vue-eslint-parser).

The [`vue_oxlint_jsx`](../vue_oxlint_jsx) crate consumes this AST and converts it into an OXC-compatible `Program` for downstream linting.

## Credits

AST tests are copied from [vue-eslint-parser](https://github.com/vuejs/vue-eslint-parser/tree/master/test/fixtures/ast).

## License

[MIT](../../LICENSE)
