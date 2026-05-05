# Architecture Adjustment Guide: Moving AST Normalization to Rust

## Goal

Eliminate nearly all AST shape-fixing logic from `packages/vue-oxlint-toolkit/js/vue-ast.ts`. The Rust crate `vue_oxlint_parser` should emit JSON that is already structurally compatible with `vue-eslint-parser`, so the JS side only needs:

1. `JSON.parse` the native payload.
2. Assemble `Program.body` by lifting script statements (and computing `Program` span).
3. Inject `parent` pointers and `loc` getters (byte-offset → line/column conversion).
4. Assign the two pre-built token arrays to the correct fields (`program.tokens`, `templateBody.tokens`).

No recursive tree rewriting. No re-computation of `references` or `variables`. No token-stream rebuilding. No field deletion (`scrubOxcOnlyDefaults`).

---

## Correct Responsibility Split

### Rust side (`crates/vue_oxlint_parser`)

- **Lexer** emits tokens in the exact shape `vue-eslint-parser` expects.
- **Parser** builds an AST that is already compatible; any structural merge/adjustment happens during parsing, not in JS.
- **ESTree serialization** writes final field names and values directly (e.g., `value` instead of `text`, `directive: false` instead of a separate `VPureAttribute` type).
- **Semantic analysis** (`oxc_semantic`) produces `references` and `variables`; they are attached to the correct nodes before JSON serialization.

### JS side (`packages/vue-oxlint-toolkit`)

- **`parse.ts`** — entry point: calls native parse, runs `JSON.parse`, calls `toProgram`.
- **`vue-ast.ts`** — reduced to:
  - `toProgram`: lift `VPureScript.body` into `Program.body`, compute `Program` span, choose `templateBody`, wire token/comments arrays.
  - `attachMetadata`: inject `parent` and `loc` (the only things that truly require JS-side runtime info).
- **`locator.ts`** — UTF-8 byte offset → UTF-16 index/line/column mapping.
- **`types.ts`** — type definitions.

---

## 1. Rust-side Serialization Changes

### 1.1 `VText` — rename field

**Current**: Rust serializes `text`.
**Required**: Serialize `value` directly.

```rust
// crates/vue_oxlint_parser/src/ast/nodes/elements.rs
impl ESTree for VText<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VText"));
    state.serialize_field("value", &self.text);   // <-- "value", not "text"
    state.serialize_span(self.span);
    state.end();
  }
}
```

**JS-side impact**: Delete the `VText` text→value conversion in `normalizeCurrentNode`.

---

### 1.2 `VElement` — add `namespace` and lowercase `name`

**Current**: JS adds `namespace` and mutates `name = rawName.toLowerCase()`.
**Required**: Rust sets these during serialization.

```rust
impl ESTree for VElement<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VElement"));
    state.serialize_field("name", &self.name.to_lowercase()); // <-- ensure lowercase
    state.serialize_field("rawName", &self.raw_name);
    state.serialize_field("namespace", &"http://www.w3.org/1999/xhtml");
    state.serialize_field("style", &(self.name.eq_ignore_ascii_case("style")));
    // ... rest
  }
}
```

**JS-side impact**: Remove `namespace`, `name` overwrite, and `style` flag injection.

---

### 1.3 `VPureAttribute` → serialize as `VAttribute` (`directive: false`)

**Current**: Rust serializes `type: "VPureAttribute"`. JS rewrites it to `VAttribute` + `directive: false`.
**Required**: Rust should directly emit `VAttribute`.

```rust
impl ESTree for VPureAttribute<'_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VAttribute"));
    state.serialize_field("directive", &false);
    state.serialize_field("key", &self.key);
    state.serialize_field("value", &self.value);
    state.serialize_span(self.span);
    state.end();
  }
}
```

**JS-side impact**: Delete the `VPureAttribute` branch entirely from `normalizeCurrentNode`. Delete `convertShorthandBindAttribute` JS logic; handle shorthand bind in Rust instead (see 2.2).

---

### 1.4 `VStartTag` — omit empty `variables`

**Current**: JS deletes `variables` when empty.
**Required**: Rust `ESTree` impl should conditionally skip the field if empty.

```rust
impl ESTree for VStartTag<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    let mut state = serializer.serialize_struct();
    state.serialize_field("type", &JsonSafeString("VStartTag"));
    state.serialize_field("attributes", &self.attributes);
    if !self.variables.is_empty() {
      state.serialize_field("variables", &self.variables);
    }
    state.serialize_field("selfClosing", &self.self_closing);
    state.serialize_span(self.span);
    state.end();
  }
}
```

**JS-side impact**: Remove the `delete startTag.variables` block.

---

### 1.5 `VDirectiveKey` — `argument` null when empty

**Current**: JS replaces empty-string `VIdentifier` with `null`.
**Required**: Rust `VDirectiveArgument::VIdentifier` serialization should emit `null` when `name.is_empty()`.

```rust
impl ESTree for VDirectiveArgument<'_, '_> {
  fn serialize<S: Serializer>(&self, serializer: S) {
    match self {
      VDirectiveArgument::VDirectiveArgument(expr) => expr.serialize(serializer),
      VDirectiveArgument::VIdentifier(ident) => {
        if ident.name.is_empty() {
          serializer.buffer_mut().print_str("null");
        } else {
          ident.serialize(serializer);
        }
      }
    }
  }
}
```

**JS-side impact**: Remove the `VDirectiveKey` argument-nullification branch.

---

### 1.6 Dynamic directive arguments (`[foo]`)

**Current**: JS detects `[...]` in argument names and synthesizes a `VExpressionContainer`.
**Required**: Rust should build and serialize the `VExpressionContainer` directly.

When parsing `: [foo] = expr`, the parser should construct:

```rust
VDirectiveArgument::VDirectiveArgument(Box::new(VDirectiveArgumentExpression {
  expression: Expression::Identifier(...),
  references: ...,
  span: ...,
}))
```

**JS-side impact**: Delete `createDynamicArgument` and `isDynamicArgument`.

---

### 1.7 `VExpressionContainer.references`

**Current**: JS re-computes `references` via `collectExpressionReferences`.
**Required**: Rust should already attach the correct `references` (from `oxc_semantic` for expressions, and inline construction for `v-for` right-hand side / interpolation). JS should trust and forward them without recomputation.

**JS-side impact**: Delete `collectExpressionReferences`, `visitExpression`, `collectPatternNames`, `isLocalReference`, `currentLocals`.

---

### 1.8 `VElement.variables`

**Current**: JS re-collects variables with `collectElementVariables`.
**Required**: Rust already computes these in `element.rs`. Ensure they are serialized correctly. JS must not recompute or delete them.

**JS-side impact**: Delete `collectElementVariables`, `pushVariable`, `collectPatternIdentifiers`, `collectPatternIdentifierInto`.

---

### 1.9 `VLiteral` span excludes `=`

**Current**: JS shifts `start` by `+1` when the source starts with `=`.
**Required**: Rust should set the correct span for `VLiteral.value` so no JS offset fix is needed.

---

## 2. Rust-side Parser Changes

### 2.1 Merge adjacent text nodes in `parse_children`

**Current**: JS merges adjacent `VText` nodes in `mergeAdjacentTextNodes`.
**Required**: Rust `parse_children` should merge consecutive `HTMLText` / `HTMLWhitespace` tokens into a single `VText` node **unless separated by a comment**.

This requires passing `template_comments` ranges into the parser or doing the merge after parsing but before returning.

**JS-side impact**: Delete `mergeAdjacentTextNodes`, `hasCommentBetween`, `textNodeValue`.

---

### 2.2 Handle shorthand bind (`:class`) and bare bind (`:= "value"`) in parser

**Current**: JS detects `:class` (no value) and converts it into a full `VAttribute` with `VDirectiveKey` + `VExpressionContainer` containing an `Identifier`.
**Required**: Rust `parse_attribute` should recognize shorthand bind and emit the fully formed directive node directly.

Similarly, `:= "value"` (bare bind association) should be handled during attribute parsing, not in JS.

**JS-side impact**: Delete `isShorthandBindAttribute`, `convertShorthandBindAttribute`, `mergeBareBindAssociationAttributes`, `isBareBindAssociationAttribute`, `isLiteralAttributeName`, `createBareBindLiteralAttribute`, `camelize`, `createIdentifierExpression`.

---

### 2.3 `scrubOxcOnlyDefaults` — eliminate by fixing serialization

**Current**: JS walks the entire tree deleting fields like `optional: false`, `typeAnnotation: null`, etc.
**Required**: On the Rust side, when serializing Oxc AST nodes (via `oxc_estree`), use custom serializers or wrapper types that **omit** these fields when they hold default values.

For example, instead of serializing an Oxc `Identifier` directly, wrap it in a struct whose `ESTree` impl only emits `optional` when `true`, `typeAnnotation` when `Some`, etc.

**JS-side impact**: Delete `scrubOxcOnlyDefaults` entirely.

---

## 3. Rust-side Token Serialization Changes

The goal: Rust emits `template_tokens` and `script_tokens` that are ready to assign without JS-side rebuilding.

### 3.1 `HTMLTagOpen` / `HTMLEndTagOpen` value

**Current**: JS merges `HTMLTagOpen` + `HTMLIdentifier` into one token with lowercase value.
**Required**: Rust lexer/parser should produce a single token or ensure the `HTMLTagOpen` token itself carries the tag name as `value`.

Alternatively, keep two tokens but let JS only filter/assign, not reconstruct objects.

### 3.2 Discard whitespace inside tags

**Current**: JS skips `HTMLWhitespace` when `inTag === true`.
**Required**: Rust parser should not push tag-internal whitespace into `template_tokens`.

### 3.3 `HTMLLiteral` token expansion

**Current**: JS splits a string-literal attribute value into `"` `Punctuator`, internal JS tokens, and closing `"`.
**Required**: Rust `parse_attribute` or `parse_directive_attribute` should generate these tokens directly and append them to `template_tokens` in the correct order.

### 3.4 Interpolation token expansion (`{{ expr }}`)

**Current**: JS sees `VExpressionStart`, `HTMLText(expressionSource)`, `VExpressionEnd` and tries to tokenize `expressionSource` with a crude `tokensFromExpressionText`.
**Required**: Rust should run the expression through `oxc_parser` with token collection, then insert:

- `VExpressionStart` (value: `"{{"`)
- JS tokens from `oxc_parser`
- `VExpressionEnd` (value: `"}}"`)

This makes JS token handling trivial.

**JS-side impact**: Delete `normalizeTemplateTokens`, `collectExpressionTokensInLiteral`, `tokensForQuotedExpression`, `tokensForDynamicArgument`, `tokensFromExpressionText`, `createTemplatePunctuator`, `markTemplatePunctuator`, `normalizeProgramTokens`, `findScriptTagPairs`, `findTagClose`.

### 3.5 `VExpressionStart` / `VExpressionEnd` value

**Current**: JS hardcodes `value: '{{'` / `'}}'`.
**Required**: Rust token serialization should set `value` to `{{` / `}}`.

### 3.6 `HTMLTagClose` / `HTMLAssociation` / `HTMLSelfClosingTagClose` value

**Current**: JS sets `value: ''`.
**Required**: Rust token serialization sets `value: ''` directly.

### 3.7 Program tokens (`<script>` wrappers)

**Current**: JS searches `templateTokens` for script tag positions, filters `scriptTokens`, and wraps them with synthetic `<script>` / `</script>` punctuator tokens.
**Required**: Rust should build `script_tokens` as the final ordered array including the synthetic wrapper tokens, so JS just assigns `program.tokens = sfc.scriptTokens`.

**JS-side impact**: `toProgram` token assignment becomes a single field copy.

---

## 4. JS-side Retained Logic

After the above changes, `vue-ast.ts` should only contain:

### 4.1 `toProgram(sfc, source, locator)`

- Iterate `sfc.children`.
- Extract `VPureScript.body` into `body` array.
- Replace `VPureScript` children inside `<script>` elements with `VText` nodes (so the template tree stays valid).
- Determine `programStart` / `programEnd` from script spans and body nodes.
- Find the `<template>` element → `templateBody`.
- Attach `comments` and `tokens` to `templateBody` (direct assignment from `sfc.template_comments` / `sfc.templateTokens`).
- Construct `VDocumentFragment` if required by vue-eslint-parser parity.
- Construct `Program` object.
- Return `program`.

### 4.2 `attachMetadata(value, parent, locator)`

- Walk the tree (or use the already-walked structure).
- `Object.defineProperty(..., 'parent', ...)`.
- `Object.defineProperty(..., 'loc', { get() { return { start: locator(start), end: locator(end) }; } })`.
- Convert `start`/`end` byte offsets to `range` if needed.

**No node shape changes. No field deletions. No recomputation.**

---

## 5. Files to Modify / Delete

### Rust (`crates/vue_oxlint_parser`)

| File                                     | Action                                                                                                     |
| ---------------------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `src/ast/nodes/elements.rs`              | Update `ESTree` impls for `VText`, `VElement`, `VStartTag`, `VEndTag`                                      |
| `src/ast/nodes/attribute.rs`             | Change `VPureAttribute` to serialize as `VAttribute`                                                       |
| `src/ast/nodes/directive.rs`             | Fix `VDirectiveArgument` empty/null handling; add dynamic-arg container support                            |
| `src/ast/nodes/javascript.rs`            | Ensure `references` are always present on `VExpressionContainer`-like nodes                                |
| `src/lexer/tokens.rs`                    | Set correct `value` for `VExpressionStart/End`, `HTMLTagClose`, etc.                                       |
| `src/parser/parse/mod.rs`                | Merge adjacent text nodes; pass comments to parser for merge logic                                         |
| `src/parser/parse/children.rs`           | Implement text-node merging after parsing children                                                         |
| `src/parser/parse/attributes.rs`         | Build shorthand-bind and bare-bind nodes fully; emit directive tokens correctly                            |
| `src/parser/oxc_parse.rs`                | Ensure token JSON is generated for expressions and script blocks; build `script_tokens` including wrappers |
| (new) `src/ast/oxc_compat.rs` or similar | Wrapper types around Oxc AST nodes that omit default fields during ESTree serialization                    |

### JS (`packages/vue-oxlint-toolkit`)

| File            | Action                                                                                       |
| --------------- | -------------------------------------------------------------------------------------------- |
| `js/vue-ast.ts` | Massively reduce; delete everything listed above; keep only `toProgram` and `attachMetadata` |
| `js/parse.ts`   | Minor cleanup; may no longer need `parseNativeAst` wrapper                                   |

---

## 6. Testing Strategy After Adjustment

The strict `expect(actual).toEqual(expected)` fixture test should remain. After moving logic to Rust, failures in that test will **directly** point to Rust serialization bugs, not JS rewriting bugs. This makes debugging faster and the architecture honest.

---

## Summary Checklist

- [x] Rust `VText` serializes `value`, not `text`.
- [x] Rust `VElement` serializes `namespace`, lowercased `name`, and `style`.
- [x] Rust `VPureAttribute` serializes as `VAttribute` with `directive: false`.
- [x] Rust `VStartTag` omits `variables` when empty.
- [x] Rust `VDirectiveKey` emits `argument: null` when empty.
- [x] Rust parser handles dynamic arguments (`[foo]`) as `VExpressionContainer`.
- [x] Rust parser handles shorthand bind (`:class`) and bare bind-like attribute forms.
- [x] Rust parser merges adjacent text nodes before returning.
- [x] Rust `references` / `variables` are final; JS does not recompute. (shorthand-bind references constructed inline)
- [x] Rust token output matches vue-eslint-parser format (no JS rebuilding).
- [x] Rust `script_tokens` includes `<script>` / `</script>` wrapper punctuators.
- [x] JS `vue-ast.ts` reduced to `toProgram` + metadata/error injection helpers.
- [x] JS `scrubOxcOnlyDefaults` deleted.
- [x] JS `normalizeTemplateTokens` / `normalizeProgramTokens` deleted.
- [x] JS `collectExpressionReferences` / `collectElementVariables` deleted.
