import type { Ranged, Span } from '@oxlint/plugins'
import type {
  ParseError,
  VForExpression,
  VOnExpression,
  VSlotScopeExpression,
  VGenericExpression,
  VNode,
  VText,
  VExpressionContainer,
  VIdentifier,
  VDirectiveKey,
  VLiteral,
  VAttribute,
  VDirective,
  VStartTag,
  VEndTag,
  VElement,
  VDocumentFragment,
  VStyleElement,
} from '.'

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

export type RawVForExpression = RawNode<VForExpression>
export type RawVOnExpression = RawNode<VOnExpression>
export type RawVSlotScopeExpression = RawNode<VSlotScopeExpression>
export type RawVGenericExpression = RawNode<VGenericExpression>
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
