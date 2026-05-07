import type { NativeMapping, NativeTransformResult } from '../bindings'
import { transformJsx as nativeTransformJsx } from '../bindings'
import { createSourceOffsetMap, toIndex, toLocation, toRange } from './parse'
import type { SourceOffsetMap } from './parse'

import type { Comment, Diagnostic, Range } from '@oxlint/plugins'

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

export function transformJsx(source: string): ToolkitTransformResult {
  const result: NativeTransformResult = nativeTransformJsx(source)
  const offsetMap = createSourceOffsetMap(source)
  const virtualOffsetMap = createSourceOffsetMap(result.sourceText)

  return {
    sourceText: result.sourceText,
    scriptKind: result.scriptKind,
    comments: result.comments.map((comment) => ({
      type: comment.type,
      value: comment.value,
      start: toIndex(offsetMap, comment.start),
      end: toIndex(offsetMap, comment.end),
      range: toRange(offsetMap, comment),
      loc: toLocation(offsetMap, comment),
    })),
    irregularWhitespaces: result.irregularWhitespaces.map((range) => toRange(offsetMap, range)),
    errors: result.errors.map((error) => ({
      message: error.message,
      loc: toLocation(offsetMap, error),
    })),
    mappings: result.mappings.map((mapping) => toMapping(mapping, offsetMap, virtualOffsetMap)),
  }
}

function toMapping(
  mapping: NativeMapping,
  offsetMap: SourceOffsetMap,
  virtualOffsetMap: SourceOffsetMap,
): Mapping {
  return {
    virtualStart: toIndex(virtualOffsetMap, mapping.virtualStart),
    virtualEnd: toIndex(virtualOffsetMap, mapping.virtualEnd),
    originalStart: toIndex(offsetMap, mapping.originalStart),
    originalEnd: toIndex(offsetMap, mapping.originalEnd),
  }
}
