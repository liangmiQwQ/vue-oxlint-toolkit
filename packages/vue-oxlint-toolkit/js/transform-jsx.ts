import type { NativeMapping, NativeTransformResult } from '../bindings'
import { transformJsx as nativeTransformJsx } from '../bindings'
import { createLocator, toLocation, toRange } from './locator'
import type { Locator, Mapping, ToolkitTransformResult } from './types'

export function transformJsx(source: string): ToolkitTransformResult {
  const result: NativeTransformResult = nativeTransformJsx(source)
  const locator = createLocator(source)
  const virtualLocator = createLocator(result.sourceText)

  return {
    sourceText: result.sourceText,
    scriptKind: result.scriptKind,
    comments: result.comments.map((comment) => ({
      type: comment.type,
      value: comment.value,
      start: locator.toIndex(comment.start),
      end: locator.toIndex(comment.end),
      range: toRange(comment, locator),
      loc: toLocation(comment, locator),
    })),
    irregularWhitespaces: result.irregularWhitespaces.map((range) => toRange(range, locator)),
    errors: result.errors.map((error) => ({
      message: error.message,
      loc: toLocation(error, locator),
    })),
    mappings: result.mappings.map((mapping) => toMapping(mapping, locator, virtualLocator)),
  }
}

function toMapping(mapping: NativeMapping, locator: Locator, virtualLocator: Locator): Mapping {
  return {
    virtualStart: virtualLocator.toIndex(mapping.virtualStart),
    virtualEnd: virtualLocator.toIndex(mapping.virtualEnd),
    originalStart: locator.toIndex(mapping.originalStart),
    originalEnd: locator.toIndex(mapping.originalEnd),
  }
}
