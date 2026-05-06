import type { Diagnostic } from '@oxlint/plugins'
import type { OxlintProgram } from './ast/nodes'
import type { ToolkitTransformResult } from './transform-result'

export interface ParseResult {
  ast: OxlintProgram
  errors: Diagnostic[]
  panicked: boolean
  transform: ToolkitTransformResult | null
}
