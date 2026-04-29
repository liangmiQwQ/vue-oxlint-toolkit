import type { Comment, Diagnostic, Range } from '@oxlint/plugins'
import type { NativeRange, NativeTransformResult } from '../bindings'
import { transformJsx as nativeTransformJsx } from '../bindings'
import { type AstNode, Codegen, type CodegenHook } from './codegen'

/**
 * AST-location-based mapping. There is one entry per AST node that has a
 * non-zero span (`start === 0 && end === 0` are synthesised nodes such as
 * the wrapping fragment, and are skipped). All offsets are JavaScript
 * string indices (UTF-16 code units), matching `range`/`loc` semantics.
 */
export interface Mapping {
  /** AST node type at this mapping point. */
  type: string
  /** Offset in the generated source text where this node starts. */
  virtualStart: number
  /** Offset in the generated source text where this node ends. */
  virtualEnd: number
  /** Offset in the original source where this node starts. */
  originalStart: number
  /** Offset in the original source where this node ends. */
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

export { Codegen } from './codegen'
export type { AstNode, CodegenHook, CodegenOptions, CodegenResult } from './codegen'

export function transformJsx(source: string): ToolkitTransformResult {
  const result: NativeTransformResult = nativeTransformJsx(source)
  const locator = createLocator(source)

  const ast = parseAst(result.estreeJson)
  const { code: sourceText, mappings } = runCodegen(ast, locator)

  return {
    sourceText,
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
    mappings,
  }
}

function parseAst(json: string): AstNode | null {
  if (!json || json === 'null') return null
  try {
    return JSON.parse(json) as AstNode
  } catch {
    return null
  }
}

function runCodegen(
  ast: AstNode | null,
  locator: ReturnType<typeof createLocator>,
): { code: string; mappings: Mapping[] } {
  if (!ast) return { code: '', mappings: [] }

  const inflight = new Map<AstNode, number>()
  const mappings: Mapping[] = []

  const hook: CodegenHook = {
    enter(node, virtualOffset) {
      inflight.set(node, virtualOffset)
    },
    leave(node, virtualOffset) {
      const virtualStart = inflight.get(node)
      inflight.delete(node)
      if (virtualStart === undefined) return

      const start = node.start
      const end = node.end
      // Skip synthesised nodes with no source span.
      if (typeof start !== 'number' || typeof end !== 'number') return
      if (start === 0 && end === 0) return

      mappings.push({
        type: node.type,
        virtualStart,
        virtualEnd: virtualOffset,
        originalStart: locator.toIndex(start),
        originalEnd: locator.toIndex(end),
      })
    },
  }

  const codegen = new Codegen({ hooks: [hook] })
  const { code } = codegen.build(ast)
  return { code, mappings }
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
