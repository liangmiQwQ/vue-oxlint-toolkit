# vue_oxlint_jsx

A transformer that generates JS/TSX representation for the AST returned by [`vue_oxlint_parser`](https://github.com/liangmiQwQ/vue-oxlint-toolkit/tree/main/crates/vue_oxlint_parser), for use in Oxlint's script checking.

## Features

- **Linter Ready** Automatically generates metadatas which are required by `oxc_linter` (such as `module_record` and `irregular_whitespaces`).
- **High Performance** Complete the AST transformation in a single traversal.

## Credits

This crate includes a fork of [vue-oxc-parser](https://github.com/zhiyuanzmj/vue-oxc-parser) by [zhiyuanzmj](https://github.com/zhiyuanzmj).

## License

[MIT](../../LICENSE)
