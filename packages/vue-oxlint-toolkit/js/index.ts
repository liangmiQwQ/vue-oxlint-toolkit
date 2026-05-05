import type { Comment, Diagnostic, Range } from '@oxlint/plugins'
import type { NativeMapping, NativeRange, NativeTransformResult } from '../bindings'
import { parseVue as nativeParseVue, transformJsx as nativeTransformJsx } from '../bindings'

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
  transform: ToolkitTransformResult | null
}

export function parse(_path: string, source: string, _options: {} = {}): ParseResult {
  const result = nativeParseVue(source)
  const locator = createLocator(source)
  const sfc = JSON.parse(result.astJson)
  const program = toProgram(sfc, locator)

  return {
    ast: program,
    errors: result.errors.map((error) => ({
      message: error.message,
      loc: toLocation(error, locator),
    })),
    panicked: result.panicked,
    transform: null,
  }
}

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

function toProgram(sfc: any, locator: ReturnType<typeof createLocator>) {
  const body: any[] = []
  const templateChildren: any[] = []

  for (const child of sfc.children ?? []) {
    if (child?.type === 'VPureScript') {
      body.push(...(child.body ?? []))
    } else if (child?.type === 'VElement' && child.name === 'script') {
      const scriptChildren = child.children ?? []
      for (const scriptChild of scriptChildren) {
        if (scriptChild?.type === 'VPureScript') {
          body.push(...(scriptChild.body ?? []))
        }
      }
      child.children = scriptChildren.filter(
        (scriptChild: any) => scriptChild?.type !== 'VPureScript',
      )
      templateChildren.push(child)
    } else {
      templateChildren.push(child)
    }
  }

  sfc.children = templateChildren
  sfc.comments = sfc.template_comments ?? []
  sfc.tokens = sfc.templateTokens ?? []
  normalizeVueAst(sfc)
  const templateBody =
    sfc.children.find((child: any) => child?.type === 'VElement' && child.name === 'template') ??
    null
  if (templateBody) {
    templateBody.comments = sfc.template_comments ?? []
    templateBody.tokens = sfc.templateTokens ?? []
  }
  const program = {
    type: 'Program',
    sourceType: sfc.source_type ?? 'module',
    body,
    comments: [...(sfc.script_comments ?? []), ...(sfc.template_comments ?? [])],
    tokens: sfc.scriptTokens ?? [],
    templateBody,
    start: sfc.start,
    end: sfc.end,
    range: [sfc.start, sfc.end],
  }

  attachMetadata(program, null, locator)
  return program
}

function normalizeVueAst(node: any) {
  if (!node || typeof node !== 'object') {
    return
  }

  if (node.type === 'VElement') {
    node.variables = node.variables?.length ? node.variables : collectElementVariables(node)
    if (node.startTag) {
      node.startTag.variables ??= node.variables
    }
  }

  if (node.type === 'VExpressionContainer') {
    if (!Array.isArray(node.references) || node.references.length === 0) {
      node.references = node.reference ?? collectExpressionReferences(node.expression)
    }
    delete node.reference
  }

  for (const child of Object.values(node)) {
    if (Array.isArray(child)) {
      for (const item of child) {
        normalizeVueAst(item)
      }
    } else {
      normalizeVueAst(child)
    }
  }
}

function collectElementVariables(element: any) {
  const variables: Array<{ id: any; kind: string }> = []
  const seen = new Set<string>()

  for (const attribute of element.startTag?.attributes ?? []) {
    const expression = attribute?.value?.expression
    if (expression?.type === 'VForExpression') {
      for (const id of collectPatternIdentifiers(expression.left)) {
        pushVariable(variables, seen, id, 'v-for')
      }
    }
    if (expression?.type === 'VSlotScopeExpression') {
      for (const id of collectPatternIdentifiers(expression.params)) {
        pushVariable(variables, seen, id, 'v-slot')
      }
    }
  }

  return variables
}

function pushVariable(
  variables: Array<{ id: any; kind: string }>,
  seen: Set<string>,
  id: any,
  kind: string,
) {
  const key = `${kind}:${id.start}:${id.end}:${id.name}`
  if (seen.has(key)) {
    return
  }

  seen.add(key)
  variables.push({ id, kind })
}

function collectExpressionReferences(expression: any) {
  const references: Array<{ id: any; mode: 'r' | 'rw' | 'w' }> = []
  const localStack: Array<Set<string>> = []

  visitExpression(expression, references, localStack)
  return references
}

function visitExpression(
  node: any,
  references: Array<{ id: any; mode: 'r' | 'rw' | 'w' }>,
  localStack: Array<Set<string>>,
) {
  if (!node || typeof node !== 'object') {
    return
  }

  if (node.type === 'Identifier' && !isLocalReference(node.name, localStack)) {
    references.push({ id: node, mode: 'r' })
    return
  }

  if (node.type === 'MemberExpression') {
    visitExpression(node.object, references, localStack)
    if (node.computed) {
      visitExpression(node.property, references, localStack)
    }
    return
  }

  if (node.type === 'Property') {
    if (node.computed) {
      visitExpression(node.key, references, localStack)
    }
    visitExpression(node.value, references, localStack)
    return
  }

  if (node.type === 'VariableDeclarator') {
    collectPatternNames(node.id, currentLocals(localStack))
    visitExpression(node.init, references, localStack)
    return
  }

  if (node.type === 'VForExpression') {
    const locals = new Set<string>()
    collectPatternNames(node.left, locals)
    localStack.push(locals)
    visitExpression(node.right, references, localStack)
    localStack.pop()
    return
  }

  if (node.type === 'VSlotScopeExpression') {
    return
  }

  if (node.type === 'ArrowFunctionExpression' || node.type === 'FunctionExpression') {
    const locals = new Set<string>()
    for (const param of node.params ?? []) {
      collectPatternNames(param, locals)
    }
    localStack.push(locals)
    visitExpression(node.body, references, localStack)
    localStack.pop()
    return
  }

  for (const [key, child] of Object.entries(node)) {
    if (key === 'parent' || key === 'loc' || key === 'id') {
      continue
    }

    if (Array.isArray(child)) {
      for (const item of child) {
        visitExpression(item, references, localStack)
      }
    } else {
      visitExpression(child, references, localStack)
    }
  }
}

function collectPatternNames(node: any, names: Set<string>) {
  if (!node || typeof node !== 'object') {
    return
  }

  if (node.type === 'Identifier') {
    names.add(node.name)
    return
  }

  for (const child of Object.values(node)) {
    if (Array.isArray(child)) {
      for (const item of child) {
        collectPatternNames(item, names)
      }
    } else {
      collectPatternNames(child, names)
    }
  }
}

function collectPatternIdentifiers(node: any) {
  const identifiers: any[] = []
  collectPatternIdentifierInto(node, identifiers)
  return identifiers
}

function collectPatternIdentifierInto(node: any, identifiers: any[]) {
  if (!node || typeof node !== 'object') {
    return
  }

  if (node.type === 'Identifier') {
    identifiers.push(node)
    return
  }

  for (const child of Object.values(node)) {
    if (Array.isArray(child)) {
      for (const item of child) {
        collectPatternIdentifierInto(item, identifiers)
      }
    } else {
      collectPatternIdentifierInto(child, identifiers)
    }
  }
}

function isLocalReference(name: string, localStack: Array<Set<string>>) {
  return localStack.some((locals) => locals.has(name))
}

function currentLocals(localStack: Array<Set<string>>) {
  if (localStack.length === 0) {
    localStack.push(new Set())
  }

  return localStack[localStack.length - 1]
}

function attachMetadata(value: any, parent: any, locator: ReturnType<typeof createLocator>) {
  if (!value || typeof value !== 'object') {
    return
  }

  if (parent) {
    Object.defineProperty(value, 'parent', {
      configurable: true,
      enumerable: false,
      value: parent,
      writable: true,
    })
  }

  if (typeof value.start === 'number' && typeof value.end === 'number') {
    value.range ??= [value.start, value.end]
    Object.defineProperty(value, 'loc', {
      configurable: true,
      enumerable: false,
      get() {
        return {
          start: locator(value.start),
          end: locator(value.end),
        }
      },
    })
  }

  for (const [key, child] of Object.entries(value)) {
    if (key === 'parent' || key === 'loc') {
      continue
    }

    if (Array.isArray(child)) {
      for (const item of child) {
        attachMetadata(item, value, locator)
      }
    } else {
      attachMetadata(child, value, locator)
    }
  }
}
