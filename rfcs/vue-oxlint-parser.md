# RFC: `vue_oxlint_parser` — First-Party Vue SFC Parser

`vue_oxlint_jsx` currently depends on `vue-compiler-core`, which is unmaintained, ships incomplete spans, and has accumulated a tower of patches in the JSX crate to compensate. This RFC proposes implementing `vue_oxlint_parser` as the first-party SFC parser for the toolkit, designed so both `vue_oxlint_jsx` and `packages/vue-oxlint-toolkit` consume the same AST without re-parsing embedded JavaScript.

## Goals

1. Replace `vue-compiler-core` with a Rust-native SFC parser owned by this repo.
2. Produce one canonical AST (`VueSingleFileComponent`) consumed by both downstreams.
3. Parse every embedded JS region exactly once during SFC parsing where practical.
4. Strict, complete spans on every node — no missing locations.
5. Preserve and extend the `clean_spans` mechanism from the clean-codegen-mapping RFC.

## Non-goals

- Sourcemap support (already out of scope for the vendored codegen).
- Vue 2 filter syntax (`{{ x | foo }}`) — emit a diagnostic and skip.
- Non-HTML template preprocessors (`<template lang="pug">` etc.) — emit a diagnostic, leave `children: []`, continue parsing the rest of the SFC.
- Type checking of any kind in this iteration.

## Top-Level AST

```
VueSingleFileComponent {
  children: Vec<VNode>,                 // SFC tags as a flat children list
  script_comments: Vec<Comment>,        // ONLY comments from <script> / <script setup> bodies
  script_tokens: Vec<Token>,            // JS tokens from scripts, collected via TokensParserConfig
  irregular_whitespaces: Box<[Span]>,
  clean_spans: FxHashSet<Span>,
  module_record: ModuleRecord,          // Moved from the current jsx crate
  source_type: SourceType,              // derived from <script (setup) lang>
  errors: Vec<OxcDiagnostic>,
  panicked: bool,                       // unrecoverable parse failure, like oxc_parser
}
```

`Token` is `oxc_parser::Token` (a packed `u128` with `kind()`, `span()`, etc.). Token spans are in original SFC byte-offset space. Consumers mapping to `vue-eslint-parser`-shaped output should include these in `Program.tokens`.

HTML `<!-- -->` comments live as `VComment` nodes in the tree — they are _not_ flattened into `script_comments`. The two comment worlds stay separate; the ESTree adapter on the toolkit side will route script comments to `Program.comments` and leave template comments on their tree positions.

`VNode` variants:

- `VElement { start_tag, end_tag, children, span }`
- `VText { value, span }`
- `VComment { value, span }`
- `VInterpolation { expression: Expression, span }`
- `VCDATA { value, span }`

`VStartTag` carries `name_span`, `attributes: Vec<VAttribute | VDirective>`, and `self_closing: bool`. Every attribute, directive part (name, arg, modifier, value), and quote position gets its own `Span`. This is the main thing `vue-compiler-core` got wrong; getting it right here is what unlocks the JSX-crate simplifications below.

## Embedded JavaScript — Parsed Once

Every embedded JS region is parsed during SFC parsing and stored as an `oxc_ast` node on the V-node it belongs to. Downstream never re-parses.

| Source                                            | Strategy                                                                                             | Stored as                                                        |
| ------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| `<script>` / `<script setup>` body                | `oxc_parser::Parser::parse` on the slice (no wrap)                                                   | `Program` (directives + statements) on the `VElement`            |
| `{{ expr }}`                                      | wrap as `(expr)`, unwrap                                                                             | `Expression` on `VInterpolation`                                 |
| `:foo` / `v-bind` / `v-if` / `v-show` / `v-model` | parse as expression                                                                                  | `Expression` on the directive                                    |
| `v-for="(a,i) in xs"`                             | regex-split on `\s(in\|of)\s`; wrap LHS as `((LHS)=>0)` to recover patterns; parse RHS as expression | `VForDirective { left: Vec<BindingPattern>, right: Expression }` |
| `v-slot:name="(props)"`                           | wrap as `((props)=>0)` to get parameters                                                             | `VSlotDirective { params: Option<Vec<BindingPattern>> }`         |
| `v-on` / `@evt`                                   | parse as statements list with `{ ... }` (BlockStatement) wrap                                        | `VOnExpression`                                                  |

### Reusing the `oxc_parse` mutation trick

The in-place wrap-and-reset pattern in today's `parser/mod.rs::oxc_parse` (writing wrap bytes into the arena buffer, parsing, then resetting) is the foundation of "spans always point to original SFC offsets." This is lifted into `vue_oxlint_parser` essentially verbatim.

## SourceType

`VueSingleFileComponent.source_type` is derived during parsing. We should follow the similar logic in the current jsx crate, parse script's source_type first before doing other things, error on multiple sourceTypes in the one single SFC. This matches how `vue-eslint-parser` + `@typescript-eslint/parser` currently interact.

## Arena Ownership

Two-allocator design with `'b: 'a`:

- `'a` — owns all V\* nodes (`VueSingleFileComponent` and the V-tree).
- `'b` — owns all nodes produced by `oxc_parser` (script `Program`s, embedded `Expression`s, `Statement`s, `BindingPattern`s, etc.) referenced from the V-tree.

The parser's public API takes both allocators (or one of each, depending on the consumer's needs). Consumers:

- **`vue_oxlint_jsx`** uses the `'b` arena to allocate the emitted JSX `Program`, sharing it with the parsed JS nodes it incorporates by reference.
- **`packages/vue-oxlint-toolkit`** only reads/copies the AST during JSON serde to the JS side, so it does not allocate further into `'b`.

This is unproven — flagging as a design risk to validate during phase 1.

## Cross-Boundary Serialization

The toolkit's napi layer constructs the `vue-eslint-parser`-shaped `Program` view on the Rust side from `VueSingleFileComponent`, then serializes to JSON via `serde_json` and hands it to JS. JSON is the v1 format; binary formats (rkyv, postcard) and lazy node-handle APIs are deferred until profiling shows they are needed.

### Two location kinds

`vue-eslint-parser` nodes carry both `range: [start, end]` (UTF-16 offsets) and `loc: { start: {line, column}, end: {line, column} }`. Plan:

- Keep raw `Span` (UTF-8 byte offsets) on every V-node in Rust.
- Build a `LineColumnIndex` (line-start table + UTF-8↔UTF-16 conversion) once per source inside the toolkit's serde layer.
- Resolve `(range, loc)` lazily as nodes are serialized. The conversion logic currently in `js/index.ts::createLocator` moves into Rust so it happens once, not per-rule.

### CRLF normalization

`vue-eslint-parser` normalizes `\r\n` → `\n` for `loc` calculation but keeps `range` against the original source. `LineColumnIndex` handles this explicitly.

### Entity decoding

Spans point at the _raw_ source range; the _decoded_ string is a separate field on `VText` / attribute values. The ESTree adapter exposes both, matching `vue-eslint-parser`.

## Error Handling

Mirror `oxc_parser`'s semantics:

- Recoverable errors are pushed into `errors` and parsing continues.
- Unrecoverable structural errors (unclosed `<template>`, etc.) set `panicked: true` and abort.
- Script syntax errors do not panic the SFC parse — the relevant block's `body` becomes empty/partial; template parsing continues.
- Multiple `<template>` / `<script>` / `<script setup>` blocks: emit a diagnostic, keep the first of each, ignore the extras.

Lexing modes (raw-text for `<script>`/`<style>`/`<textarea>`/`<title>`, foreign content for `<svg>`/`<math>`) follow `vue-eslint-parser`'s behavior exactly.

## `clean_spans` Continuity

`clean_spans: FxHashSet<Span>` is populated as top-level script statements and directives are parsed (same rule as today: nodes coming directly from a single `oxc_parser` call are clean). It rides along on the parser return; codegen consumes it unchanged. The clean-codegen-mapping RFC's invariants are preserved.

## What This Buys the JSX Crate

Once every V-node has a real span and embedded JS is pre-parsed:

- `elements/v_for.rs` loses its regex + wrap-and-parse logic — consumes `VForDirective.{left, right}` directly.
- `elements/v_slot.rs` loses its wrap logic — consumes `params` directly.
- `elements/directive.rs` and `elements/mod.rs` shed the "find where this attribute's value actually starts" span-reconstruction patches.
- `script.rs` becomes thinner — script `Program` already arrives parsed; just merge `global` + `setup` and stitch in the SFC-struct JSX statement.
- `irregular_whitespaces.rs` and `modules.rs` become near pass-throughs.
- v-on gains a real implementation: `($event-less) => { stmts }` arrow wrappers in JSX output (`() => { ... }` block-statement form) so statement-list handlers stop being silently dropped.

`ParserImpl` shrinks to a "V-tree → JSX `Program` transformer." The mutable buffer / `oxc_parse` trick moves out of the JSX crate into `vue_oxlint_parser`, where it belongs.

## Module Layout

```
crates/vue_oxlint_parser/src/
├── lib.rs                   # public API: parse_sfc(), VueSfcParserReturn
├── ast.rs                   # all AST types
├── irregular_whitespaces.rs # collect_irregular_whitespaces utility
└── parser/
    ├── mod.rs               # Parser struct, oxc_parse_with_wrap trick, parse_impl
    ├── element.rs           # VNode/VElement construction (consumes lexer output)
    ├── attribute.rs         # VAttribute / VDirective construction
    ├── script.rs            # <script> block parsing
    ├── expression.rs        # embedded JS: interpolations, directives, v-for, v-slot, v-on
    └── lexer/
        └── mod.rs           # byte-level scanning helpers (impl Parser<'a>)
```

The `lexer/` sub-module follows the same `impl Parser<'a>` pattern used by `vue_oxlint_jsx/parser/elements/`. It owns all low-level source navigation (`current_byte`, `matches`, `advance`, `skip_whitespace`, etc.) so that `element.rs` and `attribute.rs` stay focused on AST construction.

## Migration Phases

1. Tokenizer + minimal V-tree (`VElement`, `VText`, `VComment`, attributes), no embedded JS yet. Validate span fidelity against existing snapshots in `crates/vue_oxlint_jsx/test/snapshots`.
2. Add `<script>` / `<script setup>` parsing + module record + comments + `clean_spans` + `source_type`.
3. Add interpolation and pure-expression directives (`v-bind`, `v-if`, `v-show`, `v-model`, basic `v-on`).
4. Add `v-for`, `v-slot`, and `v-on` statement-list form.
5. Switch `vue_oxlint_jsx::ParserImpl` to consume the V-tree instead of `vue-compiler-core`. Element handlers shed re-parsing logic.
6. Wire the napi package: V-tree → `vue-eslint-parser`-shaped `Program` adapter on the Rust side, serialized via `serde_json`, exposed alongside the existing `transformJsx`.
7. Drop the `vue-compiler-core` dependency.

Each phase keeps the JSX crate's existing test suite green; regressions surface immediately.

## Open Questions

- The two-allocator (`'b: 'a`) ownership scheme is unproven in this codebase.
