// Copied from vue-eslint-parser 10.4.0 V* AST node definitions.

import type { ESTree, Ranged, Span } from '@oxlint/plugins'

type RawSpan = { start: number; end: number } | Ranged
type RawValue<T> = T extends ParseError
  ? T
  : T extends Span
    ? RawNode<T>
    : T extends (infer U)[]
      ? RawValue<U>[]
      : T

export type RawNode<T> = T extends unknown
  ? RawSpan & {
      [K in keyof Omit<T, 'parent' | 'loc' | 'range' | 'start' | 'end'>]: RawValue<
        Omit<T, 'parent' | 'loc' | 'range' | 'start' | 'end'>[K]
      >
    }
  : never

export type Node =
  | VNode
  | VForExpression
  | VOnExpression
  | VSlotScopeExpression
  | VGenericExpression
  | VFilterSequenceExpression
  | VFilter

export interface Token extends Span {
  type: string
  value: string
}

export interface ParseError extends SyntaxError {
  code?: string
  index: number
  lineNumber: number
  column: number
}

export const NS = Object.freeze({
  HTML: 'http://www.w3.org/1999/xhtml',
  MathML: 'http://www.w3.org/1998/Math/MathML',
  SVG: 'http://www.w3.org/2000/svg',
  XLink: 'http://www.w3.org/1999/xlink',
  XML: 'http://www.w3.org/XML/1998/namespace',
  XMLNS: 'http://www.w3.org/2000/xmlns/',
} as const)

export type Namespace =
  | typeof NS.HTML
  | typeof NS.MathML
  | typeof NS.SVG
  | typeof NS.XLink
  | typeof NS.XML
  | typeof NS.XMLNS

export interface Variable {
  id: ESTree.BindingIdentifier
  kind: 'v-for' | 'scope' | 'generic'
  references: Reference[]
}

export interface Reference {
  id: ESTree.IdentifierReference
  mode: 'rw' | 'r' | 'w'
  variable: Variable | null
  isValueReference?: boolean
  isTypeReference?: boolean
}

export interface VForExpression extends Span {
  type: 'VForExpression'
  parent: VExpressionContainer
  left: ESTree.BindingPattern[]
  right: ESTree.Expression
}

export interface VOnExpression extends Span {
  type: 'VOnExpression'
  parent: VExpressionContainer
  body: ESTree.Statement[]
}

export interface VSlotScopeExpression extends Span {
  type: 'VSlotScopeExpression'
  parent: VExpressionContainer
  params: ESTree.BindingPattern[]
}

export interface VGenericExpression extends Span {
  type: 'VGenericExpression'
  parent: VExpressionContainer
  params: ESTree.TSTypeParameter[]
  rawParams: string[]
}

export interface VFilterSequenceExpression extends Span {
  type: 'VFilterSequenceExpression'
  parent: VExpressionContainer
  expression: ESTree.Expression
  filters: VFilter[]
}

export interface VFilter extends Span {
  type: 'VFilter'
  parent: VFilterSequenceExpression
  callee: ESTree.IdentifierReference
  arguments: ESTree.Argument[]
}

export type VNode =
  | VAttribute
  | VDirective
  | VDirectiveKey
  | VDocumentFragment
  | VElement
  | VEndTag
  | VExpressionContainer
  | VIdentifier
  | VLiteral
  | VStartTag
  | VText

export interface VText extends Span {
  type: 'VText'
  parent: VDocumentFragment | VElement
  value: string
}

export interface VExpressionContainer extends Span {
  type: 'VExpressionContainer'
  parent: VDocumentFragment | VElement | VDirective | VDirectiveKey
  expression:
    | ESTree.Expression
    | VFilterSequenceExpression
    | VForExpression
    | VOnExpression
    | VSlotScopeExpression
    | VGenericExpression
    | null
  references: Reference[]
}

export interface VIdentifier extends Span {
  type: 'VIdentifier'
  parent: VAttribute | VDirectiveKey
  name: string
  rawName: string
}

export interface VDirectiveKey extends Span {
  type: 'VDirectiveKey'
  parent: VDirective
  name: VIdentifier
  argument: VExpressionContainer | VIdentifier | null
  modifiers: VIdentifier[]
}

export interface VLiteral extends Span {
  type: 'VLiteral'
  parent: VAttribute
  value: string
}

export interface VAttribute extends Span {
  type: 'VAttribute'
  parent: VStartTag
  directive: false
  key: VIdentifier
  value: VLiteral | null
}

export interface VDirective extends Span {
  type: 'VAttribute'
  parent: VStartTag
  directive: true
  key: VDirectiveKey
  value: VExpressionContainer | null
}

export interface VStartTag extends Span {
  type: 'VStartTag'
  parent: VElement
  selfClosing: boolean
  attributes: (VAttribute | VDirective)[]
}

export interface VEndTag extends Span {
  type: 'VEndTag'
  parent: VElement
}

export interface VElement extends Span {
  type: 'VElement'
  parent: VDocumentFragment | VElement
  namespace: Namespace
  name: string
  rawName: string
  startTag: VStartTag
  children: (VElement | VText | VExpressionContainer)[]
  endTag: VEndTag | null
  variables: Variable[]
}

export interface VDocumentFragment extends Span {
  type: 'VDocumentFragment'
  parent: null
  children: (VElement | VText | VExpressionContainer | VStyleElement)[]
  tokens: Token[]
  comments: Token[]
  errors: ParseError[]
}

export interface VStyleElement extends VElement {
  type: 'VElement'
  name: 'style'
  style: true
  children: (VText | VExpressionContainer)[]
}

export type RawVForExpression = RawNode<VForExpression>
export type RawVOnExpression = RawNode<VOnExpression>
export type RawVSlotScopeExpression = RawNode<VSlotScopeExpression>
export type RawVGenericExpression = RawNode<VGenericExpression>
export type RawVFilterSequenceExpression = RawNode<VFilterSequenceExpression>
export type RawVFilter = RawNode<VFilter>
export type RawVNode = RawNode<VNode>
export type RawVText = RawNode<VText>
export type RawVExpressionContainer = RawNode<VExpressionContainer>
export type RawVIdentifier = RawNode<VIdentifier>
export type RawVDirectiveKey = RawNode<VDirectiveKey>
export type RawVLiteral = RawNode<VLiteral>
export type RawVAttribute = RawNode<VAttribute>
export type RawVDirective = RawNode<VDirective>
export type RawVStartTag = RawNode<VStartTag>
export type RawVEndTag = RawNode<VEndTag>
export type RawVElement = RawNode<VElement>
export type RawVDocumentFragment = RawNode<VDocumentFragment>
export type RawVStyleElement = RawNode<VStyleElement>
