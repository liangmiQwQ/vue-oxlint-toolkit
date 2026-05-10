import type { ToolkitTransformResult } from '.'
import { nativeTransformJsx } from '../bindings'
import { getConvertor, type LocationConvertor } from './location'

export function transformJsx(
  source: string,
  sourceConvertor: LocationConvertor = getConvertor(source),
): ToolkitTransformResult {
  const result = nativeTransformJsx(source)
  const generatedConvertor = getConvertor(result.sourceText)

  return {
    sourceText: result.sourceText,
    scriptKind: result.scriptKind,
    comments: result.comments.map(sourceConvertor.fix),
    irregularWhitespaces: result.irregularWhitespaces.map(sourceConvertor.range),
    errors: result.errors.map(sourceConvertor.fix),
    mappings: result.mappings.map((mapping) => {
      const virtual = generatedConvertor.toUtf16({
        start: mapping.virtualStart,
        end: mapping.virtualEnd,
      })
      const original = sourceConvertor.toUtf16({
        start: mapping.originalStart,
        end: mapping.originalEnd,
      })

      return {
        virtualStart: virtual.start,
        virtualEnd: virtual.end,
        originalStart: original.start,
        originalEnd: original.end,
      }
    }),
  }
}
