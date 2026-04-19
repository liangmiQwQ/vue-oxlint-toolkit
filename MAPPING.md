# Vue-Oxc-Toolkit Node Mapping

This document explains how Vue template nodes are transformed into [Oxc](https://github.com/oxc-project/oxc) AST nodes. The toolkit represents Vue templates using a standard JavaScript/JSX AST, enabling use of existing JavaScript tooling.

## SFC Structure

A Vue Single File Component (SFC) is transformed into a standard `Program`. The `Program.body` follows a specific structure:

1. **Top-level Statements**: Contains all imports from both `<script>` and `<script setup>`, as well as all statements from the normal `<script>` block.
2. **Inner Arrow Function Expression**: A single `ArrowFunctionExpression` that encapsulates the scope of `<script setup>`. It is always the last statement in the `Program.body` (wrapped in an `ExpressionStatement`) and its body contains:
   - **Local Bindings**: All non-import statements from the `<script setup>` block.
   - **Structural JSX Fragment**: The last statement in the block, which is an expression statement containing a `JSXFragment` that represents the physical structure of the SFC.

The **Structural JSX Fragment** serves as the "return" of the component's structure, containing:

- Placeholder elements for `<script>` and `<script setup>` to maintain source mapping.
- The `<template>` content, transformed into JSX.
- Other blocks like `<style>` if present.

### Example

```vue
<script>
export default {
  data() {
    return {
      count: 0
    };
  }
}
</script>

<script setup>
import { ref } from 'vue';

const count = ref(0);
</script>

<template>
  <div>{{ count }}</div>
</template>
```

```jsx
import { ref } from 'vue';

export default {
  data() {
    return {
      count: 0
    };
  } 
};

async () => {
  const count = ref(0);
  <>
    <script></script>
    <script setup></script>
    <template>
      <div>{ count }</div>
    </template>
  </>;
};
```

## Elements and Components

Vue elements are mapped to `JSXElement` or `JSXFragment`.

- **HTML Elements** (`<div>`): Mapped to `JSXOpeningElement` with a lowercased `JSXIdentifier`.
- **Components** (`<MyComponent />`): Mapped to `JSXOpeningElement` with a `JSXIdentifierReference`.
- **Namespaced Components** (`<motion.div />`): Mapped to `JSXOpeningElement` with a `JSXMemberExpression`.
- **Kebab-case Components** (`<my-component />`): Transformed to PascalCase (`MyComponent`) as a `JSXIdentifierReference`.

### Closing Elements

The `closing_element` of a `JSXElement` is determined by the tag syntax:

- **Self-Closing Tags** (`<div />`, `<img />`, `<Component />`): Have a `JSXClosingElement` with an **empty element name** (`name: ""`). This distinguishes them from void tags.
- **Void Tags without Self-Closing Syntax** (`<br>`, `<input>`, `<img>`): Have `closing_element: None`.
- **Normal Tags with Explicit Closing** (`<div></div>`): Have a `JSXClosingElement` with the proper element name.

### Example

```vue
<template>
  <img />
  <input>
  <div></div>
</template>
```

```jsx
<template>
  <img></>
  <input>
  <div></div>
</template>
```

## Attributes and Directives

Attributes are mapped to `JSXAttributeItem`.

- **Static Attributes** (`class="foo"`): Mapped to `JSXAttribute` with a `StringLiteral` value.
- **Directives** (`v-bind`, `v-on`, `v-slot`): Mapped to `JSXAttribute` where the name is a `JSXNamespacedName`.
  - **Namespace**: The directive type (e.g., `v-bind`, `v-on`, `v-slot`).
  - **Name**: The argument (e.g., `class`, `click`).
  - **Shorthands**: Normalized to full names (`:` -> `v-bind`, `@` -> `v-on`, `#` -> `v-slot`).
  - **Value**: Wrapped in `JSXExpressionContainer`. If the directive only has a name (e.g., `v-else`), use `None`.

### v-bind Shorthand Without Value

When `:prop` is used without a value, the toolkit synthesizes an identifier reference matching the prop name. Dashed prop names are normalized to camelCase.

| Template  | Synthesized JSX attribute |
| --------- | ------------------------- |
| `:id`     | `:id="id"`                |
| `:msg-id` | `:msg-id="msgId"`         |

### v-bind Without Argument (Spread)

`v-bind="expr"` (or its shorthand `:="expr"`) has no argument — it spreads an object onto the element. This maps directly to a `JSXSpreadAttribute` (`{...expr}`).

| Template               | JSX                |
| ---------------------- | ------------------ |
| `<div v-bind="obj" />` | `<div {...obj} />` |
| `<div :="obj" />`      | `<div {...obj} />` |

### Dynamic Arguments

Dynamic arguments (e.g., `:[arg]="val"`) are wrapped in brackets within the `JSXNamespacedName` or handled via `ObjectExpression` when transformed.

## Structural Transformations

Some directives require structural changes to represent Vue's logic in JSX.

### `v-if` / `v-else-if` / `v-else`

Conditional chains are transformed into nested `ConditionalExpression` nodes wrapping the elements.

### Example

```vue
<div v-if="ok" />
<p v-else />
```

The parent's children will contain a `JSXExpressionContainer` with a ternary operator: `ok ? <div v-if:={}/> : <p v-else:/>`.

---

### `v-for`

Transformed into a `CallExpression` on the data source, wrapping the element in an `ArrowFunctionExpression`.

### Example

```vue
<div v-for="item in items" :key="item.id" />
```

- The list is wrapped in `items(item => <div />)`.
- The element inside the arrow function body retains the `v-for` attribute (with `JSXEmptyExpression`) to keep the source mapping.

---

### `v-slot`

Slots are collected into an `ObjectExpression` within the `children` of a component. Each property in the object represents a slot.

### Example

```vue
<Comp>
  <template #header="{ message }">
    {{ message }}
  </template>

  <template #[id]="{ message }">
    {{ message }}
  </template>
</Comp>
```

The `Comp` element's children contain a `JSXExpressionContainer` holding an `ObjectExpression`:

```jsx
<template>{{
  header: ({ message }) => <>{ message }</>
}}<template>

<template>{{
  [id]: ({ message }) => <>{ message }</>
}}</template>
```

## Text and Interpolation

- **Plain Text**: Mapped to `JSXText`.
- **Interpolation** (`{{ msg }}`): Mapped to `JSXExpressionContainer` containing the JavaScript expression.

## Comments

Template comments are captured as AST comments. They are represented by empty `JSXExpressionContainer` nodes to maintain their relative position in the tree.

Comments in JavaScript will be just treated as normal comments, collecting them in the `Program`.
