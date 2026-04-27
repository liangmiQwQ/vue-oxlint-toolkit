/**
 * TypeScript types mirroring the Rust V* AST in `vue_oxlint_parser::ast`.
 *
 * These describe the shape of the JSON returned by `parseSync`. The script
 * Program is delivered as opaque ESTree-compatible JSON (see oxc_estree); we
 * type it as `unknown` here to avoid coupling to a specific oxc release.
 */

export interface Span {
  start: number
  end: number
}

export type VNamespace = 'html' | 'svg' | 'mathml'

export interface VDocumentFragment {
  type: 'VDocumentFragment'
  range: Span
  children: VRootChild[]
}

export type VRootChild = VElement | VText

export interface VElement {
  type: 'VElement'
  range: Span
  name: string
  raw_name: string
  namespace: VNamespace
  start_tag: VStartTag
  end_tag: VEndTag | null
  children: VElementChild[]
}

export interface VStartTag {
  type: 'VStartTag'
  range: Span
  self_closing: boolean
  attributes: VAttribute[]
}

export interface VEndTag {
  type: 'VEndTag'
  range: Span
}

export type VElementChild =
  | { VElement: VElement }
  | { VText: VText }
  | { VExpressionContainer: VExpressionContainer }

export interface VText {
  type: 'VText'
  range: Span
  value: string
}

export interface VExpressionContainer {
  type: 'VExpressionContainer'
  range: Span
  raw_expression: string
  expression_range: Span
  raw: boolean
}

export interface VAttribute {
  type: 'VAttribute'
  range: Span
  directive: boolean
  key: VAttributeKey
  value: VAttributeValue | null
}

export type VAttributeKey = VIdentifier | VDirectiveKey

export interface VIdentifier {
  type: 'VIdentifier'
  range: Span
  name: string
  raw_name: string
}

export interface VDirectiveKey {
  type: 'VDirectiveKey'
  range: Span
  name: string
  argument: string | null
  modifiers: string[]
  raw: string
}

export type VAttributeValue = VLiteral | VExpressionContainer

export interface VLiteral {
  type: 'VLiteral'
  range: Span
  value: string
}

export interface ParsedScript {
  tag: string
  setup: boolean
  lang: string | null
  content_range: Span
  errors: string[]
  /** ESTree-compatible JSON produced by oxc_estree. */
  program: unknown
}
