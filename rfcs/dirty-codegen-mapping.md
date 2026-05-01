# RFC: Dirty Codegen Mapping

## Summary

Replace the current recursive codegen mapping model with a dirty-subtree printer.

The current codegen maps every printed AST node by entering and leaving `Gen` /`GenExpr`. That produces many overlapping mappings and makes reparsed AST span comparison unstable. This RFC proposes a simpler output contract:

- Clean subtrees are copied from the original Vue SFC source by span.
- Dirty subtrees are printed by the custom codegen.
- Only the outermost emitted segment gets a mapping.
- Synthetic nodes with `SPAN` are printed without mapping.

This keeps source fidelity for original JS/TS and makes mappings coarse, stable, and useful for diagnostic remapping.

## Motivation

The toolkit needs to feed generated JS/TSX to downstream JavaScript tooling while mapping diagnostics back to the original Vue SFC. The current implementation uses vendored `oxc_codegen` and records mappings around every generated node. This has
several problems:

- Parent and child nodes often produce identical or overlapping generated ranges.
- Reparsed AST spans do not always match generated mapping ranges exactly.
- The mapping snapshot is large and noisy.
- Preserving original JS/TS syntax becomes a long tail of codegen fidelity fixes.

Most original `<script>` and `<script setup>` code does not need to be regenerated.
If a subtree is unchanged, copying the original source is both more accurate and
easier to map.

## Definitions

### Clean Node

A clean node is a node whose generated output is exactly the node's original source text from the Vue SFC.

Clean nodes can be emitted with:

```rust
span.source_text(original_source)
```

### Dirty Node

A dirty node is a node whose generated output cannot be obtained by slicing the original Vue SFC source at its span.

Dirty nodes include:

- Synthetic wrapper nodes.
- Template-derived JSX nodes.
- Nodes created by Vue directive transforms.
- Nodes whose child was rewritten, inserted, removed, or wrapped.

We can know whether it is a dirty or clean node by checking its origin. Nodes from the AST builder are always dirty node, the others are clean nodes (As they are from oxc_parser).

### Dirty Invariant

The parser/codegen preparation phase must maintain this invariant:

> A clean parent contains only clean descendants. A dirty parent may contain clean or dirty descendants.

With this invariant, codegen only needs to check the current node. It does not
need to ask whether the subtree contains dirty descendants.

## Proposed Design

### Track Dirty Nodes as Side Metadata

Do not modify Oxc AST structs. Maintain a side set keyed by node identity:

```rust
nodes: FxHashSet<NodeKey>,
```

As `node_id` is only initialized after semantic, we have to use memory addresses as `NodeKey`. As the memory addresses will never change after it is added to allocator.

### Use Final AST Addresses

Record addresses after nodes are in their final AST storage. Do not record temporary builder-local addresses.

A safe pattern is:

1. Build the subtree.
2. Attach it to the arena-backed AST.
3. Visit the final subtree and collect concrete node identities into `DirtySet`.

The allocator keeps AST storage stable during a single parse/codegen run, so
address identity is acceptable as non-persistent side metadata.

### Hide Builder Access Behind a Dirty-Aware API

The parser should not freely call the raw AST builder when it creates generated nodes for codegen mode. Instead, route generated-node construction through a helper that can mark the final subtree dirty.

Conceptually:

```rust
self.build_dirty(|builder| {
  // create generated AST subtree
})
```

The helper is responsible for marking the final nodes dirty, not the caller.

We also need to hide `self.ast` in the internal parser to force using `build_dirty` fn.

### Codegen Output Strategy

For every printable boundary:

```text
if node is clean:
  copy original source text for node.span
  push one mapping: generated range -> node.span
  skip children

else if node.span == SPAN:
  print generated syntax
  do not push mapping
  continue through generated structure as needed

else:
  print generated syntax for the whole node
  push one mapping: generated range -> node.span
  skip nested boundary mappings and children walk
```

This makes dirty real-span nodes own their generated segment mapping. Inner nodes do not create additional mappings.

## Oxc AST Visit Notes

`oxc_ast_visit` provides many fine-grained visit hooks such as:

- `visit_function`
- `visit_arrow_function_expression`
- `visit_class`
- `visit_jsx_element`
- `visit_jsx_opening_element`
- `visit_jsx_expression_container`

However, some wrapper enums do not have their own `AstKind`:

- `Expression`
- `Statement`
- `Declaration`
- `JSXChild`
- `JSXAttributeItem`

The visitor matches those wrappers and visits the concrete variant.

For example, both `FunctionDeclaration` and `FunctionExpression` visit the same
underlying `Function` node through `visit_function`; `ArrowFunctionExpression`
has a separate node and hook.

`AstKind` values passed to `enter_node` are temporary enum values. Do not use the
address of the `AstKind` enum itself as node identity. Use the concrete node
reference inside the variant, Oxc node IDs, or `AstKind::unstable_address()`.

This means a visitor-based dirty collector is feasible, but dirty keys should
target concrete AST nodes, not wrapper enums.

## Mapping Contract

Mappings are not a full node-to-node source map. They answer:

> Given a generated diagnostic range, what original Vue SFC range should receive
> the diagnostic?

The mapping list should be interpreted as generated output segments. If a
diagnostic overlaps multiple mappings, consumers can choose the smallest mapping
covering the diagnostic start, or the best overlap.

The mapping contract should not require reparsed AST nodes to match original AST
node spans one-for-one.

## Testing Strategy

Replace full mapping snapshots with focused tests:

- Generated source remains parseable.
- Clean script statements are raw-copied exactly.
- Clean script mappings map generated statement ranges to original script ranges.
- Synthetic wrappers produce no mapping.
- Dirty real-span Vue transform boundaries produce exactly one mapping.
- Diagnostics inside generated output can be remapped to expected Vue ranges.

Keep a small number of snapshot tests for generated source. Avoid snapshotting
the full mapping list for every fixture.

## Migration Plan

1. Introduce `DirtySet` and `NodeKey`.
2. Add dirty-aware construction helpers around current Vue transform sites.
3. Mark all generated template/wrapper/directive subtrees dirty.
4. Update vendored codegen to support raw-copy clean boundaries.
5. Add the nested mapping guard.
6. Replace recursive mapping snapshots with focused mapping assertions.
7. Remove the current per-node `enter_mapping` / `leave_mapping` behavior.

## Decision

Adopt the dirty-subtree codegen model as the next codegen mapping direction.
Implement it conservatively first: clean subtrees raw-copy, dirty boundaries own
one mapping, and nested mappings are suppressed.
