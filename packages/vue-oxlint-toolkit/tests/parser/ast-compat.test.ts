import { expect, it } from 'vite-plus/test'
import { readTestFiles } from '../utils'
import { AST } from 'vue-eslint-parser'
import { parse, visitorKeys } from '../../js'
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

it('exports the same visitor keys as vue-eslint-parser', () => {
  expect(visitorKeys).toEqual(AST.KEYS)
})

for (const testFile of TEST_FILES.pass) {
  it(`should produce the same normalized AST as vue-eslint-parser: ${testFile.path}`, () => {
    const toolkitParseResult = parse(testFile.path, testFile.source_text, VUE_ESLINT_PARSER_OPTION)
    const vueEslintParserResult = vueEslintParser.parse(
      testFile.source_text,
      VUE_ESLINT_PARSER_OPTION,
    )
    delete vueEslintParserResult.errors

    expect(toolkitParseResult.ast).toEqual(expect.objectContaining(vueEslintParserResult))
  })
}
