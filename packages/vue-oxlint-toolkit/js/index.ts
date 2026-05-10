import type { Comment, Diagnostic, Range } from '@oxlint/plugins'
import { nativeParse } from '../bindings'
import { getConvertor } from './location'
import { transformJsx } from './transform'

export { transformJsx } from './transform'

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
  // ast: AST.ESLintProgram (the import of AST brings a lot of unnecessary types definition in dts, remove it temporarily)
  ast: any
  errors: Diagnostic[]
  panicked: boolean
  transform: ToolkitTransformResult
}

export function parse(_path: string, source: string, _options?: {}): ParseResult {
  const sourceConvertor = getConvertor(source)
  const result = nativeParse(source)

  return {
    transform: transformJsx(source, sourceConvertor),
    ast: null,
    errors: result.errors.map(sourceConvertor.fix),
    panicked: result.panicked,
  }
}
