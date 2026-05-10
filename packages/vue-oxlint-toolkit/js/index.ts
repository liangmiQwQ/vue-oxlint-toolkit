import type { Comment, Diagnostic, Range } from '@oxlint/plugins'
import { transformJsx as nativeTransformJsx } from '../bindings'
import { withLoc } from './location'

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

export declare function parse(path: string, source: string, options?: {}): ParseResult

export function transformJsx(source: string): ToolkitTransformResult {
  const result = nativeTransformJsx(source)

  return {
    sourceText: result.sourceText,
    scriptKind: result.scriptKind,
    comments: result.comments.map((comment) => withLoc(source, comment)),
    irregularWhitespaces: result.irregularWhitespaces,
    errors: result.errors.map((error) => withLoc(source, error)),
    mappings: result.mappings,
  }
}
