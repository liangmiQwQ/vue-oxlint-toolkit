import { it, expect } from 'vite-plus/test'
import { transformJsx } from '../js'

it('transforms Vue SFCs to generated JSX', () => {
  const result = transformJsx(`<script setup lang="ts">
const msg: string = 'hello'
</script>

<template>
  <div>{{ msg }}</div>
</template>`)

  expect(result.scriptKind).toBe('tsx')
  expect(result.sourceText).toContain(`const msg: string = 'hello';`)
  expect(result.sourceText).toContain('<div>{msg}</div>')

  // The codegen produces an AST-location-based mapping for every node with a
  // non-zero span. Synthesised wrapper nodes (span 0,0) are skipped.
  expect(result.mappings.length).toBeGreaterThan(0)
  for (const m of result.mappings) {
    expect(m.virtualEnd).toBeGreaterThanOrEqual(m.virtualStart)
    expect(m.originalEnd).toBeGreaterThanOrEqual(m.originalStart)
  }

  // The `{{ msg }}` interpolation refers to source offset 85..88. Its
  // mapping should round-trip the slice through to the generated `msg`.
  const msgMapping = result.mappings.find((m) => m.originalStart === 85 && m.originalEnd === 88)
  expect(msgMapping).toBeDefined()
  expect(result.sourceText.slice(msgMapping!.virtualStart, msgMapping!.virtualEnd)).toBe('msg')
})

it('prints self-closing JSX elements as self-closing', () => {
  const result = transformJsx(`<template>
  <Foo />
</template>`)

  expect(result.sourceText).toContain('<Foo />')
  expect(result.sourceText).not.toContain('<Foo>')
})

it('preserves type-only imports in generated source', () => {
  const result = transformJsx(`<script setup lang="ts">
import type DefaultType from 'default-type'
import type * as Types from 'types'
import { type Foo, Bar, type Baz as RenamedBaz } from 'pkg'
</script>`)

  expect(result.sourceText).toContain(`import type DefaultType from 'default-type';`)
  expect(result.sourceText).toContain(`import type * as Types from 'types';`)
  expect(result.sourceText).toContain(
    `import { type Foo, Bar, type Baz as RenamedBaz } from 'pkg';`,
  )
})

it('returns parser metadata', () => {
  const result = transformJsx(`<template>
  <!-- hello -->
  <div>ok</div>
</template>`)

  expect(result.comments).toMatchObject([
    {
      type: 'Block',
      value: ' hello ',
      range: [17, 24],
      loc: {
        start: { line: 2, column: 6 },
        end: { line: 2, column: 13 },
      },
    },
  ])
  expect(result.irregularWhitespaces).toEqual([])
  expect(result.errors).toEqual([])
})

it('keeps full line comment values', () => {
  const result = transformJsx(`<script>
//a
// hello
</script>`)

  expect(result.comments).toMatchObject([
    {
      type: 'Line',
      value: 'a',
    },
    {
      type: 'Line',
      value: ' hello',
    },
  ])
})

it('converts native byte offsets to JavaScript locations', () => {
  const result = transformJsx(`<script>
const s = "你好" // hello
</script>`)

  expect(result.comments[0]).toMatchObject({
    value: ' hello',
    range: [24, 32],
    loc: {
      start: { line: 2, column: 15 },
      end: { line: 2, column: 23 },
    },
  })
})

it('keeps bogus template comment values', () => {
  const result = transformJsx(`<template>
<! hello>
</template>`)

  expect(result.comments[0]).toMatchObject({
    type: 'Block',
    value: ' hello',
    range: [13, 19],
    loc: {
      start: { line: 2, column: 2 },
      end: { line: 2, column: 8 },
    },
  })
})
