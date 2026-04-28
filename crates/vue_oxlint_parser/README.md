# vue_oxlint_parser

A Vue parser generated [AST](https://github.com/vuejs/vue-eslint-parser/blob/master/docs/ast.md) for linting purpose.

## Features

- **Good Compatibility** Generate the same AST as [vue-eslint-parser](https://github.com/vuejs/vue-eslint-parser), keep error messages as same as possible.
- **JavaScript Support** Use `serde` to make sure AST is available on JavaScript side via `napi`, make existing ESLint rules can be reused.
- **High Performance** Follow the tokenization/parsing architecture, use `oxc` for inner script parsing.

## Credits

AST tests are copied from [vue-eslint-parser](https://github.com/vuejs/vue-eslint-parser/tree/master/test/fixtures/ast).

## License

[MIT](../../LICENSE)
