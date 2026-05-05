import type { AstNode, Locator, OxlintProgram } from './types'

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

export function toProgram(sfc: AstNode, source: string, locator: Locator): OxlintProgram {
  const body: AstNode[] = []
  const templateChildren: AstNode[] = []
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
      continue
    }

    if (child.type === 'VElement' && child.name === 'script') {
      const scriptChildren = getNodeArray(child.children)
      const scriptIsSetup = isScriptSetupElement(child)
      if (!sawScriptElement) {
        firstScriptIsSetup = scriptIsSetup
        sawScriptElement = true
      }
      for (const scriptChild of scriptChildren) {
        if (scriptChild.type !== 'VPureScript') {
          continue
        }
        appendScriptBody(body, scriptChild, scriptBlockCount === 0)
        ;[programStart, programEnd, hasProgramSpan] = includeProgramSpan(
          programStart,
          programEnd,
          hasProgramSpan,
          scriptChild,
        )
        scriptBlockCount += 1
      }
      child.children = scriptChildren.map((scriptChild) => {
        if (scriptChild.type !== 'VPureScript') {
          return scriptChild
        }
        const start = scriptChild.start ?? 0
        const end = scriptChild.end ?? start
        return { type: 'VText', value: source.slice(start, end), start, end, range: [start, end] }
      })
    }

    templateChildren.push(child)
  }

  if (body.length === 0) {
    programStart = 0
    programEnd = 0
  }

  sfc.children = templateChildren
  const templateBody =
    templateChildren.find((child) => child.type === 'VElement' && child.name === 'template') ?? null
  if (templateBody) {
    templateBody.comments = getArray(sfc.template_comments)
    templateBody.tokens = getArray(sfc.templateTokens)
  }
  const templateErrors = templateBody ? collectTemplateErrors(templateBody, locator) : []
  if (templateBody) {
    templateBody.errors = templateErrors
  }

  const fragment: AstNode = {
    type: 'VDocumentFragment',
    children: templateChildren,
    comments: getArray(sfc.template_comments),
    errors: templateErrors,
    parent: null,
    range: rangeFromAstNode(sfc, source.length),
    start: sfc.start,
    end: sfc.end,
    tokens: getArray(sfc.templateTokens),
  }
  const rawProgramTokens = getNodeArray(sfc.scriptTokens)
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

  attachMetadata(fragment, null, locator)
  attachMetadata(program, null, locator)
  return program as OxlintProgram
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

  if (
    (typeof value.start === 'number' && typeof value.end === 'number') ||
    (Array.isArray(value.range) &&
      typeof value.range[0] === 'number' &&
      typeof value.range[1] === 'number')
  ) {
    const start = value.start ?? value.range?.[0] ?? 0
    const end = value.end ?? value.range?.[1] ?? start
    value.range ??= [start, end]
    const [locationStart, locationEnd] = value.range

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
      attachReferenceLikeMetadata(child, value, locator, key === 'references')
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

function attachReferenceLikeMetadata(
  value: unknown,
  parent: AstNode,
  locator: Locator,
  isReference: boolean,
) {
  if (!Array.isArray(value)) {
    return
  }

  for (const item of value) {
    if (!isObject(item)) {
      continue
    }
    if (isReference && !('variable' in item)) {
      item.isValueReference = undefined
      item.isTypeReference = undefined
    }
    const id = getNode(item.id)
    const matchingId = findIdentifier(parent, id)
    if (matchingId) {
      item.id = matchingId
      continue
    }
    attachMetadata(id, getNode(id?.parent) ?? parent, locator)
  }
}

function findIdentifier(root: AstNode, target: AstNode | undefined): AstNode | undefined {
  if (!target) {
    return undefined
  }

  const stack = [root]
  while (stack.length > 0) {
    const node = stack.pop()
    if (!node) {
      continue
    }
    if (
      node !== target &&
      node.type === target.type &&
      node.name === target.name &&
      node.start === target.start &&
      node.end === target.end
    ) {
      return node
    }

    for (const [key, child] of Object.entries(node)) {
      if (
        key === 'parent' ||
        key === 'loc' ||
        key === 'tokens' ||
        key === 'comments' ||
        key === 'references' ||
        key === 'variables'
      ) {
        continue
      }
      if (Array.isArray(child)) {
        for (const item of child) {
          if (isObject(item)) {
            stack.push(item)
          }
        }
      } else if (isObject(child)) {
        stack.push(child)
      }
    }
  }
}

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

function appendScriptBody(body: AstNode[], script: AstNode, keepDirectives: boolean) {
  const statements = getNodeArray(script.body)
  if (!keepDirectives) {
    for (const statement of statements) {
      delete statement.directive
    }
  }
  body.push(...statements)
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

function tokenRange(token: AstNode): [number, number] {
  return token.range ?? [token.start ?? 0, token.end ?? 0]
}

function rangeFromAstNode(node: AstNode, fallbackEnd: number): [number, number] {
  return node.range ?? [node.start ?? 0, node.end ?? fallbackEnd]
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
