import { it, expect } from 'vite-plus/test'
import { transformJsx } from '../js'

it('transforms Vue SFCs to generated JSX', () => {
  const source = `<script setup lang="ts">
const msg: string = 'hello'
</script>

<template>
  <div>{{ msg }}</div>
</template>`
  const result = transformJsx(source)

  expect(result.scriptKind).toBe('tsx')
  expect(result.sourceText).toContain("const msg: string = 'hello'")
  expect(result.sourceText).toContain('<div>{msg}</div>')

  const originalStart = source.indexOf('msg }}</')
  const virtualStart = result.sourceText.indexOf('{msg}') + 1

  expect(result.mappings).toContainEqual({
    virtualStart,
    virtualEnd: virtualStart + 'msg'.length,
    originalStart,
    originalEnd: originalStart + 'msg'.length,
  })
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

it('converts native byte offsets in mappings', () => {
  const source = `<script setup>
const 你好 = 1
</script>
<template>{{ 你好 }}</template>`
  const result = transformJsx(source)
  const originalStart = source.indexOf('const 你好')
  const virtualStart = result.sourceText.indexOf('const 你好')

  expect(result.mappings).toContainEqual({
    virtualStart,
    virtualEnd: virtualStart + 'const 你好'.length,
    originalStart,
    originalEnd: originalStart + 'const 你好'.length,
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
