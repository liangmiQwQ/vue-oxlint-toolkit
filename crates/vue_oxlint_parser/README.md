# vue_oxlint_parser

A `vue-eslint-parser` compatible Vue parser, based on [Oxc](https://github.com/oxc-project/oxc).

## Feature

- **Serialize**: Use `oxc_estree` to serialize vue-eslint-parser compatible AST to JSON for napi use.
- **Two-Arena Architecture**: Use two `allocators`, one for V\* nodes and one for JS Oxc Nodes, easy to manage memory.
- **High Performance**: Follow lexer / parser design mode, built on top of the fastest JavaScript parser [Oxc](https://github.com/oxc-project/oxc).

## License

[MIT](../../LICENSE)
