import type { Comment, Diagnostic, Range } from '@oxlint/plugins'

import { nativeParse } from '../bindings/index.js'
import { getConvertor } from './location.ts'
import { rebuildProgram } from './parse/index.ts'
import { transformJsx } from './transform.ts'

export { transformJsx } from './transform.ts'
export { visitorKeys } from './parse/visitor-keys.ts'
export * from './ast/index.ts'

export interface Mapping {
  virtualStart: number
  virtualEnd: number
  originalStart: number
  originalEnd: number
}

export interface ToolkitTransformResult {
  sourceText: string
  scriptKind: 'jsx' | 'tsx'
  comments: Comment[]
  irregularWhitespaces: Range[]
  errors: Diagnostic[]
  mappings: Mapping[]
}

export interface ParseResult {
  // Ast: AST.ESLintProgram (the import of AST brings a lot of unnecessary types definition in dts, remove it temporarily)
  ast: any
  errors: Diagnostic[]
  panicked: boolean
  transform: ToolkitTransformResult
}

export function parse(_path: string, source: string, _options?: object): ParseResult {
  const sourceConvertor = getConvertor(source)
  const result = nativeParse(source)
  const sfc = JSON.parse(result.astJson)

  return {
    transform: transformJsx(source, sourceConvertor),
    ast: rebuildProgram(sfc, sourceConvertor),
    errors: result.errors.map(sourceConvertor.fix),
    panicked: result.panicked
  }
}
