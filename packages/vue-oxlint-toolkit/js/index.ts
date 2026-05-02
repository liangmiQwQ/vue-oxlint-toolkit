import type { Comment, Diagnostic, Range } from '@oxlint/plugins'
import type { NativeMapping, NativeRange, NativeTransformResult } from '../bindings'
import { transformJsx as nativeTransformJsx } from '../bindings'

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
  transform: ToolkitTransformResult
}

export declare function parse(path: string, source: string, options?: {}): ParseResult

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

function toMapping(
  mapping: NativeMapping,
  locator: ReturnType<typeof createLocator>,
  virtualLocator: ReturnType<typeof createLocator>,
): Mapping {
  return {
    virtualStart: virtualLocator.toIndex(mapping.virtualStart),
    virtualEnd: virtualLocator.toIndex(mapping.virtualEnd),
    originalStart: locator.toIndex(mapping.originalStart),
    originalEnd: locator.toIndex(mapping.originalEnd),
  }
}

function toRange(range: NativeRange, locator: ReturnType<typeof createLocator>): Range {
  return [locator.toIndex(range.start), locator.toIndex(range.end)]
}

function toLocation(range: NativeRange, locator: ReturnType<typeof createLocator>) {
  return {
    start: locator(range.start),
    end: locator(range.end),
  }
}

function createLocator(source: string) {
  const lineStarts = [{ byte: 0, index: 0 }]
  const byteToIndex = new Map<number, number>([[0, 0]])
  let byteOffset = 0

  for (let index = 0; index < source.length; ) {
    const codePoint = source.codePointAt(index)!
    const codeUnitLength = codePoint > 0xffff ? 2 : 1

    byteOffset += utf8ByteLength(codePoint)
    index += codeUnitLength
    byteToIndex.set(byteOffset, index)

    if (codePoint === 10) {
      lineStarts.push({ byte: byteOffset, index })
    }
  }

  const toIndex = (offset: number) => {
    const index = byteToIndex.get(offset)

    if (index === undefined) {
      throw new RangeError(`Offset ${offset} is not on a UTF-8 character boundary.`)
    }

    return index
  }

  const locator = (offset: number) => {
    let low = 0
    let high = lineStarts.length - 1

    while (low <= high) {
      const mid = (low + high) >> 1

      if (lineStarts[mid].byte <= offset) {
        low = mid + 1
      } else {
        high = mid - 1
      }
    }

    const lineIndex = Math.max(0, high)
    const index = toIndex(offset)

    return {
      line: lineIndex + 1,
      column: index - lineStarts[lineIndex].index,
    }
  }

  locator.toIndex = toIndex

  return locator
}

function utf8ByteLength(codePoint: number) {
  if (codePoint <= 0x7f) {
    return 1
  }

  if (codePoint <= 0x7ff) {
    return 2
  }

  if (codePoint <= 0xffff) {
    return 3
  }

  return 4
}
