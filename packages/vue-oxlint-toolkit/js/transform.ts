import type { ToolkitTransformResult } from '.'
import { nativeTransformJsx } from '../bindings'
import { withLoc } from './location'

export function transformJsx(source: string): ToolkitTransformResult {
  const result = nativeTransformJsx(source)

  return {
    sourceText: result.sourceText,
    scriptKind: result.scriptKind,
    comments: result.comments.map((comment) => withLoc(source, comment)),
    irregularWhitespaces: result.irregularWhitespaces,
    errors: result.errors.map((error) => withLoc(source, error)),
    mappings: result.mappings,
  }
}
