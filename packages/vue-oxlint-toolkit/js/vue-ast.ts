import type { AstNode, Locator, OxlintProgram, Reference, Variable } from './types'

const htmlNamespace = 'http://www.w3.org/1999/xhtml'
const htmlTokenTypes = new Set([
  'HTMLAssociation',
  'HTMLCDataText',
  'HTMLComment',
  'HTMLEndTagOpen',
  'HTMLIdentifier',
  'HTMLLiteral',
  'HTMLRawText',
  'HTMLRCDataText',
  'HTMLSelfClosingTagClose',
  'HTMLTagClose',
  'HTMLTagOpen',
  'HTMLText',
  'HTMLWhitespace',
  'VExpressionEnd',
  'VExpressionStart',
])
const vueAstNodeTypes = new Set([
  'VAttribute',
  'VDirectiveKey',
  'VDocumentFragment',
  'VElement',
  'VEndTag',
  'VExpressionContainer',
  'VForExpression',
  'VIdentifier',
  'VLiteral',
  'VOnExpression',
  'VSlotScopeExpression',
  'VStartTag',
  'VText',
])
const jsTokenTypes = new Set([
  'Boolean',
  'Identifier',
  'JSXIdentifier',
  'Keyword',
  'Null',
  'Numeric',
  'Punctuator',
  'RegularExpression',
  'String',
  'Template',
])

export function toProgram(sfc: AstNode, source: string, locator: Locator): OxlintProgram {
  const body: AstNode[] = []
  const templateChildren: AstNode[] = []
  const rawTemplateTokens = getNodeArray(sfc.templateTokens)
  const scriptTokens = getNodeArray(sfc.scriptTokens)
  let programStart = typeof sfc.start === 'number' ? sfc.start : 0
  let programEnd = typeof sfc.end === 'number' ? sfc.end : source.length
  let hasProgramSpan = false
  let scriptBlockCount = 0
  let firstScriptIsSetup = false
  let sawScriptElement = false

  for (const child of getNodeArray(sfc.children)) {
    if (child.type === 'VPureScript') {
      appendScriptBody(body, child, scriptBlockCount === 0)
      ;[programStart, programEnd, hasProgramSpan] = includeProgramSpan(
        programStart,
        programEnd,
        hasProgramSpan,
        child,
      )
      scriptBlockCount += 1
    } else if (child.type === 'VElement' && child.name === 'script') {
      const scriptChildren = getNodeArray(child.children)
      const scriptIsSetup = isScriptSetupElement(child)
      if (!sawScriptElement) {
        firstScriptIsSetup = scriptIsSetup
        sawScriptElement = true
      }
      for (const scriptChild of scriptChildren) {
        if (scriptChild.type === 'VPureScript') {
          appendScriptBody(body, scriptChild, scriptBlockCount === 0)
          ;[programStart, programEnd, hasProgramSpan] = includeProgramSpan(
            programStart,
            programEnd,
            hasProgramSpan,
            scriptChild,
          )
          scriptBlockCount += 1
        }
      }
      child.children = scriptChildren.map((scriptChild) => {
        if (scriptChild.type !== 'VPureScript') {
          return scriptChild
        }
        const start = scriptChild.start ?? 0
        const end = scriptChild.end ?? start
        return { type: 'VText', value: source.slice(start, end), start, end, range: [start, end] }
      })
      templateChildren.push(child)
    } else {
      templateChildren.push(child)
    }
  }

  if (body.length === 0) {
    programStart = 0
    programEnd = 0
  }

  sfc.children = templateChildren
  sfc.comments = getArray(sfc.template_comments)
  sfc.tokens = normalizeTemplateTokens(rawTemplateTokens, source)
  const templateCommentRanges = getNodeArray(sfc.template_comments).map((comment) =>
    tokenRange(comment),
  )
  normalizeVueAst(sfc, source, templateCommentRanges)
  const normalizedTemplateChildren = getNodeArray(sfc.children)
  const templateBody =
    normalizedTemplateChildren.find(
      (child) => child.type === 'VElement' && child.name === 'template',
    ) ?? null

  if (templateBody) {
    templateBody.comments = getArray(sfc.template_comments)
    templateBody.tokens = sfc.tokens
  }
  const templateErrors = templateBody ? collectTemplateErrors(templateBody, locator) : []
  if (templateBody) {
    templateBody.errors = templateErrors
  }

  const fragment: AstNode = {
    type: 'VDocumentFragment',
    children: normalizedTemplateChildren,
    comments: getArray(sfc.template_comments),
    errors: templateErrors,
    parent: null,
    range: rangeFromAstNode(sfc, source.length),
    start: sfc.start,
    end: sfc.end,
    tokens: sfc.tokens,
  }
  const rawProgramTokens = normalizeProgramTokens(rawTemplateTokens, scriptTokens, source)
  if (body.length === 0 && rawProgramTokens.length > 0) {
    programStart = tokenRange(rawProgramTokens[0])[1]
    programEnd = programStart
  }
  const programTokens =
    body.length === 0 ? (firstScriptIsSetup ? [] : rawProgramTokens.slice(0, 2)) : rawProgramTokens
  const programRange = getProgramRange(programStart, programEnd, body, {
    firstScriptIsSetup,
    scriptBlockCount,
  })

  const program: AstNode = {
    type: 'Program',
    sourceType: typeof sfc.source_type === 'string' ? sfc.source_type : 'module',
    body,
    comments: getArray(sfc.script_comments),
    tokens: programTokens,
    templateBody: templateBody ?? undefined,
    start: programStart,
    end: programEnd,
    range: programRange,
  }
  if (body.length > 0 || rawProgramTokens.length > 0) {
    program.parent = null
  }

  normalizeVueAst(program, source, templateCommentRanges)
  attachMetadata(fragment, null, locator)
  attachMetadata(program, null, locator)
  return program as OxlintProgram
}

function normalizeVueAst(node: unknown, source: string, commentRanges: Array<[number, number]>) {
  if (!isObject(node)) {
    return
  }

  normalizeCurrentNode(node, source, commentRanges)

  for (const child of Object.values(node)) {
    if (Array.isArray(child)) {
      for (const item of child) {
        normalizeVueAst(item, source, commentRanges)
      }
    } else {
      normalizeVueAst(child, source, commentRanges)
    }
  }
}

function normalizeCurrentNode(
  node: AstNode,
  source: string,
  commentRanges: Array<[number, number]>,
) {
  if (!node.type && getNode(node.name)?.type === 'VIdentifier' && Array.isArray(node.modifiers)) {
    node.type = 'VDirectiveKey'
  }

  if (node.type === 'VPureAttribute') {
    if (isShorthandBindAttribute(node)) {
      convertShorthandBindAttribute(node)
    } else {
      node.type = 'VAttribute'
      node.directive = false
    }
  }

  if (node.type === 'VPureAttribute') {
    node.type = 'VAttribute'
    node.directive = false
  }

  scrubOxcOnlyDefaults(node)

  if (node.type === 'VElement') {
    node.namespace = htmlNamespace
    if (typeof node.rawName === 'string') {
      node.name = node.rawName.toLowerCase()
    }
    if (node.name === 'style') {
      node.style = true
    }
    node.variables = collectElementVariables(node)
    const startTag = getNode(node.startTag)
    if (startTag) {
      delete startTag.variables
    }
  }

  if (Array.isArray(node.children)) {
    node.children = mergeAdjacentTextNodes(node.children, commentRanges)
  }

  if (node.type === 'VStartTag' && isEmptyArray(node.variables)) {
    delete node.variables
  }

  if (node.type === 'VDirectiveKey') {
    node.type = 'VDirectiveKey'
    const argument = getNode(node.argument)
    if (argument?.type === 'VIdentifier' && argument.name === '') {
      node.argument = null
    } else if (argument?.type === 'VIdentifier' && isDynamicArgument(argument)) {
      node.argument = createDynamicArgument(argument)
    }
  }

  if (node.type === 'VText' && typeof node.text === 'string') {
    node.value = node.text
    delete node.text
  }

  if (node.type === 'VExpressionContainer') {
    includeDirectiveValueQuotes(node, source)
    if (!Array.isArray(node.references) || node.references.length === 0) {
      node.references = getArray(node.reference).length
        ? node.reference
        : collectExpressionReferences(node.expression)
    }
    for (const reference of getNodeArray(node.references)) {
      reference.isValueReference = undefined
      reference.isTypeReference = undefined
    }
    delete node.reference
  }

  if (
    node.type === 'VLiteral' &&
    typeof node.value === 'string' &&
    typeof node.start === 'number' &&
    source[node.start] === '='
  ) {
    node.start += 1
    node.range = [node.start, node.end ?? node.start]
  }
}

function normalizeTemplateTokens(tokens: AstNode[], source: string): AstNode[] {
  const normalized: AstNode[] = []
  let inTag = false

  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index]
    if (inTag && token.type === 'HTMLWhitespace') {
      continue
    }

    if (
      (token.type === 'HTMLTagOpen' || token.type === 'HTMLEndTagOpen') &&
      getNode(tokens[index + 1])?.type === 'HTMLIdentifier'
    ) {
      const name = getNode(tokens[index + 1])
      normalized.push({
        type: token.type,
        value: typeof name?.value === 'string' ? name.value.toLowerCase() : name?.value,
        start: token.start,
        end: name?.end,
        range: [token.start ?? 0, name?.end ?? token.end ?? 0],
      })
      inTag = true
      index += 1
      continue
    }

    if (token.type === 'VExpressionStart') {
      normalized.push({ ...token, value: '{{' })
      const expressionText = getNode(tokens[index + 1])
      if (expressionText?.type === 'HTMLText') {
        normalized.push(...tokensFromExpressionText(expressionText, source))
        index += 1
      }
      continue
    }

    if (token.type === 'VExpressionEnd') {
      normalized.push({ ...token, value: '}}' })
      continue
    }

    if (token.type === 'HTMLLiteral') {
      const expressionTokens = collectExpressionTokensInLiteral(tokens, index)
      if (expressionTokens.length > 0) {
        normalized.push(...tokensForQuotedExpression(token, expressionTokens, source))
        index += expressionTokens.length
        continue
      }
    }

    if (
      token.type === 'HTMLIdentifier' &&
      typeof token.value === 'string' &&
      token.value.startsWith('[') &&
      token.value.endsWith(']')
    ) {
      normalized.push(...tokensForDynamicArgument(token))
      continue
    }

    if (token.type === 'Punctuator') {
      normalized.push(markTemplatePunctuator(token))
      continue
    }

    if (token.type === 'HTMLTagClose' || token.type === 'HTMLAssociation') {
      normalized.push({ ...token, value: '' })
      if (token.type === 'HTMLTagClose') {
        inTag = false
      }
      continue
    }

    if (token.type === 'HTMLSelfClosingTagClose') {
      normalized.push({ ...token, value: '' })
      inTag = false
      continue
    }

    normalized.push(token)
  }

  return normalized
}

function collectExpressionTokensInLiteral(tokens: AstNode[], literalIndex: number) {
  const literal = tokens[literalIndex]
  const literalRange = tokenRange(literal)
  const expressionTokens: AstNode[] = []

  for (let index = literalIndex + 1; index < tokens.length; index += 1) {
    const token = tokens[index]
    const range = tokenRange(token)
    if (
      range[0] < literalRange[0] ||
      range[1] > literalRange[1] ||
      !jsTokenTypes.has(token.type ?? '')
    ) {
      break
    }
    expressionTokens.push(token)
  }

  return expressionTokens
}

function tokensForQuotedExpression(literal: AstNode, expressionTokens: AstNode[], source: string) {
  const [start, end] = tokenRange(literal)
  const quote = source[start]
  if (quote !== '"' && quote !== "'") {
    return expressionTokens
  }

  return [
    createTemplatePunctuator(quote, start, start + 1),
    ...expressionTokens,
    createTemplatePunctuator(quote, end - 1, end),
  ]
}

function tokensForDynamicArgument(token: AstNode): AstNode[] {
  const [start, end] = tokenRange(token)
  const value = String(token.value)
  return [
    createTemplatePunctuator('[', start, start + 1),
    {
      type: 'Identifier',
      value: value.slice(1, -1),
      start: start + 1,
      end: end - 1,
      range: [start + 1, end - 1],
    },
    createTemplatePunctuator(']', end - 1, end),
  ]
}

function createTemplatePunctuator(value: string, start: number, end: number): AstNode {
  return markTemplatePunctuator({ type: 'Punctuator', value, start, end, range: [start, end] })
}

function markTemplatePunctuator(token: AstNode) {
  return { ...token, __templatePunctuator: true }
}

function normalizeProgramTokens(
  templateTokens: AstNode[],
  scriptTokens: AstNode[],
  source: string,
): AstNode[] {
  const tokens: AstNode[] = []
  const scriptTagPairs = findScriptTagPairs(templateTokens, source)

  for (const pair of scriptTagPairs) {
    tokens.push(pair.open)
    tokens.push(
      ...scriptTokens.filter(
        (token) =>
          typeof token.start === 'number' &&
          typeof token.end === 'number' &&
          token.start >= tokenRange(pair.open)[1] &&
          token.end <= tokenRange(pair.close)[0],
      ),
    )
    tokens.push(pair.close)
  }

  return tokens
}

function findScriptTagPairs(tokens: AstNode[], source: string) {
  const pairs: Array<{ open: AstNode; close: AstNode }> = []
  const stack: AstNode[] = []

  for (let index = 0; index < tokens.length; index += 1) {
    const token = tokens[index]
    const name = getNode(tokens[index + 1])
    if (name?.type !== 'HTMLIdentifier' || String(name.value).toLowerCase() !== 'script') {
      continue
    }

    const close = findTagClose(tokens, index + 2)
    if (!close) {
      continue
    }

    if (token.type === 'HTMLTagOpen') {
      stack.push({
        type: 'Punctuator',
        value: '<script>',
        start: token.start,
        end: close.end,
        range: [token.start ?? 0, close.end ?? token.end ?? 0],
      })
    } else if (token.type === 'HTMLEndTagOpen') {
      const open = stack.pop()
      if (open) {
        pairs.push({
          open,
          close: {
            type: 'Punctuator',
            value: '</script>',
            start: token.start,
            end: close.end,
            range: [token.start ?? 0, close.end ?? token.end ?? 0],
          },
        })
      }
    }
  }

  return pairs.filter((pair) => {
    const range = tokenRange(pair.open)
    return source.slice(range[0], range[1]).includes('<script')
  })
}

function findTagClose(tokens: AstNode[], start: number): AstNode | undefined {
  for (let index = start; index < tokens.length; index += 1) {
    const token = tokens[index]
    if (token.type === 'HTMLTagClose' || token.type === 'HTMLSelfClosingTagClose') {
      return token
    }
  }
}

function tokenRange(token: AstNode): [number, number] {
  return token.range ?? [token.start ?? 0, token.end ?? 0]
}

function includeProgramSpan(
  currentStart: number,
  currentEnd: number,
  hasCurrentSpan: boolean,
  node: AstNode,
): [number, number, boolean] {
  const start = node.start ?? currentStart
  const end = node.end ?? currentEnd
  if (!hasCurrentSpan) {
    return [start, end, true]
  }
  return [Math.min(currentStart, start), Math.max(currentEnd, end), true]
}

function appendScriptBody(body: AstNode[], script: AstNode, keepDirectives: boolean) {
  const statements = getNodeArray(script.body)
  if (!keepDirectives) {
    for (const statement of statements) {
      delete statement.directive
    }
  }
  body.push(...statements)
}

function getProgramRange(
  start: number,
  end: number,
  body: AstNode[],
  metadata: { firstScriptIsSetup: boolean; scriptBlockCount: number },
): [number, number] {
  if (metadata.scriptBlockCount < 2 || body.length === 0) {
    return [start, end]
  }

  const firstBody = body[0]
  const lastBody = body.at(-1)
  return [metadata.firstScriptIsSetup ? (firstBody.start ?? start) : start, lastBody?.end ?? end]
}

function isScriptSetupElement(element: AstNode) {
  const startTag = getNode(element.startTag)
  return getNodeArray(startTag?.attributes).some((attribute) => {
    const key = getNode(attribute.key)
    return key?.type === 'VIdentifier' && key.name === 'setup'
  })
}

const voidHtmlElements = new Set([
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

function collectTemplateErrors(templateBody: AstNode, locator: Locator) {
  const errors: SyntaxError[] = []
  collectTemplateErrorsInto(templateBody, errors, locator)
  for (const token of getNodeArray(templateBody.tokens)) {
    if (
      token.type === 'HTMLEndTagOpen' &&
      typeof token.value === 'string' &&
      voidHtmlElements.has(token.value.toLowerCase())
    ) {
      errors.push(createParseError('x-invalid-end-tag', tokenRange(token)[0], locator))
    }
  }
  return errors
}

function collectTemplateErrorsInto(node: AstNode, errors: SyntaxError[], locator: Locator) {
  if (node.type === 'VElement') {
    const startTag = getNode(node.startTag)
    if (
      startTag?.selfClosing === true &&
      typeof node.name === 'string' &&
      !voidHtmlElements.has(node.name.toLowerCase()) &&
      typeof node.start === 'number'
    ) {
      errors.push(
        createParseError(
          'non-void-html-element-start-tag-with-trailing-solidus',
          node.start,
          locator,
        ),
      )
    }
  }

  for (const child of getNodeArray(node.children)) {
    collectTemplateErrorsInto(child, errors, locator)
  }
}

function createParseError(message: string, index: number, locator: Locator) {
  const location = locator(index)
  const error = new SyntaxError(message)
  Object.assign(error, {
    code: message,
    index,
    lineNumber: location.line,
    column: location.column,
  })
  return error
}

function mergeAdjacentTextNodes(children: unknown[], commentRanges: Array<[number, number]>) {
  const merged: unknown[] = []

  for (const child of children) {
    const previous = getNode(merged.at(-1))
    const current = getNode(child)
    if (
      previous?.type !== 'VText' ||
      current?.type !== 'VText' ||
      hasCommentBetween(previous, current, commentRanges)
    ) {
      merged.push(child)
      continue
    }

    const previousValue = textNodeValue(previous)
    const currentValue = textNodeValue(current)
    previous.value = previousValue + currentValue
    delete previous.text
    previous.end = current.end
    previous.range = [
      previous.start ?? previous.range?.[0] ?? 0,
      current.end ?? current.range?.[1] ?? 0,
    ]
  }

  return merged
}

function hasCommentBetween(
  previous: AstNode,
  current: AstNode,
  commentRanges: Array<[number, number]>,
) {
  const previousEnd = previous.end ?? previous.range?.[1]
  const currentStart = current.start ?? current.range?.[0]
  if (typeof previousEnd !== 'number' || typeof currentStart !== 'number') {
    return false
  }

  return commentRanges.some(([start, end]) => start >= previousEnd && end <= currentStart)
}

function textNodeValue(node: AstNode) {
  if (typeof node.value === 'string') {
    return node.value
  }
  if (typeof node.text === 'string') {
    return node.text
  }
  return ''
}

function rangeFromAstNode(node: AstNode, fallbackEnd: number): [number, number] {
  return node.range ?? [node.start ?? 0, node.end ?? fallbackEnd]
}

function tokensFromExpressionText(token: AstNode, source: string): AstNode[] {
  if (typeof token.start !== 'number' || typeof token.end !== 'number') {
    return []
  }

  const raw = source.slice(token.start, token.end)
  const leading = raw.length - raw.trimStart().length
  const value = raw.trim()
  if (!/^[A-Za-z_$][\w$]*$/.test(value)) {
    return []
  }

  const start = token.start + leading
  return [
    {
      type: 'Identifier',
      value,
      start,
      end: start + value.length,
      range: [start, start + value.length],
    },
  ]
}

function isDynamicArgument(argument: AstNode) {
  return (
    typeof argument.name === 'string' &&
    argument.name.startsWith('[') &&
    argument.name.endsWith(']') &&
    typeof argument.start === 'number' &&
    typeof argument.end === 'number'
  )
}

function isShorthandBindAttribute(node: AstNode) {
  const key = getNode(node.key)
  return (
    key?.type === 'VIdentifier' &&
    typeof key.rawName === 'string' &&
    key.rawName.startsWith(':') &&
    node.value === null
  )
}

function convertShorthandBindAttribute(node: AstNode) {
  const key = getNode(node.key)
  const rawName = typeof key?.rawName === 'string' ? key.rawName : ''
  const argumentName = rawName.slice(1)
  const attrStart = node.start ?? key?.start ?? 0
  const attrEnd = node.end ?? key?.end ?? attrStart
  const argumentStart = attrStart + 1
  const expression = createIdentifierExpression(camelize(argumentName), argumentStart, attrEnd)

  node.type = 'VAttribute'
  node.directive = true
  node.key = {
    type: 'VDirectiveKey',
    name: {
      type: 'VIdentifier',
      name: 'bind',
      rawName: ':',
      start: attrStart,
      end: argumentStart,
      range: [attrStart, argumentStart],
    },
    argument: {
      type: 'VIdentifier',
      name: argumentName,
      rawName: argumentName,
      start: argumentStart,
      end: attrEnd,
      range: [argumentStart, attrEnd],
    },
    modifiers: [],
    start: attrStart,
    end: attrEnd,
    range: [attrStart, attrEnd],
  }
  node.value = {
    type: 'VExpressionContainer',
    expression,
    references: [{ id: expression, mode: 'r', variable: null }],
    start: argumentStart,
    end: attrEnd,
    range: [argumentStart, attrEnd],
  }
}

function createDynamicArgument(argument: AstNode): AstNode {
  const start = argument.start ?? 0
  const end = argument.end ?? start
  const expressionStart = start + 1
  const expressionEnd = Math.max(expressionStart, end - 1)
  const name = String(argument.name).slice(1, -1)
  const expression: AstNode = {
    type: 'Identifier',
    name,
    start: expressionStart,
    end: expressionEnd,
    range: [expressionStart, expressionEnd],
  }

  return {
    type: 'VExpressionContainer',
    expression,
    references: [
      { id: expression, mode: 'r', isValueReference: undefined, isTypeReference: undefined },
    ],
    start,
    end,
    range: [start, end],
  }
}

function createIdentifierExpression(name: string, start: number, end: number): AstNode {
  return { type: 'Identifier', name, start, end, range: [start, end] }
}

function camelize(value: string) {
  return value.replace(/-([a-z])/gu, (_, character: string) => character.toUpperCase())
}

function includeDirectiveValueQuotes(node: AstNode, source: string) {
  const expression = getNode(node.expression)
  if (
    !expression ||
    typeof node.start !== 'number' ||
    typeof node.end !== 'number' ||
    node.start === 0
  ) {
    return
  }

  const quote = source[node.start - 1]
  if ((quote !== '"' && quote !== "'") || source[node.end] !== quote) {
    return
  }

  node.start -= 1
  node.end += 1
  node.range = [node.start, node.end]
}

function scrubOxcOnlyDefaults(node: AstNode) {
  if (isEmptyArray(node.decorators)) {
    delete node.decorators
  }
  if (
    node.optional === false &&
    node.type !== 'CallExpression' &&
    node.type !== 'MemberExpression'
  ) {
    delete node.optional
  }
  if (node.typeAnnotation === null) {
    delete node.typeAnnotation
  }
  if (node.definite === false) {
    delete node.definite
  }
  if (node.declare === false) {
    delete node.declare
  }
  if (node.importKind === 'value') {
    delete node.importKind
  }
  if (node.exportKind === 'value') {
    delete node.exportKind
  }
  if (node.phase === null) {
    delete node.phase
  }
  if (node.returnType === null) {
    delete node.returnType
  }
  if (node.typeParameters === null) {
    delete node.typeParameters
  }
  if (node.typeArguments === null) {
    delete node.typeArguments
  }
  if (node.directive === null) {
    delete node.directive
  }
}

function collectElementVariables(element: AstNode): Variable[] {
  const variables: Variable[] = []
  const seen = new Set<string>()
  const startTag = getNode(element.startTag)

  for (const attribute of getNodeArray(startTag?.attributes)) {
    const value = getNode(attribute.value)
    const expression = getNode(value?.expression)
    if (expression?.type === 'VForExpression') {
      for (const id of collectPatternIdentifiers(expression.left)) {
        pushVariable(variables, seen, id, 'v-for')
      }
    }
    if (expression?.type === 'VSlotScopeExpression') {
      for (const id of collectPatternIdentifiers(expression.params)) {
        pushVariable(variables, seen, id, 'scope')
      }
    }
  }

  return variables
}

function pushVariable(variables: Variable[], seen: Set<string>, id: AstNode, kind: string) {
  const key = `${kind}:${id.start}:${id.end}:${String(id.name)}`
  if (seen.has(key)) {
    return
  }

  seen.add(key)
  variables.push({ id, kind })
}

function collectExpressionReferences(expression: unknown): Reference[] {
  const references: Reference[] = []
  const localStack: Array<Set<string>> = []

  visitExpression(expression, references, localStack)
  return references
}

function visitExpression(node: unknown, references: Reference[], localStack: Array<Set<string>>) {
  if (!isObject(node)) {
    return
  }

  if (
    node.type === 'Identifier' &&
    typeof node.name === 'string' &&
    !isLocalReference(node.name, localStack)
  ) {
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
    for (const param of getArray(node.params)) {
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

function collectPatternNames(node: unknown, names: Set<string>) {
  if (!isObject(node)) {
    return
  }

  if (node.type === 'Identifier' && typeof node.name === 'string') {
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

function collectPatternIdentifiers(node: unknown) {
  const identifiers: AstNode[] = []
  collectPatternIdentifierInto(node, identifiers)
  return identifiers
}

function collectPatternIdentifierInto(node: unknown, identifiers: AstNode[]) {
  if (!isObject(node)) {
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

function attachMetadata(value: unknown, parent: AstNode | null, locator: Locator) {
  if (!isObject(value)) {
    return
  }

  if (parent && !isToken(value)) {
    Object.defineProperty(value, 'parent', {
      configurable: true,
      enumerable: true,
      value: parent,
      writable: true,
    })
  }

  if (typeof value.start === 'number' && typeof value.end === 'number') {
    const start = value.start
    const end = value.end
    value.range ??= [start, end]
    const [locationStart, locationEnd] = value.range

    // vue-eslint-parser exposes these fields during equality checks; using a
    // getter avoids precomputing locations for every nested node.
    Object.defineProperty(value, 'loc', {
      configurable: true,
      enumerable: true,
      get() {
        return {
          start: locator(locationStart),
          end: locator(locationEnd),
        }
      },
    })

    if (
      typeof value.type === 'string' &&
      (vueAstNodeTypes.has(value.type) || htmlTokenTypes.has(value.type) || isTagPunctuator(value))
    ) {
      delete value.start
      delete value.end
    }
    if (value.__templatePunctuator === true) {
      delete value.start
      delete value.end
      delete value.__templatePunctuator
    }
  }

  for (const [key, child] of Object.entries(value)) {
    if (key === 'parent' || key === 'loc') {
      continue
    }

    if (key === 'tokens' || key === 'comments' || key === 'errors') {
      if (Array.isArray(child)) {
        for (const item of child) {
          attachMetadata(item, null, locator)
        }
      }
      continue
    }

    if (key === 'references' || key === 'variables') {
      attachReferenceLikeMetadata(child, value, locator)
      continue
    }

    if (key === 'templateBody' && isObject(child) && isObject(child.parent)) {
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

function attachReferenceLikeMetadata(value: unknown, parent: AstNode, locator: Locator) {
  if (!Array.isArray(value)) {
    return
  }

  for (const item of value) {
    if (!isObject(item)) {
      continue
    }
    const id = getNode(item.id)
    attachMetadata(id, getNode(id?.parent) ?? parent, locator)
  }
}

function getArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : []
}

function getNodeArray(value: unknown): AstNode[] {
  return getArray(value).filter(isObject)
}

function getNode(value: unknown): AstNode | undefined {
  return isObject(value) ? value : undefined
}

function isEmptyArray(value: unknown) {
  return Array.isArray(value) && value.length === 0
}

function isObject(value: unknown): value is AstNode {
  return value !== null && typeof value === 'object'
}

function isToken(value: AstNode) {
  return typeof value.type === 'string' && htmlTokenTypes.has(value.type)
}

function isTagPunctuator(value: AstNode) {
  return (
    value.type === 'Punctuator' && typeof value.value === 'string' && value.value.startsWith('<')
  )
}
