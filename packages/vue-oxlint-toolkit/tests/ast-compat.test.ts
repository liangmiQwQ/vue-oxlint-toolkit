import { expect, it } from 'vite-plus/test'
import { readTestFiles } from './utils'
import { AST } from 'vue-eslint-parser'
import { nativeParse } from '../bindings'
import { visitorKeys } from '../js'
import vueEslintParser from 'vue-eslint-parser'
import tsParser from '@typescript-eslint/parser'

const TEST_FILES = readTestFiles()
const VUE_ESLINT_PARSER_OPTION = {
  sourceType: 'module',
  ecmaVersion: 'latest',
  ecmaFeatures: {
    jsx: true,
  },
  parser: tsParser,
  parserOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module',
  },
}

const SEMANTIC_KEYS = [
  'async',
  'computed',
  'directive',
  'exportKind',
  'generator',
  'importKind',
  'kind',
  'method',
  'name',
  'namespace',
  'operator',
  'optional',
  'prefix',
  'raw',
  'rawName',
  'selfClosing',
  'shorthand',
  'sourceType',
  'static',
  'style',
  'value',
]

it('exports the same visitor keys as vue-eslint-parser', () => {
  expect(visitorKeys).toEqual(AST.KEYS)
})

it('normalizes AST nodes through visitor keys and semantic fields', () => {
  expect(
    normalizeAstForCompatibility({
      type: 'VElement',
      name: 'div',
      rawName: 'div',
      range: [0, 11],
      startTag: {
        type: 'VStartTag',
        selfClosing: false,
        range: [0, 5],
        attributes: [],
      },
      children: [
        {
          type: 'VText',
          value: 'hello',
          range: [5, 10],
        },
      ],
      endTag: {
        type: 'VEndTag',
        range: [10, 11],
      },
      parent: {},
      tokens: [],
    }),
  ).toEqual({
    type: 'VElement',
    range: [0, 11],
    name: 'div',
    rawName: 'div',
    startTag: {
      type: 'VStartTag',
      range: [0, 5],
      selfClosing: false,
      attributes: [],
    },
    children: [
      {
        type: 'VText',
        range: [5, 10],
        value: 'hello',
      },
    ],
    endTag: {
      type: 'VEndTag',
      range: [10, 11],
    },
  })
})

it.skip('should produce the same normalized AST as vue-eslint-parser', () => {
  for (const testFile of TEST_FILES.pass) {
    const nativeParseResult = JSON.parse(nativeParse(testFile.source_text).astJson)
    const vueEslintParserResult = vueEslintParser.parse(
      testFile.source_text,
      VUE_ESLINT_PARSER_OPTION,
    )

    expect(normalizeAstForCompatibility(nativeParseResult)).toEqual(
      normalizeAstForCompatibility(vueEslintParserResult),
    )
  }
})

function normalizeAstForCompatibility(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(normalizeAstForCompatibility)
  }

  if (!isAstNode(value)) {
    return value
  }

  const node = value as AstNode
  const keys = visitorKeys[node.type as keyof typeof visitorKeys]

  if (!keys) {
    throw new Error(`Missing visitor keys for ${node.type}`)
  }

  const normalized: Record<string, unknown> = {
    type: node.type,
  }

  copyOwnKey(normalized, node, 'range')
  copyOwnKey(normalized, node, 'loc')

  for (const key of SEMANTIC_KEYS) {
    copyOwnKey(normalized, node, key)
  }

  for (const key of keys) {
    copyOwnKey(normalized, node, key)
  }

  return normalized
}

function copyOwnKey(target: Record<string, unknown>, source: AstNode, key: string) {
  if (Object.hasOwn(source, key)) {
    target[key] = normalizeAstForCompatibility(source[key])
  }
}

function isAstNode(value: unknown): value is AstNode {
  return typeof value === 'object' && value !== null && typeof (value as AstNode).type === 'string'
}

type AstNode = Record<string, unknown> & {
  type: string
}
