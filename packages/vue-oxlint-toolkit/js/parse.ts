import { parseVue as nativeParseVue } from '../bindings'
import { createLocator, toLocation } from './locator'
import type { ParseResult } from './parse-result'
import { parseVueSingleFileComponent } from './ast/vue-single-file-component'
import { toProgram } from './vue-ast'

export function parse(_path: string, source: string, _options: object = {}): ParseResult {
  const result = nativeParseVue(source)
  const locator = createLocator(source)
  const sfc = parseVueSingleFileComponent(result.astJson)
  const program = toProgram(sfc, locator)

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
