import type { Diagnostic, Token } from '@oxlint/plugins'
import type { AST } from 'vue-eslint-parser'

export type OxlintProgram = Omit<AST.ESLintProgram, 'errors'>

export interface VueSingleFileComponent {
  type: 'VueSingleFileComponent'
  children: (AST.VElement | AST.VText | AST.VExpressionContainer | VScriptElement)[]
  body: OxlintProgram['body']
  script_comments: Token[]
  template_comments: Token[]
  scriptTokens: Token[]
  templateTokens: Token[]
  scriptRange: AST.OffsetRange
  source_type?: 'script' | 'module'
  template_errors?: Diagnostic[]
  range: AST.OffsetRange
}

export interface VScriptElement extends Omit<AST.VElement, 'parent' | 'loc' | 'children'> {
  children: VPureScript[]
}

export interface VPureScript {
  type: 'VPureScript'
  start: number
  end: number
  range: AST.OffsetRange
}
