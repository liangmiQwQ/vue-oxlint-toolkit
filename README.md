# Vue OXC Toolkit

A high-performance toolkit to generate semantically correct AST from Vue SFCs for code linting purposes.

## Features

- **Full SFC Parsing**: Support for both composition and options API. Support parsing `<template>` block and TypeScript code as well.
- **JSX/TSX Transformation**: Transforms Vue templates into OXC-compatible JSX/TSX AST, enabling deep semantic analysis.
- **Linter Ready**: Automatically generates metadatas which are required by `oxc_linter` (such as`module_record` and `irregular_whitespaces`).
- **High Performance**: Complete the AST transformation in a single traversal.
- **Error Handling**: Collect the errors from both `vue-compiler-rs` and `oxc-parser`. Implement similar `paincked` logic like `oxc-parser`

## Testing

The project includes comprehensive tests:

```bash
# Run tests
just test

# Generate coverage report
just coverage
```

Current test coverage: **95.61%** (lines), **96.12%** (functions), **95.61%** (regions)

## Credits

This project includes a fork of [vue-oxc-parser](https://github.com/zhiyuanzmj/vue-oxc-parser) originally created by zhiyuanzmj.

This project depends on [vue-compiler-rs](https://github.com/HerringtonDarkholme/vue-compiler) which provides underlying support for Vue parsing.

## License

[MIT](./LICENSE) License - see [LICENSE](LICENSE) file for details.
