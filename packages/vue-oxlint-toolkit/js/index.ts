import type { Comment, Diagnostic, Location, Range } from '@oxlint/plugins'
import type { NativeTransformResult } from '../bindings'
import { transformJsx as nativeTransformJsx } from '../bindings'
import { createLocationGetter, type LocationGetter } from './location'

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

export interface ParseResult {
  // ast: AST.ESLintProgram (the import of AST brings a lot of unnecessary types definition in dts, remove it temporarily)
  ast: any
  errors: Diagnostic[]
  panicked: boolean
  transform: ToolkitTransformResult
}

export declare function parse(path: string, source: string, options?: {}): ParseResult

export function transformJsx(source: string): ToolkitTransformResult {
  const result = nativeTransformJsx(source) as NativeTransformResult
  const sourceLocations = createLocationGetter(source)

  return {
    sourceText: result.sourceText,
    scriptKind: result.scriptKind,
    comments: result.comments.map((comment) => createComment(comment, sourceLocations)),
    irregularWhitespaces: result.irregularWhitespaces,
    errors: result.errors.map((error) => ({
      message: error.message,
      loc: sourceLocations.loc(error.start, error.end),
    })),
    mappings: result.mappings,
  }
}

type NativeComment = NativeTransformResult['comments'][number]

function createComment(comment: NativeComment, locations: LocationGetter): Comment {
  let loc: Location | undefined

  return {
    type: comment.type,
    value: comment.value,
    start: comment.start,
    end: comment.end,
    range: comment.range,
    get loc() {
      return (loc ??= locations.loc(comment.start, comment.end))
    },
  }
}
