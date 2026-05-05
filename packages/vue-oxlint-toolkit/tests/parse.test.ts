import { readdirSync, readFileSync } from 'node:fs'
import { basename, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import { parse as parseWithVueEslintParser } from 'vue-eslint-parser'
import { expect, it } from 'vite-plus/test'
import { parse } from '../js'

const root = fileURLToPath(new URL('../../..', import.meta.url))

it('parses a Vue SFC into an ESLint-style program', () => {
  const source = `<script lang="ts">
const msg: string = 'hello'
</script>

<template>
  <div id="app">{{ msg }}</div>
</template>`
  const result = parse('fixture.vue', source)

  expect(result.panicked).toBe(false)
  expect(result.transform).toBeNull()
  expect(result.errors).toEqual([])
  expect(result.ast.type).toBe('Program')
  expect(result.ast.body[0]).toMatchObject({ type: 'VariableDeclaration' })
  expect(result.ast.templateBody).toMatchObject({
    type: 'VElement',
    name: 'template',
  })
  expect(result.ast.templateBody.tokens.length).toBeGreaterThan(0)
  expect(result.ast.tokens.length).toBeGreaterThan(0)
})

it('adds non-enumerable parent links and loc getters', () => {
  const source = `<template><div>ok</div></template>`
  const result = parse('fixture.vue', source)
  const template = result.ast.templateBody
  const div = template.children[0]

  expect(div.parent).toBe(template)
  expect(Object.keys(div)).not.toContain('parent')
  expect(div.loc.start).toEqual({ line: 1, column: 10 })
  expect(div.range).toEqual([10, 23])
})

it('parses directive attributes as expression containers', () => {
  const source = `<template><button :class="buttonClass" @click="submit">ok</button></template>`
  const result = parse('fixture.vue', source)
  const button = result.ast.templateBody.children[0]

  expect(button.startTag.attributes).toMatchObject([
    {
      type: 'VAttribute',
      directive: true,
      key: {
        name: { name: 'bind' },
        argument: { name: 'class' },
      },
      value: {
        type: 'VExpressionContainer',
        expression: { type: 'Identifier', name: 'buttonClass' },
        references: [{ id: { name: 'buttonClass' }, mode: 'r' }],
      },
    },
    {
      type: 'VAttribute',
      directive: true,
      key: {
        name: { name: 'on' },
        argument: { name: 'click' },
      },
      value: {
        type: 'VExpressionContainer',
        expression: {
          type: 'VOnExpression',
          body: [
            {
              type: 'ExpressionStatement',
              expression: { type: 'Identifier', name: 'submit' },
            },
          ],
        },
        references: [{ id: { name: 'submit' }, mode: 'r' }],
      },
    },
  ])
  expect(button.startTag.variables).toEqual([])
})

it('parses v-for and v-slot bindings as variables', () => {
  const source = `<template><ul><li v-for="(item, index) in items">{{ item }}</li><slot v-slot="{ row }" /></ul></template>`
  const result = parse('fixture.vue', source)
  const ul = result.ast.templateBody.children[0]
  const li = ul.children[0]
  const slot = ul.children[1]

  expect(li.startTag.attributes[0]).toMatchObject({
    directive: true,
    value: {
      type: 'VExpressionContainer',
      expression: {
        type: 'VForExpression',
        right: { type: 'Identifier', name: 'items' },
      },
      references: [{ id: { name: 'items' }, mode: 'r' }],
    },
  })
  expect(li.startTag.variables.map((variable: any) => variable.id.name)).toEqual(['item', 'index'])
  expect(slot.startTag.attributes[0]).toMatchObject({
    directive: true,
    value: {
      type: 'VExpressionContainer',
      expression: { type: 'VSlotScopeExpression' },
    },
  })
  expect(slot.startTag.variables.map((variable: any) => variable.id.name)).toEqual(['row'])
})

it('uses semantic references for nested directive expressions', () => {
  const source = `<template><div v-for="(a, b) in ((x) => x + 1)(y)">{{ a }}</div></template>`
  const result = parse('fixture.vue', source)
  const div = result.ast.templateBody.children[0]
  const expression = div.startTag.attributes[0].value

  expect(expression.references.map((reference: any) => reference.id.name)).toEqual(['y'])
  expect(div.children[0].references.map((reference: any) => reference.id.name)).toEqual(['a'])
})

it('keeps non-error global fixtures structurally compatible with vue-eslint-parser', () => {
  for (const fixture of listVueFixtures(join(root, 'fixtures'))) {
    const relativeFixture = relative(root, fixture)
    if (relativeFixture.includes('/error/') || basename(fixture).includes('error')) {
      continue
    }

    const source = readFileSync(fixture, 'utf8')
    let expected
    try {
      expected = parseWithVueEslintParser(source, { sourceType: 'module' })
    } catch {
      continue
    }
    if (!expected.templateBody) {
      continue
    }

    let actual
    try {
      actual = parse(relativeFixture, source)
    } catch (error) {
      throw new Error(`${relativeFixture}: ${(error as Error).message}`)
    }
    expect(actual.panicked, relativeFixture).toBe(false)
    expect(actual.ast.templateBody, relativeFixture).not.toBeNull()
    expect(elementNames(actual.ast.templateBody), relativeFixture).toEqual(
      elementNames(expected.templateBody),
    )
    expect(actual.ast.templateBody.tokens.length, relativeFixture).toBeGreaterThan(0)
  }
})

function listVueFixtures(dir: string): string[] {
  const fixtures: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      fixtures.push(...listVueFixtures(path))
    } else if (entry.isFile() && path.endsWith('.vue')) {
      fixtures.push(path)
    }
  }
  return fixtures
}

function elementNames(node: any): string[] {
  const names: string[] = []
  visit(node)
  return names

  function visit(value: any) {
    if (!value || typeof value !== 'object') {
      return
    }
    if (value.type === 'VElement') {
      names.push(value.rawName ?? value.name)
    }
    for (const child of value.children ?? []) {
      visit(child)
    }
  }
}
