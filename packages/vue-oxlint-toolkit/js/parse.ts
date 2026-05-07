import type { NativeRange } from '../bindings'
import { parseVue } from '../bindings'
import type { OxlintProgram, VPureScript, VScriptElement, VueSingleFileComponent } from './ast'
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
type TemplateError = ParseDiagnostic | SyntaxError
type TemplateBody = AST.VElement & Partial<AST.HasConcreteInfo>
type ScriptNode = (AST.VText | VPureScript) & { body?: OxlintProgram['body'] }
type ScriptElement = VScriptElement & {
  name: 'script'
  children: ScriptNode[]
}
type NodeValue = Record<string, unknown>
type LocationMode = 'js' | 'vue' | 'token'
const javascriptToken = Symbol('vue-oxlint-toolkit.javascriptToken')
const generatedIdentifier = Symbol('vue-oxlint-toolkit.generatedIdentifier')
const nonVoidSelfClosingError = 'non-void-html-element-start-tag-with-trailing-solidus'
const voidElementNames = new Set([
  'area',
  'base',
  'br',
  'col',
  'embed',
  'hr',
  'img',
  'input',
  'link',
  'meta',
  'param',
  'source',
  'track',
  'wbr',
])

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
  const scriptSelection = selectProgramScripts(sfc)
  const programRange = scriptRange(scriptSelection.scripts)
  const programStartEndRange = scriptStartEndRange(scriptSelection.scripts)
  const hasScript = scriptSelection.allScripts.length > 0
  const body = scriptBodies(scriptSelection.scripts)
  const program = {
    type: 'Program',
    sourceType: sfc.source_type ?? 'module',
    body,
    comments: sfc.script_comments,
    tokens: scriptSelection.includeTokens
      ? filterScriptTokens(sfc.scriptTokens, scriptSelection.scripts)
      : [],
    templateBody,
    start: programStartEndRange[0],
    end: programStartEndRange[1],
    range: programRange,
  } as unknown as OxlintProgram
  if (hasScript) {
    program.parent = null
  }
  removePureScriptChildren(sfc)
  fillImplicitBindValues(sfc)

  const templateErrors: TemplateError[] = templateBody
    ? createTemplateErrors(templateBody, sfc.templateTokens, offsetMap)
    : parseErrors
  if (templateBody) {
    templateBody.comments = sfc.template_comments as unknown as AST.Token[]
    templateBody.tokens = sfc.templateTokens as unknown as AST.Token[]
    templateBody.errors = templateErrors as unknown as AST.ParseError[]
  }

  const fragment = createDocumentFragment(sfc, templateErrors)
  annotateVueAst(fragment, offsetMap)
  annotateProgram(program, offsetMap)
  annotateList(parseErrors, offsetMap, 'vue')

  return program
}

function selectProgramScripts(sfc: VueSingleFileComponent): {
  allScripts: ScriptElement[]
  includeTokens: boolean
  scripts: ScriptElement[]
} {
  const allScripts = sfc.children.filter(isScriptElement)
  const scriptsWithBody = allScripts.filter(hasScriptBody)
  if (scriptsWithBody.length > 0) {
    return { allScripts, includeTokens: true, scripts: allScripts }
  }

  const setupScript = allScripts.find(isSetupScriptElement)
  if (setupScript && allScripts.length > 1) {
    return { allScripts, includeTokens: false, scripts: [setupScript] }
  }

  return { allScripts, includeTokens: true, scripts: allScripts.slice(0, 1) }
}

function createDocumentFragment(
  sfc: VueSingleFileComponent,
  errors: TemplateError[],
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

function scriptRange(scripts: ScriptElement[]): AST.OffsetRange {
  if (scripts.length === 1) {
    return scriptContentRange(scripts[0]) ?? [0, 0]
  }

  const bodyRanges = scripts
    .flatMap((script) => script.children)
    .flatMap(scriptBody)
    .map(byteOffsetRange)
    .filter((range): range is AST.OffsetRange => range !== null)
  const firstBodyRange = bodyRanges[0]
  const lastBodyRange = bodyRanges.at(-1)
  if (firstBodyRange && lastBodyRange) {
    const firstScriptContentRange = scriptContentRange(scripts[0])
    const start =
      isSetupScriptElement(scripts[0]) || !firstScriptContentRange
        ? firstBodyRange[0]
        : firstScriptContentRange[0]
    return [start, lastBodyRange[1]]
  }

  const textRanges = scripts
    .flatMap((script) => script.children)
    .map(byteOffsetRange)
    .filter((range): range is AST.OffsetRange => range !== null)
  const firstRange = textRanges[0]
  const lastRange = textRanges.at(-1)
  if (firstRange && lastRange) {
    return [firstRange[0], lastRange[1]]
  }

  const contentRange = scripts.map(scriptContentRange).find((range) => range !== null)
  if (contentRange) {
    return contentRange
  }

  return [0, 0]
}

function scriptStartEndRange(scripts: ScriptElement[]): AST.OffsetRange {
  const contentRanges = scripts
    .map(scriptContentRange)
    .filter((range): range is AST.OffsetRange => range !== null)
  const firstRange = contentRanges[0]
  const lastRange = contentRanges.at(-1)
  if (firstRange && lastRange) {
    return [firstRange[0], lastRange[1]]
  }

  return [0, 0]
}

function scriptContentRange(script: ScriptElement): AST.OffsetRange | null {
  const startTagRange = byteOffsetRange(script.startTag)
  if (!startTagRange) {
    return null
  }

  const endTag = script.endTag
  const endTagRange = isObject(endTag) ? byteOffsetRange(endTag) : null
  return [startTagRange[1], endTagRange?.[0] ?? startTagRange[1]]
}

function filterScriptTokens(tokens: unknown[], scripts: ScriptElement[]): unknown[] {
  return tokens.filter((token) => {
    if (!isObject(token)) {
      return false
    }

    const tokenRange = byteOffsetRange(token)
    if (!tokenRange) {
      return false
    }

    return scripts.some((script) => {
      const scriptRange = byteOffsetRange(script)
      return (
        scriptRange !== null && tokenRange[0] >= scriptRange[0] && tokenRange[1] <= scriptRange[1]
      )
    })
  })
}

function scriptBody(node: ScriptNode): OxlintProgram['body'] {
  return node.body ?? []
}

function scriptBodies(scripts: ScriptElement[]): OxlintProgram['body'] {
  return scripts.flatMap((script, index) => {
    const body = script.children.flatMap(scriptBody)
    if (index > 0) {
      stripDirectiveProperties(body)
    }
    return body
  })
}

function stripDirectiveProperties(body: OxlintProgram['body']): void {
  for (const node of body) {
    if (isObject(node) && typeof node.directive === 'string') {
      delete node.directive
    }
  }
}

function hasScriptBody(script: ScriptElement): boolean {
  return script.children.some((child) => scriptBody(child).length > 0)
}

function removePureScriptChildren(sfc: VueSingleFileComponent): void {
  for (const script of sfc.children.filter(isScriptElement)) {
    script.children = script.children.filter((child) => child.type !== 'VPureScript')
  }
}

function fillImplicitBindValues(sfc: VueSingleFileComponent): void {
  for (const child of sfc.children) {
    fillImplicitBindValue(child)
  }
}

function fillImplicitBindValue(value: unknown): void {
  if (!isObject(value)) {
    return
  }

  if (Array.isArray(value)) {
    for (const child of value) {
      fillImplicitBindValue(child)
    }
    return
  }

  if (value.type !== 'VElement') {
    return
  }

  const startTag = value.startTag
  if (isObject(startTag) && Array.isArray(startTag.attributes)) {
    for (const attribute of startTag.attributes) {
      if (isObject(attribute)) {
        fillImplicitBindAttributeValue(attribute)
      }
    }
  }
  fillImplicitBindValue(value.children)
}

function fillImplicitBindAttributeValue(attribute: NodeValue): void {
  if (attribute.directive !== true || attribute.value !== null) {
    return
  }

  const key = attribute.key
  if (!isObject(key) || !isBindDirectiveKey(key)) {
    return
  }

  const argument = key.argument
  if (!isObject(argument) || argument.type !== 'VIdentifier' || typeof argument.name !== 'string') {
    return
  }

  const argumentRange = byteOffsetRange(argument)
  if (!argumentRange) {
    return
  }

  const expression = {
    type: 'Identifier',
    name: camelize(argument.name),
    start: argumentRange[0],
    end: argumentRange[1],
    range: argumentRange,
  }
  markGeneratedIdentifier(expression)
  attribute.value = {
    type: 'VExpressionContainer',
    expression,
    references: [{ id: expression, mode: 'r', variable: null }],
    start: argumentRange[0],
    end: argumentRange[1],
    range: argumentRange,
  }
}

function isBindDirectiveKey(key: NodeValue): boolean {
  const name = key.name
  return isObject(name) && name.name === 'bind'
}

function createTemplateErrors(
  templateBody: TemplateBody,
  tokens: unknown[],
  offsetMap: SourceOffsetMap,
): SyntaxError[] {
  const errors: SyntaxError[] = []
  collectTemplateErrors(templateBody, offsetMap, errors)
  collectInvalidEndTagErrors(tokens, offsetMap, errors)
  return errors
}

function collectTemplateErrors(
  value: unknown,
  offsetMap: SourceOffsetMap,
  errors: SyntaxError[],
): void {
  if (!isObject(value)) {
    return
  }

  if (Array.isArray(value)) {
    for (const child of value) {
      collectTemplateErrors(child, offsetMap, errors)
    }
    return
  }

  if (value.type !== 'VElement') {
    return
  }

  const name = typeof value.name === 'string' ? value.name.toLowerCase() : ''
  const startTag = value.startTag
  if (isObject(startTag) && startTag.selfClosing === true && !voidElementNames.has(name)) {
    const range = byteOffsetRange(value)
    errors.push(createTemplateError(nonVoidSelfClosingError, range?.[0] ?? 0, offsetMap))
  }
  collectTemplateErrors(value.children, offsetMap, errors)
}

function collectInvalidEndTagErrors(
  tokens: unknown[],
  offsetMap: SourceOffsetMap,
  errors: SyntaxError[],
): void {
  for (const token of tokens) {
    if (
      isObject(token) &&
      token.type === 'HTMLEndTagOpen' &&
      typeof token.value === 'string' &&
      voidElementNames.has(token.value.toLowerCase())
    ) {
      const range = byteOffsetRange(token)
      errors.push(createTemplateError('x-invalid-end-tag', range?.[0] ?? 0, offsetMap))
    }
  }
}

function createTemplateError(
  message: string,
  start: number,
  offsetMap: SourceOffsetMap,
): SyntaxError {
  const location = offsetToLocation(offsetMap, start)
  const error = new SyntaxError(message)

  defineErrorProperty(error, 'message', message, false)
  defineErrorProperty(error, 'code', message)
  defineErrorProperty(error, 'index', toIndex(offsetMap, start))
  defineErrorProperty(error, 'lineNumber', location.line)
  defineErrorProperty(error, 'column', location.column)

  return error
}

function defineErrorProperty(
  error: SyntaxError,
  key: string,
  value: unknown,
  enumerable = true,
): void {
  Object.defineProperty(error, key, {
    configurable: true,
    enumerable,
    value,
    writable: true,
  })
}

function camelize(value: string): string {
  return value.replace(/-([a-z])/g, (_, character: string) => character.toUpperCase())
}

function normalizeError(error: NativeRange & { message: string }): ParseDiagnostic {
  return {
    message: error.message,
    start: error.start,
    end: error.end,
  }
}

function annotateProgram(root: object, offsetMap: SourceOffsetMap): void {
  attachLocation(root, offsetMap, 'js')
  annotateAst(root, offsetMap, 'js', null, new Set(['templateBody']))
}

function annotateVueAst(root: object, offsetMap: SourceOffsetMap): void {
  annotateAst(root, offsetMap, 'vue', null)
}

function annotateAst(
  root: object,
  offsetMap: SourceOffsetMap,
  mode: LocationMode,
  rootParent: object | null,
  skipKeys = new Set<string>(),
): void {
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

    if (value !== root && shouldAnnotate(value) && mode !== 'token') {
      Object.defineProperty(value, 'parent', {
        configurable: true,
        enumerable: true,
        value: parent,
        writable: true,
      })
    }
    attachLocation(value, offsetMap, mode)
    normalizeAstNode(value, mode)

    for (const [key, child] of Object.entries(value)) {
      if (skipKeys.has(key)) {
        continue
      }
      if (key === 'tokens' || key === 'comments' || key === 'errors') {
        if (Array.isArray(child)) {
          annotateList(child, offsetMap, 'token')
        }
        continue
      }
      if (key !== 'parent' && key !== 'loc') {
        visit(child, value)
      }
    }
    linkReferenceIds(value)
  }

  visit(root, rootParent)
}

function annotateList(values: unknown[], offsetMap: SourceOffsetMap, mode: LocationMode): void {
  for (const value of values) {
    if (!isObject(value)) {
      continue
    }

    attachLocation(value, offsetMap, mode)
    normalizeAstNode(value, mode)
  }
}

function normalizeAstNode(value: object, mode: LocationMode): void {
  if (mode !== 'js') {
    return
  }

  const node = value as NodeValue
  if (node.type === 'ImportDeclaration' && node.phase === null) {
    delete node.phase
  }
}

function attachLocation(value: object, offsetMap: SourceOffsetMap, mode: LocationMode): void {
  const node = value as NodeValue
  const locationByteRange = startEndOffsetRange(node) ?? rangeOffsetRange(node)
  const rangeByteRange = rangeOffsetRange(node) ?? locationByteRange
  if (!locationByteRange || !rangeByteRange) {
    return
  }

  const locationRange = toRange(offsetMap, {
    start: locationByteRange[0],
    end: locationByteRange[1],
  })
  const range = toRange(offsetMap, { start: rangeByteRange[0], end: rangeByteRange[1] })
  if (isGeneratedIdentifier(node)) {
    node.start = locationRange[0]
    node.end = undefined
  } else if (shouldKeepStartEnd(node, mode)) {
    if (mode === 'token') {
      markJavaScriptToken(node)
    }
    node.start = locationRange[0]
    node.end = locationRange[1]
  } else {
    delete node.start
    delete node.end
  }
  node.range = range

  Object.defineProperty(node, 'loc', {
    configurable: true,
    enumerable: true,
    get() {
      return {
        start: offsetToLocation(offsetMap, rangeByteRange[0]),
        end: offsetToLocation(offsetMap, rangeByteRange[1]),
      }
    },
  })
}

function byteOffsetRange(value: object): AST.OffsetRange | null {
  return startEndOffsetRange(value) ?? rangeOffsetRange(value)
}

function startEndOffsetRange(value: object): AST.OffsetRange | null {
  const node = value as NodeValue
  if (typeof node.start === 'number' && typeof node.end === 'number') {
    return [node.start, node.end]
  }

  return null
}

function rangeOffsetRange(value: object): AST.OffsetRange | null {
  const node = value as NodeValue
  if (
    Array.isArray(node.range) &&
    typeof node.range[0] === 'number' &&
    typeof node.range[1] === 'number'
  ) {
    return [node.range[0], node.range[1]]
  }

  return null
}

function linkReferenceIds(value: object): void {
  const node = value as NodeValue
  linkVariables(node)
  if (node.type !== 'VExpressionContainer' || !isObject(node.expression)) {
    return
  }

  if (!Array.isArray(node.references)) {
    return
  }

  const shouldAddReferenceKinds = !isGeneratedIdentifier(node.expression)
  for (const reference of node.references) {
    if (!isObject(reference)) {
      continue
    }
    const identifier = findIdentifierInSubtree(node.expression, reference.id)
    if (identifier) {
      reference.id = identifier
    } else if (sameRange(reference.id, node.expression)) {
      reference.id = node.expression
    }
    if (shouldAddReferenceKinds && !Object.hasOwn(reference, 'isValueReference')) {
      reference.isValueReference = undefined
    }
    if (shouldAddReferenceKinds && !Object.hasOwn(reference, 'isTypeReference')) {
      reference.isTypeReference = undefined
    }
  }
}

function linkVariables(node: NodeValue): void {
  if (!Array.isArray(node.variables)) {
    return
  }

  for (const variable of node.variables) {
    if (!isObject(variable)) {
      continue
    }
    const candidate = findIdentifierInSubtree(node, variable.id)
    if (candidate) {
      variable.id = candidate
    }
  }
}

function findIdentifierInSubtree(root: object, target: unknown): object | null {
  const seen = new WeakSet<object>()
  const visit = (value: unknown): object | null => {
    if (!isObject(value) || seen.has(value)) {
      return null
    }
    seen.add(value)
    if (value !== target && value.type === 'Identifier' && sameRange(value, target)) {
      return value
    }
    if (value.type === 'Property') {
      const found = visit(value.value)
      if (found) {
        return found
      }
    }
    for (const [key, child] of Object.entries(value)) {
      if (key === 'parent' || key === 'loc' || key === 'tokens' || key === 'comments') {
        continue
      }
      if (key === 'variables' || (value.type === 'Property' && key === 'value')) {
        continue
      }
      const found = visit(child)
      if (found) {
        return found
      }
    }
    return null
  }

  return visit(root)
}

function sameRange(left: unknown, right: unknown): boolean {
  if (!isObject(left) || !isObject(right)) {
    return false
  }

  const leftRange = byteOffsetRange(left)
  const rightRange = byteOffsetRange(right)
  return (
    leftRange !== null &&
    rightRange !== null &&
    leftRange[0] === rightRange[0] &&
    leftRange[1] === rightRange[1]
  )
}

function shouldKeepStartEnd(node: NodeValue, mode: LocationMode): boolean {
  if (mode === 'js') {
    return true
  }

  if (mode === 'token') {
    return isMarkedJavaScriptToken(node) || isJavaScriptToken(node)
  }

  return !isVueAstNode(node)
}

function isVueAstNode(node: NodeValue): boolean {
  return typeof node.type === 'string' && node.type.startsWith('V')
}

function isJavaScriptToken(node: NodeValue): boolean {
  if (typeof node.type !== 'string') {
    return false
  }

  if (Object.hasOwn(node, 'range')) {
    return false
  }

  if (node.type.startsWith('HTML') || node.type.startsWith('VExpression')) {
    return false
  }

  return true
}

function markJavaScriptToken(node: NodeValue): void {
  Object.defineProperty(node, javascriptToken, {
    configurable: true,
    value: true,
  })
}

function markGeneratedIdentifier(node: NodeValue): void {
  Object.defineProperty(node, generatedIdentifier, {
    configurable: true,
    value: true,
  })
}

function isMarkedJavaScriptToken(node: NodeValue): boolean {
  return (node as NodeValue & { [javascriptToken]?: boolean })[javascriptToken] === true
}

function isGeneratedIdentifier(node: NodeValue): boolean {
  return (node as NodeValue & { [generatedIdentifier]?: boolean })[generatedIdentifier] === true
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

function isScriptElement(node: VueSingleFileComponent['children'][number]): node is ScriptElement {
  return node.type === 'VElement' && node.name === 'script'
}

function isSetupScriptElement(node: ScriptElement): boolean {
  const attributes = node.startTag.attributes
  return attributes.some((attribute: unknown) => {
    if (!isObject(attribute) || attribute.directive !== false) {
      return false
    }

    const key = attribute.key
    return isObject(key) && key.name === 'setup'
  })
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
