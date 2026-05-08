import type { NativeRange } from '../bindings'
import { parseVue } from '../bindings'
import type { OxlintProgram, VueSingleFileComponent } from './ast'
import type { ToolkitTransformResult } from './transform-jsx'

import type { Diagnostic } from '@oxlint/plugins'
import type { AST } from 'vue-eslint-parser'

export interface ParseResult {
  ast: OxlintProgram
  errors: Diagnostic[]
  panicked: boolean
  transform?: ToolkitTransformResult | null
}

export interface SourceOffsetMap {
  lineStarts: OffsetPoint[]
  byteToIndex: Map<number, number>
}

interface OffsetPoint {
  byte: number
  index: number
}

export interface SourcePosition {
  line: number
  column: number
}

export interface SourceLocation {
  start: SourcePosition
  end: SourcePosition
}

type ParseDiagnostic = NativeRange & { loc?: SourceLocation; message: string }
type TemplateBody = AST.VElement & Partial<AST.HasConcreteInfo>
type NodeValue = Record<string, unknown>

export function parse(_path: string, source: string, _options: object = {}): ParseResult {
  const result = parseVue(source)
  const offsetMap = createSourceOffsetMap(source)
  const sfc = JSON.parse(result.astJson) as VueSingleFileComponent
  const errors = result.errors.map(normalizeError)

  return {
    ast: buildProgram(sfc, errors, offsetMap),
    errors: errors as unknown as Diagnostic[],
    panicked: result.panicked,
    transform: null,
  }
}

export function createSourceOffsetMap(source: string): SourceOffsetMap {
  const lineStarts: OffsetPoint[] = [{ byte: 0, index: 0 }]
  const byteToIndex = new Map<number, number>([[0, 0]])
  let byte = 0

  for (let index = 0; index < source.length; ) {
    const codePoint = source.codePointAt(index)
    if (codePoint === undefined) {
      break
    }

    const width = codePoint > 0xffff ? 2 : 1
    byte += utf8ByteLength(codePoint)
    index += width
    byteToIndex.set(byte, index)

    if (codePoint === 10) {
      lineStarts.push({ byte, index })
    }
  }

  return { lineStarts, byteToIndex }
}

export function toIndex(offsetMap: SourceOffsetMap, offset: number): number {
  const index = offsetMap.byteToIndex.get(offset)
  if (index === undefined) {
    throw new RangeError(`Offset ${offset} is not on a UTF-8 character boundary.`)
  }

  return index
}

export function toRange(offsetMap: SourceOffsetMap, range: NativeRange): AST.OffsetRange {
  return [toIndex(offsetMap, range.start), toIndex(offsetMap, range.end)]
}

export function toLocation(offsetMap: SourceOffsetMap, range: NativeRange): SourceLocation {
  return {
    start: offsetToLocation(offsetMap, range.start),
    end: offsetToLocation(offsetMap, range.end),
  }
}

function buildProgram(
  sfc: VueSingleFileComponent,
  parseErrors: ParseDiagnostic[],
  offsetMap: SourceOffsetMap,
): OxlintProgram {
  const templateBody = sfc.children.find(isTemplateElement)
  const program = {
    type: 'Program',
    sourceType: sfc.source_type ?? 'module',
    body: sfc.body,
    comments: sfc.script_comments,
    tokens: sfc.scriptTokens,
    templateBody,
    parent: null,
    start: sfc.scriptRange[0],
    end: sfc.scriptRange[1],
    range: sfc.scriptRange,
  } as unknown as OxlintProgram

  if (templateBody) {
    templateBody.comments = sfc.template_comments as unknown as AST.Token[]
    templateBody.tokens = sfc.templateTokens as unknown as AST.Token[]
    templateBody.errors = (sfc.template_errors ?? []) as unknown as AST.ParseError[]
  }

  const fragment = createDocumentFragment(sfc, parseErrors)
  annotateAst(fragment, offsetMap)
  annotateAst(program, offsetMap, new Set(['templateBody']))
  annotateList(parseErrors, offsetMap)

  return program
}

function createDocumentFragment(
  sfc: VueSingleFileComponent,
  errors: ParseDiagnostic[],
): AST.VDocumentFragment {
  return {
    type: 'VDocumentFragment',
    children: sfc.children,
    comments: sfc.template_comments,
    errors,
    tokens: sfc.templateTokens,
    parent: null,
    range: sfc.range,
  } as unknown as AST.VDocumentFragment
}

function normalizeError(error: NativeRange & { message: string }): ParseDiagnostic {
  return {
    message: error.message,
    start: error.start,
    end: error.end,
  }
}

function annotateAst(root: object, offsetMap: SourceOffsetMap, skipKeys = new Set<string>()): void {
  const seen = new WeakSet<object>()
  const visit = (value: unknown, parent: object | null): void => {
    if (!isObject(value) || seen.has(value)) {
      return
    }

    seen.add(value)
    if (Array.isArray(value)) {
      for (const child of value) {
        visit(child, parent)
      }
      return
    }

    if (value !== root && shouldAnnotate(value)) {
      Object.defineProperty(value, 'parent', {
        configurable: true,
        enumerable: true,
        value: parent,
        writable: true,
      })
    }
    attachLocation(value, offsetMap)

    for (const [key, child] of Object.entries(value)) {
      if (skipKeys.has(key)) {
        continue
      }
      if (key === 'tokens' || key === 'comments' || key === 'errors') {
        if (Array.isArray(child)) {
          annotateList(child, offsetMap)
        }
        continue
      }
      if (key !== 'parent' && key !== 'loc') {
        visit(child, value)
      }
    }
  }

  visit(root, null)
}

function annotateList(values: unknown[], offsetMap: SourceOffsetMap): void {
  for (const value of values) {
    if (!isObject(value)) {
      continue
    }

    attachLocation(value, offsetMap)
  }
}

function attachLocation(value: object, offsetMap: SourceOffsetMap): void {
  const node = value as NodeValue
  const byteRange = byteOffsetRange(node)
  if (!byteRange) {
    return
  }

  Object.defineProperty(node, 'loc', {
    configurable: true,
    enumerable: true,
    get() {
      return {
        start: offsetToLocation(offsetMap, byteRange[0]),
        end: offsetToLocation(offsetMap, byteRange[1]),
      }
    },
  })
}

function byteOffsetRange(value: object): AST.OffsetRange | null {
  const node = value as NodeValue
  if (typeof node.start === 'number' && typeof node.end === 'number') {
    return [node.start, node.end]
  }

  if (
    Array.isArray(node.range) &&
    typeof node.range[0] === 'number' &&
    typeof node.range[1] === 'number'
  ) {
    return [node.range[0], node.range[1]]
  }

  return null
}

function offsetToLocation(offsetMap: SourceOffsetMap, offset: number): SourcePosition {
  const lineIndex = findLineIndex(offsetMap.lineStarts, offset)
  return {
    line: lineIndex + 1,
    column: toIndex(offsetMap, offset) - offsetMap.lineStarts[lineIndex].index,
  }
}

function findLineIndex(lineStarts: OffsetPoint[], offset: number): number {
  let low = 0
  let high = lineStarts.length - 1

  while (low <= high) {
    const middle = (low + high) >> 1
    if (lineStarts[middle].byte <= offset) {
      low = middle + 1
    } else {
      high = middle - 1
    }
  }

  return Math.max(0, high)
}

function isTemplateElement(node: VueSingleFileComponent['children'][number]): node is TemplateBody {
  return node.type === 'VElement' && node.name === 'template'
}

function shouldAnnotate(value: NodeValue): boolean {
  return typeof value.type === 'string' || typeof value.message === 'string'
}

function isObject(value: unknown): value is NodeValue {
  return value !== null && typeof value === 'object'
}

function utf8ByteLength(codePoint: number): number {
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
