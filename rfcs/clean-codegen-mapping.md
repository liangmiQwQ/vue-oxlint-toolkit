# RFC: Clean Node Copying for Codegen

The current codegen is very confusing, it generates ast from parsed ast, make downstream confused and lost details, also provide bad mappings.

## Core Idea

Use clean/dirty nodes solutions, clean node means codegen can just copy source code instead of generating it. Dirty node means codegen should generate code normally.

**Default assumption: Every node is dirty.**

Only nodes coming from source AST (via `clone_in` or the single `oxc_parse` call in script.rs) are marked **clean**.

## Clean Tracking

Store clean nodes by **Span** in a `CleanSet<Span>`.

Why Span? Node addresses are unstable during AST construction. Span is stable and sufficient for lookup.

## The One Rule for Codegen

When printing a node:

```
if node.span is in CleanSet:
    // Clean node - entire subtree is clean
    emit original source text at node.span
    add one mapping: generated_range -> node.span
    return (do NOT traverse children)
else:
    // Dirty node - normal codegen
    traverse and print children normally
    if node has a span (even if dirty):
        emit generated syntax, and add a mapping, just like the code now
    else:
        emit generated syntax
```

That's it. No recursive checking. No dirty propagation. No parent marking.

## Why This Works

The core rule: Clean nodes can only include clean nodes, dirty nodes can include both clean and dirty nodes.

- Clean subtree = all nodes inside are also clean (guaranteed by construction, because you never modify clean nodes or insert dirty nodes into them)
- Codegen never looks inside a clean node → no overlapping mappings
- Dirty nodes that wrap clean content generate their own code normally, inner clean node handles itself, so inner node inside dirty nodes are totally safe.

## Implementation Notes

1. **Default dirty** - Only explicit marking adds to CleanSet
2. **Mark at clone time** - When `clone_in` happens, add the resulting node's span to CleanSet
3. **Mark parsed boundary** - The single `oxc_parse` call: iterate statements/directives, add each span to CleanSet
4. **Codegen** - Before printing any node, check CleanSet by span. If present, copy source and skip children traverse/codegen

## What Codegen Must Support

- Ability to emit raw source text given a span
- Ability to skip subtree traversal on demand
- Ability to emit a span's source even for dirty nodes (but still traverse children)

## What This Eliminates

- Recursive dirty checking
- Parent-to-child dirty propagation
- Per-node enter/leave mapping
- Overlapping mapping entries
