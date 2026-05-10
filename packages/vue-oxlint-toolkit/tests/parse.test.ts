import { expect, it } from 'vite-plus/test'
import { readTestFiles } from './utils'
import { AST } from 'vue-eslint-parser'
import { nativeParse } from '../bindings'
import { getConvertor } from '../js/location'
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

for (const testFile of TEST_FILES.pass) {
  it('should produce the same tokens as vue-eslint-parser', () => {
    const convertor = getConvertor(testFile.source_text)
    const nativeParseResult = JSON.parse(nativeParse(testFile.source_text).astJson)
    const bodyTokens = (nativeParseResult.scriptTokens as AST.Token[]).map((token) =>
      normalizeToken(convertor.fix(token)),
    )
    const templateBodyToken = (nativeParseResult.templateTokens as AST.Token[]).map((token) =>
      normalizeToken(convertor.fix(token)),
    )

    const vueEslintParserResult = vueEslintParser.parse(
      testFile.source_text,
      VUE_ESLINT_PARSER_OPTION,
    )

    expect(bodyTokens).toEqual(vueEslintParserResult.tokens)

    // To avoid no `<template>` scripts
    if (vueEslintParserResult.templateBody) {
      expect(templateBodyToken).toEqual(vueEslintParserResult.templateBody.tokens)
    }
  })
}

it('should produce v-on handler tokens with statement syntax', () => {
  expectNativeTokens(
    `<template>
  <button @click="count++" />
  <button v-on:click="foo(); bar()" />
  <button @keyup.enter="if (ok) submit()" />
</template>
`,
  )
})

function expectNativeTokens(sourceText: string) {
  const convertor = getConvertor(sourceText)
  const nativeParseResult = JSON.parse(nativeParse(sourceText).astJson)
  const bodyTokens = (nativeParseResult.scriptTokens as AST.Token[]).map((token) =>
    normalizeToken(convertor.fix(token)),
  )
  const templateBodyToken = (nativeParseResult.templateTokens as AST.Token[]).map((token) =>
    normalizeToken(convertor.fix(token)),
  )

  const vueEslintParserResult = vueEslintParser.parse(sourceText, VUE_ESLINT_PARSER_OPTION)

  expect(bodyTokens).toEqual(vueEslintParserResult.tokens)
  expect(templateBodyToken).toEqual(vueEslintParserResult.templateBody?.tokens)
}

// Oxlint's token has start and end field, while eslint's do not have.
function normalizeToken<T extends AST.Token & { start?: number; end?: number }>(token: T) {
  const { start: _start, end: _end, ...rest } = token

  return rest
}
