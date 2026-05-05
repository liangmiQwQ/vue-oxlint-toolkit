import { parseVue as nativeParseVue } from '../bindings'
import { createLocator, toLocation } from './locator'
import type { AstNode, JsonObject, ParseResult } from './types'
import { toProgram } from './vue-ast'

export function parse(_path: string, source: string, _options: object = {}): ParseResult {
  const result = nativeParseVue(source)
  const locator = createLocator(source)
  const sfc = parseNativeAst(result.astJson)
  const program = toProgram(sfc, source, locator)

  return {
    ast: program,
    errors: result.errors.map((error) => ({
      message: error.message,
      loc: toLocation(error, locator),
    })),
    panicked: result.panicked,
    transform: null,
  }
}

function parseNativeAst(astJson: string): AstNode {
  const parsed: unknown = JSON.parse(astJson)
  if (!isObject(parsed)) {
    throw new TypeError('Native Vue parser returned a non-object AST payload.')
  }

  return parsed
}

function isObject(value: unknown): value is JsonObject {
  return value !== null && typeof value === 'object'
}
