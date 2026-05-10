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
      convertor.fix(token),
    )
    const templateBodyToken = (nativeParseResult.templateTokens as AST.Token[]).map((token) =>
      convertor.fix(token),
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
