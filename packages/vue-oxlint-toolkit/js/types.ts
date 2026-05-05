import type { Comment, Diagnostic, Range } from '@oxlint/plugins'
import type { AST } from 'vue-eslint-parser'

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

export type OxlintProgram = Omit<AST.ESLintProgram, 'errors'>

export interface ParseResult {
  ast: OxlintProgram
  errors: Diagnostic[]
  panicked: boolean
  transform: ToolkitTransformResult | null
}

export interface SourceLocation {
  line: number
  column: number
}

export interface Locator {
  (offset: number): SourceLocation
  toIndex: (offset: number) => number
}

export type JsonObject = Record<string, unknown>

export type AstNode = JsonObject & {
  type?: string
  start?: number
  end?: number
  range?: [number, number]
}

export type ReferenceMode = 'r' | 'rw' | 'w'

export interface Variable {
  id: AstNode
  kind: string
}

export interface Reference {
  id: AstNode
  mode: ReferenceMode
}
