import { AST } from 'vue-eslint-parser'
import type { NativeRange } from '../bindings'
import { parseVue as nativeParseVue } from '../bindings'
import type { Diagnostic } from '@oxlint/plugins'
import type { OxlintProgram } from './ast'
import type { ToolkitTransformResult } from './transform-jsx'

export interface SourceLocation {
  line: number
  column: number
}

export interface SourceOffsetMap {
  lineStarts: readonly SourceOffset[]
  byteToIndex: ReadonlyMap<number, number>
}

export interface ParseResult {
  ast: OxlintProgram
  errors: Diagnostic[]
  panicked: boolean
  transform: ToolkitTransformResult | null
}

export function parse(_path: string, source: string, _options: object = {}): ParseResult {
  const result = nativeParseVue(source)
  const offsetMap = createSourceOffsetMap(source)
  const sfc: VueSingleFileComponent = JSON.parse(result.astJson)
  const program = toProgram(sfc, offsetMap)

  return {
    ast: program,
    errors: result.errors.map((error) => ({
      message: error.message,
      loc: toLocation(offsetMap, error),
    })),
    panicked: result.panicked,
    transform: null,
  }
}

export function createSourceOffsetMap(source: string): SourceOffsetMap {
  const lineStarts: SourceOffset[] = [{ byte: 0, index: 0 }]
  const byteToIndex = new Map<number, number>([[0, 0]])
  let byteOffset = 0

  for (let index = 0; index < source.length; ) {
    const codePoint = source.codePointAt(index)
    if (codePoint === undefined) {
      break
    }

    const codeUnitLength = codePoint > 0xffff ? 2 : 1
    byteOffset += utf8ByteLength(codePoint)
    index += codeUnitLength
    byteToIndex.set(byteOffset, index)

    if (codePoint === 10) {
      lineStarts.push({ byte: byteOffset, index })
    }
  }

  return { lineStarts, byteToIndex }
}

export function toIndex(offsetMap: SourceOffsetMap, offset: number) {
  const index = offsetMap.byteToIndex.get(offset)

  if (index === undefined) {
    throw new RangeError(`Offset ${offset} is not on a UTF-8 character boundary.`)
  }

  return index
}

export function toRange(offsetMap: SourceOffsetMap, range: NativeRange): [number, number] {
  return [toIndex(offsetMap, range.start), toIndex(offsetMap, range.end)]
}

export function toLocation(offsetMap: SourceOffsetMap, range: NativeRange) {
  return {
    start: offsetToLocation(offsetMap, range.start),
    end: offsetToLocation(offsetMap, range.end),
  }
}

function toProgram(sfc: VueSingleFileComponent, offsetMap: SourceOffsetMap): OxlintProgram {
  const children = sfc.children
  const body = collectProgramBody(children)
  const templateBody = findTemplateBody(children)
  const templateComments = sfc.template_comments
  const templateTokens = sfc.templateTokens
  const fragment = createDocumentFragment(sfc, children, templateComments, templateTokens)
  const programRange = getProgramRange(body, sfc.scriptTokens as AstNode[])
  const program: AstNode = {
    type: 'Program',
    sourceType: sfc.source_type ?? 'module',
    body,
    comments: sfc.script_comments,
    tokens: sfc.scriptTokens,
    templateBody: templateBody ?? undefined,
    start: programRange[0],
    end: programRange[1],
    range: programRange,
  }

  if (templateBody) {
    templateBody.comments = templateComments
    templateBody.tokens = templateTokens
    templateBody.errors = getArray(templateBody.errors)
  }

  attachAstMetadata(program, offsetMap)
  attachAstMetadata(fragment, offsetMap, null)
  attachListMetadata(getArray(program.comments), offsetMap)
  attachListMetadata(getArray(program.tokens), offsetMap)
  attachListMetadata(templateComments, offsetMap)
  attachListMetadata(templateTokens, offsetMap)

  return program as OxlintProgram
}

function collectProgramBody(children: AstNode[]): AstNode[] {
  const body: AstNode[] = []

  for (const child of children) {
    if (isPureScriptNode(child)) {
      body.push(...child.body)
      continue
    }

    if (isScriptElement(child)) {
      for (const scriptChild of getNodeArray(child.children)) {
        if (isPureScriptNode(scriptChild)) {
          body.push(...scriptChild.body)
        }
      }
    }
  }

  return body
}

function findTemplateBody(children: AstNode[]): VueElementNode | null {
  return children.find(isTemplateElement) ?? null
}

function createDocumentFragment(
  sfc: VueSingleFileComponent,
  children: AstNode[],
  comments: VueSingleFileComponent['template_comments'],
  tokens: VueSingleFileComponent['templateTokens'],
) {
  return {
    type: 'VDocumentFragment',
    children,
    comments,
    errors: getArray(sfc.template_errors),
    parent: null,
    range: getRange(sfc) ?? [0, 0],
    start: sfc.start,
    end: sfc.end,
    tokens,
  }
}

function getProgramRange(body: AstNode[], scriptTokens: AstNode[]): OffsetRange {
  const firstBody = body[0]
  const lastBody = body.at(-1)
  const bodyStart = firstBody ? getStart(firstBody) : undefined
  const bodyEnd = lastBody ? getEnd(lastBody) : undefined

  if (typeof bodyStart === 'number' && typeof bodyEnd === 'number') {
    return [bodyStart, bodyEnd]
  }

  const firstToken = scriptTokens[0]
  const lastToken = scriptTokens.at(-1)
  const tokenStart = firstToken ? getEnd(firstToken) : undefined
  const tokenEnd = lastToken ? getStart(lastToken) : undefined

  if (typeof tokenStart === 'number' && typeof tokenEnd === 'number') {
    return [tokenStart, tokenEnd]
  }

  return [0, 0]
}

const visitorKeys = new Proxy(
  {
    ...AST.KEYS,
    VPureScript: [],
  } as Record<string, readonly string[]>,
  {
    get(keys, type) {
      if (typeof type !== 'string') {
        return Reflect.get(keys, type)
      }

      return keys[type] ?? []
    },
  },
)

function attachAstMetadata(root: AstNode, offsetMap: SourceOffsetMap, rootParent?: AstNode | null) {
  const hasRootParent = rootParent !== undefined

  AST.traverseNodes(root as AST.Node, {
    visitorKeys,
    enterNode(node, parent) {
      const astNode = node as AstNode
      const actualParent =
        astNode === root && hasRootParent ? rootParent : (parent as AstNode | null)
      attachNodeMetadata(astNode, offsetMap, actualParent, astNode !== root || hasRootParent)
      attachReferenceLikeMetadata(astNode, offsetMap)
    },
    leaveNode() {},
  })
}

function attachListMetadata(values: unknown[], offsetMap: SourceOffsetMap) {
  for (const value of values) {
    if (isAstNode(value)) {
      attachLocationMetadata(value, offsetMap)
    }
  }
}

function attachReferenceLikeMetadata(node: AstNode, offsetMap: SourceOffsetMap) {
  for (const reference of getArray<ReferenceLike>(node.references)) {
    attachDetachedIdMetadata(reference.id, node, offsetMap)
  }
  for (const variable of getArray<ReferenceLike>(node.variables)) {
    attachDetachedIdMetadata(variable.id, node, offsetMap)
  }
}

function attachDetachedIdMetadata(id: unknown, owner: AstNode, offsetMap: SourceOffsetMap) {
  if (!isAstNode(id)) {
    return
  }

  if (!('parent' in id)) {
    attachParentMetadata(id, owner)
  }
  attachLocationMetadata(id, offsetMap)
}

function attachNodeMetadata(
  node: AstNode,
  offsetMap: SourceOffsetMap,
  parent: AstNode | null | undefined,
  shouldAttachParent: boolean,
) {
  if (shouldAttachParent) {
    attachParentMetadata(node, parent)
  }

  attachLocationMetadata(node, offsetMap)
}

function attachParentMetadata(node: AstNode, parent: AstNode | null | undefined) {
  Object.defineProperty(node, 'parent', {
    configurable: true,
    enumerable: true,
    value: parent,
    writable: true,
  })
}

function attachLocationMetadata(value: AstNode, offsetMap: SourceOffsetMap) {
  const range = getRange(value)
  if (!range) {
    return
  }

  value.range ??= range
  const [start, end] = value.range

  Object.defineProperty(value, 'loc', {
    configurable: true,
    enumerable: true,
    get() {
      return {
        start: offsetToLocation(offsetMap, start),
        end: offsetToLocation(offsetMap, end),
      }
    },
  })
}

function offsetToLocation(offsetMap: SourceOffsetMap, offset: number): SourceLocation {
  const lineIndex = findLineIndex(offsetMap.lineStarts, offset)
  const index = toIndex(offsetMap, offset)

  return {
    line: lineIndex + 1,
    column: index - offsetMap.lineStarts[lineIndex].index,
  }
}

function findLineIndex(lineStarts: readonly SourceOffset[], offset: number) {
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

  return Math.max(0, high)
}

function getArray<T = unknown>(value: unknown): T[] {
  return Array.isArray(value) ? (value as T[]) : []
}

function getNodeArray(value: unknown): AstNode[] {
  return getArray(value).filter(isAstNode)
}

function getRange(node: AstNode): OffsetRange | undefined {
  if (
    Array.isArray(node.range) &&
    typeof node.range[0] === 'number' &&
    typeof node.range[1] === 'number'
  ) {
    return node.range
  }

  const start = getStart(node)
  const end = getEnd(node)
  return typeof start === 'number' && typeof end === 'number' ? [start, end] : undefined
}

function getStart(node: AstNode) {
  if (typeof node.start === 'number') {
    return node.start
  }

  return Array.isArray(node.range) && typeof node.range[0] === 'number' ? node.range[0] : undefined
}

function getEnd(node: AstNode) {
  if (typeof node.end === 'number') {
    return node.end
  }

  return Array.isArray(node.range) && typeof node.range[1] === 'number' ? node.range[1] : undefined
}

function isAstNode(value: unknown): value is AstNode {
  return isObject(value) && typeof value.type === 'string'
}

function isPureScriptNode(node: AstNode): node is VPureScript {
  return node.type === 'VPureScript' && Array.isArray(node.body)
}

function isScriptElement(node: AstNode): node is VueElementNode {
  return node.type === 'VElement' && node.name === 'script'
}

function isTemplateElement(node: AstNode): node is VueElementNode {
  return node.type === 'VElement' && node.name === 'template'
}

function isObject(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object'
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

interface SourceOffset {
  byte: number
  index: number
}

type OffsetRange = AST.OffsetRange

type AstNode = Record<string, unknown> & {
  type: string
  start?: number
  end?: number
  range?: OffsetRange
  loc?: AST.LocationRange
}

type ReferenceLike = {
  id: AstNode
}

type VPureScript = AstNode & {
  type: 'VPureScript'
  body: AstNode[]
}

type VueElementNode = AstNode & {
  type: 'VElement'
  name: string
  children?: AstNode[]
  comments?: AstNode[]
  tokens?: AstNode[]
  errors?: unknown[]
}

type VueSingleFileComponent = AstNode & {
  type: 'VueSingleFileComponent'
  children: AstNode[]
  script_comments: AstNode[]
  template_comments: AstNode[]
  scriptTokens: AstNode[]
  templateTokens: AstNode[]
  source_type?: 'script' | 'commonjs' | 'module'
  template_errors?: unknown[]
}
