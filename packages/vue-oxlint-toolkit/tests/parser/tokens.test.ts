import tsParser from '@typescript-eslint/parser'
import { expect, it } from 'vite-plus/test'
import type { AST } from 'vue-eslint-parser'
import vueEslintParser from 'vue-eslint-parser'

import { nativeParse } from '../../bindings/index.js'
import { getConvertor } from '../../js/location.ts'
import { readTestFiles } from '../utils.ts'

const TEST_FILES = readTestFiles()
const VUE_ESLINT_PARSER_OPTION = {
  sourceType: 'module',
  ecmaVersion: 'latest',
  ecmaFeatures: {
    jsx: true
  },
  parser: tsParser,
  parserOptions: {
    ecmaVersion: 'latest',
    sourceType: 'module'
  }
}

for (const testFile of TEST_FILES.pass) {
  it('should produce the same tokens as vue-eslint-parser', () => {
    const convertor = getConvertor(testFile.source_text)
    const nativeParseResult = JSON.parse(nativeParse(testFile.source_text).astJson)
    const bodyTokens = (nativeParseResult.scriptTokens as AST.Token[]).map(token =>
      normalizeToken(convertor.fix(token))
    )
    const templateBodyToken = (nativeParseResult.templateTokens as AST.Token[]).map(token =>
      normalizeToken(convertor.fix(token))
    )

    const vueEslintParserResult = vueEslintParser.parse(
      testFile.source_text,
      VUE_ESLINT_PARSER_OPTION
    )

    expect(bodyTokens).toEqual(vueEslintParserResult.tokens)

    // To avoid no `<template>` scripts
    if (vueEslintParserResult.templateBody) {
      // oxlint-disable-next-line vitest/no-conditional-expect -- Only template fixtures expose template tokens.
      expect(templateBodyToken).toEqual(vueEslintParserResult.templateBody.tokens)
    }
  })
}

// Oxlint's token has start and end field, while eslint's do not have.
function normalizeToken<T extends AST.Token & { start?: number; end?: number }>(token: T) {
  const { start: _start, end: _end, ...rest } = token

  return rest
}
