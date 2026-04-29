import type { Comment, Diagnostic, Range } from '@oxlint/plugins'

export interface ToolkitTransformResult {
  sourceText: string
  scriptKind: 'jsx' | 'tsx'
  comments: Comment[]
  irregularWhitespaces: Range[]
  errors: Diagnostic[]
  mapping: any
}

export function vue_jsx(source: string): ToolkitTransformResult {
  // PLACEHOLDER for NAPI
  return {
    sourceText: source,
    scriptKind: 'jsx',
    comments: [],
    irregularWhitespaces: [],
    errors: [],
    mapping: null,
  }
}
