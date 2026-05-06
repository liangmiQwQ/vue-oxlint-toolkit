import type { AST } from 'vue-eslint-parser'

export type OffsetRange = [number, number]
export type SourceType = 'script' | 'commonjs' | 'module'
export type ReferenceMode = 'r' | 'rw' | 'w'
export type OxlintProgram = Omit<AST.ESLintProgram, 'errors'>

export interface SourceSpan {
  start?: number
  end?: number
  range?: OffsetRange
  loc?: AST.LocationRange
}

export type AstNode = SourceSpan &
  Record<string, unknown> & {
    type: string
  }

export interface AstToken extends AstNode {
  value?: unknown
}

export interface AstComment extends AstNode {
  value: string
}

export interface Reference {
  id: AstNode
  mode: ReferenceMode
  variable?: unknown
}

export interface Variable {
  id: AstNode
  kind: string
}

export interface PureScriptNode extends AstNode {
  type: 'VPureScript'
  body: AstNode[]
}

export interface VueElementNode extends AstNode {
  type: 'VElement'
  name: string
  children?: VueSfcChild[]
  comments?: AstComment[]
  tokens?: AstToken[]
  errors?: unknown[]
}

export type VueSfcChild = VueElementNode | PureScriptNode | AstNode

export interface VueSingleFileComponent extends AstNode {
  type: 'VueSingleFileComponent'
  children: VueSfcChild[]
  script_comments: AstComment[]
  template_comments: AstComment[]
  scriptTokens: AstToken[]
  templateTokens: AstToken[]
  source_type?: SourceType
  template_errors?: unknown[]
}

export interface DocumentFragmentNode extends AstNode {
  type: 'VDocumentFragment'
  children: VueSfcChild[]
  comments: AstComment[]
  errors: unknown[]
  parent: null
  tokens: AstToken[]
}

export function getArray<T = unknown>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : []
}

export function getNodeArray(value: unknown): AstNode[] {
  return getArray(value).filter(isAstNode)
}

export function getRange(node: AstNode): OffsetRange | undefined {
  if (
    Array.isArray(node.range) &&
    typeof node.range[0] === 'number' &&
    typeof node.range[1] === 'number'
  ) {
    return node.range
  }

  const start = getStart(node)
  const end = getEnd(node)
  return typeof start === 'number' && typeof end === 'number' ? [start, end] : undefined
}

export function getStart(node: AstNode) {
  if (typeof node.start === 'number') {
    return node.start
  }

  return Array.isArray(node.range) && typeof node.range[0] === 'number' ? node.range[0] : undefined
}

export function getEnd(node: AstNode) {
  if (typeof node.end === 'number') {
    return node.end
  }

  return Array.isArray(node.range) && typeof node.range[1] === 'number' ? node.range[1] : undefined
}

export function isAstNode(value: unknown): value is AstNode {
  return isObject(value) && typeof value.type === 'string'
}

export function isPureScriptNode(node: AstNode): node is PureScriptNode {
  return node.type === 'VPureScript' && Array.isArray(node.body)
}

export function isScriptElement(node: AstNode): node is VueElementNode {
  return node.type === 'VElement' && node.name === 'script'
}

export function isTemplateElement(node: AstNode): node is VueElementNode {
  return node.type === 'VElement' && node.name === 'template'
}

export function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object'
}
