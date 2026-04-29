/**
 * A small ESTree (+ JSX + a subset of TypeScript) printer that walks an AST
 * produced by `oxc_estree` and emits source text. The walker exposes hooks
 * that fire on enter/leave for every node, so callers can attach behaviour
 * such as building a source-mapping table while generation happens.
 *
 * It intentionally favours coverage over fidelity: the toolkit's parser
 * produces a fairly stable AST shape (Vue SFC -> JSX), so the printer
 * focuses on the node kinds that show up there. Unknown node kinds fall
 * back to a best-effort visit that still fires hooks for nested children.
 */

export interface AstNode {
  type: string
  start?: number
  end?: number
  [key: string]: unknown
}

export interface CodegenHook {
  enter?: (node: AstNode, virtualOffset: number) => void
  leave?: (node: AstNode, virtualOffset: number) => void
}

export interface CodegenOptions {
  hooks?: CodegenHook[]
  indent?: string
}

export interface CodegenResult {
  code: string
}

export class Codegen {
  private parts: string[] = []
  private len = 0
  private hooks: CodegenHook[]
  private indentUnit: string
  private depth = 0

  constructor(options: CodegenOptions = {}) {
    this.hooks = options.hooks ?? []
    this.indentUnit = options.indent ?? '  '
  }

  build(node: AstNode | null | undefined): CodegenResult {
    if (node) {
      this.print(node)
    }
    return { code: this.parts.join('') }
  }

  private write(s: string): void {
    if (!s) return
    this.parts.push(s)
    this.len += s.length
  }

  private newline(): void {
    this.write('\n')
    if (this.depth > 0) {
      this.write(this.indentUnit.repeat(this.depth))
    }
  }

  private print(node: AstNode | null | undefined): void {
    if (!node) return
    const start = this.len
    for (const hook of this.hooks) hook.enter?.(node, start)
    const handler = HANDLERS[node.type]
    if (handler) {
      handler(this, node)
    } else {
      this.unknown(node)
    }
    const end = this.len
    for (const hook of this.hooks) hook.leave?.(node, end)
  }

  private unknown(node: AstNode): void {
    // Best-effort fallback: walk every child key so hooks still fire,
    // and emit a placeholder so the output is unambiguous.
    this.write(`/*<${node.type}>*/`)
    for (const key of Object.keys(node)) {
      if (key === 'type' || key === 'start' || key === 'end') continue
      const value = node[key]
      this.walkUnknown(value)
    }
    this.write(`/*</${node.type}>*/`)
  }

  private walkUnknown(value: unknown): void {
    if (Array.isArray(value)) {
      for (const item of value) this.walkUnknown(item)
    } else if (value && typeof value === 'object' && typeof (value as AstNode).type === 'string') {
      this.print(value as AstNode)
    }
  }

  // -- printing helpers exposed to handlers via closure -----------------------

  emit(s: string): void {
    this.write(s)
  }

  visit(node: AstNode | null | undefined): void {
    this.print(node)
  }

  joinList(nodes: ReadonlyArray<AstNode | null | undefined>, sep: string): void {
    nodes.forEach((node, index) => {
      if (index > 0) this.write(sep)
      this.print(node)
    })
  }

  block(body: ReadonlyArray<AstNode>): void {
    this.write('{')
    if (body.length === 0) {
      this.write('}')
      return
    }
    this.depth++
    for (const stmt of body) {
      this.newline()
      this.print(stmt)
    }
    this.depth--
    this.newline()
    this.write('}')
  }
}

type Handler = (g: Codegen, node: AstNode) => void

const HANDLERS: Record<string, Handler> = Object.create(null)

function register(types: string | string[], handler: Handler): void {
  const list = Array.isArray(types) ? types : [types]
  for (const t of list) HANDLERS[t] = handler
}

// -- top level --------------------------------------------------------------

register('Program', (g, node) => {
  const body = (node.body as AstNode[]) || []
  body.forEach((stmt, i) => {
    if (i > 0) g.emit('\n')
    g.visit(stmt)
  })
})

register('ExpressionStatement', (g, node) => {
  g.visit(node.expression as AstNode)
  g.emit(';')
})

register('BlockStatement', (g, node) => {
  g.block((node.body as AstNode[]) || [])
})

register('EmptyStatement', (g) => {
  g.emit(';')
})

register('ReturnStatement', (g, node) => {
  g.emit('return')
  if (node.argument) {
    g.emit(' ')
    g.visit(node.argument as AstNode)
  }
  g.emit(';')
})

register('IfStatement', (g, node) => {
  g.emit('if (')
  g.visit(node.test as AstNode)
  g.emit(') ')
  g.visit(node.consequent as AstNode)
  if (node.alternate) {
    g.emit(' else ')
    g.visit(node.alternate as AstNode)
  }
})

register('ThrowStatement', (g, node) => {
  g.emit('throw ')
  g.visit(node.argument as AstNode)
  g.emit(';')
})

register('BreakStatement', (g, node) => {
  g.emit('break')
  if (node.label) {
    g.emit(' ')
    g.visit(node.label as AstNode)
  }
  g.emit(';')
})

register('ContinueStatement', (g, node) => {
  g.emit('continue')
  if (node.label) {
    g.emit(' ')
    g.visit(node.label as AstNode)
  }
  g.emit(';')
})

// -- declarations -----------------------------------------------------------

register('VariableDeclaration', (g, node) => {
  g.emit(`${node.kind as string} `)
  g.joinList(node.declarations as AstNode[], ', ')
  g.emit(';')
})

register('VariableDeclarator', (g, node) => {
  g.visit(node.id as AstNode)
  if (node.init) {
    g.emit(' = ')
    g.visit(node.init as AstNode)
  }
})

register('FunctionDeclaration', (g, node) => functionDecl(g, node, true))
register('FunctionExpression', (g, node) => functionDecl(g, node, false))

function functionDecl(g: Codegen, node: AstNode, statement: boolean): void {
  if (node.async) g.emit('async ')
  g.emit('function')
  if (node.generator) g.emit('*')
  if (node.id) {
    g.emit(' ')
    g.visit(node.id as AstNode)
  }
  g.emit('(')
  g.joinList((node.params as AstNode[]) || [], ', ')
  g.emit(') ')
  g.visit(node.body as AstNode)
  if (!statement) return
}

register('ArrowFunctionExpression', (g, node) => {
  if (node.async) g.emit('async ')
  g.emit('(')
  g.joinList((node.params as AstNode[]) || [], ', ')
  g.emit(') => ')
  g.visit(node.body as AstNode)
})

// -- imports / exports -------------------------------------------------------

register('ImportDeclaration', (g, node) => {
  g.emit('import ')
  const specifiers = (node.specifiers as AstNode[]) || []
  if (specifiers.length === 0) {
    g.visit(node.source as AstNode)
    g.emit(';')
    return
  }
  let needComma = false
  const named: AstNode[] = []
  for (const spec of specifiers) {
    if (spec.type === 'ImportDefaultSpecifier') {
      if (needComma) g.emit(', ')
      g.visit(spec.local as AstNode)
      needComma = true
    } else if (spec.type === 'ImportNamespaceSpecifier') {
      if (needComma) g.emit(', ')
      g.emit('* as ')
      g.visit(spec.local as AstNode)
      needComma = true
    } else {
      named.push(spec)
    }
  }
  if (named.length > 0) {
    if (needComma) g.emit(', ')
    g.emit('{ ')
    g.joinList(named, ', ')
    g.emit(' }')
  }
  g.emit(' from ')
  g.visit(node.source as AstNode)
  g.emit(';')
})

register('ImportSpecifier', (g, node) => {
  g.visit(node.imported as AstNode)
  if ((node.imported as AstNode).name !== (node.local as AstNode).name) {
    g.emit(' as ')
    g.visit(node.local as AstNode)
  }
})

register('ExportNamedDeclaration', (g, node) => {
  g.emit('export ')
  if (node.declaration) {
    g.visit(node.declaration as AstNode)
    return
  }
  g.emit('{ ')
  g.joinList((node.specifiers as AstNode[]) || [], ', ')
  g.emit(' }')
  if (node.source) {
    g.emit(' from ')
    g.visit(node.source as AstNode)
  }
  g.emit(';')
})

register('ExportSpecifier', (g, node) => {
  g.visit(node.local as AstNode)
  if ((node.local as AstNode).name !== (node.exported as AstNode).name) {
    g.emit(' as ')
    g.visit(node.exported as AstNode)
  }
})

register('ExportDefaultDeclaration', (g, node) => {
  g.emit('export default ')
  g.visit(node.declaration as AstNode)
  const decl = node.declaration as AstNode
  if (decl.type !== 'FunctionDeclaration' && decl.type !== 'ClassDeclaration') {
    g.emit(';')
  }
})

register('ExportAllDeclaration', (g, node) => {
  g.emit('export * ')
  if (node.exported) {
    g.emit('as ')
    g.visit(node.exported as AstNode)
    g.emit(' ')
  }
  g.emit('from ')
  g.visit(node.source as AstNode)
  g.emit(';')
})

// -- expressions ------------------------------------------------------------

register('Identifier', (g, node) => {
  g.emit(node.name as string)
  if (node.typeAnnotation) g.visit(node.typeAnnotation as AstNode)
})

register('PrivateIdentifier', (g, node) => {
  g.emit(`#${node.name as string}`)
})

register('Literal', (g, node) => {
  if (typeof node.raw === 'string') {
    g.emit(node.raw)
    return
  }
  const value = node.value
  if (value === null) {
    g.emit('null')
  } else if (typeof value === 'string') {
    g.emit(JSON.stringify(value))
  } else if (typeof value === 'boolean' || typeof value === 'number') {
    g.emit(String(value))
  } else if (value instanceof RegExp) {
    g.emit(value.toString())
  } else {
    g.emit(JSON.stringify(value))
  }
})

register(['NumericLiteral', 'BigIntLiteral'], (g, node) => {
  g.emit((node.raw as string) ?? String(node.value))
})

register('StringLiteral', (g, node) => {
  g.emit((node.raw as string) ?? JSON.stringify(node.value))
})

register('BooleanLiteral', (g, node) => g.emit(node.value ? 'true' : 'false'))
register('NullLiteral', (g) => g.emit('null'))
register('RegExpLiteral', (g, node) => g.emit((node.raw as string) ?? '/(?:)/'))

register('TemplateLiteral', (g, node) => {
  const quasis = (node.quasis as AstNode[]) || []
  const expressions = (node.expressions as AstNode[]) || []
  g.emit('`')
  for (let i = 0; i < quasis.length; i++) {
    g.emit((quasis[i].value as { raw?: string } | undefined)?.raw ?? '')
    if (i < expressions.length) {
      g.emit('${')
      g.visit(expressions[i])
      g.emit('}')
    }
  }
  g.emit('`')
})

register('TaggedTemplateExpression', (g, node) => {
  g.visit(node.tag as AstNode)
  g.visit(node.quasi as AstNode)
})

register('BinaryExpression', binaryLike)
register('LogicalExpression', binaryLike)

function binaryLike(g: Codegen, node: AstNode): void {
  g.emit('(')
  g.visit(node.left as AstNode)
  g.emit(` ${node.operator as string} `)
  g.visit(node.right as AstNode)
  g.emit(')')
}

register('AssignmentExpression', (g, node) => {
  g.visit(node.left as AstNode)
  g.emit(` ${node.operator as string} `)
  g.visit(node.right as AstNode)
})

register('UnaryExpression', (g, node) => {
  const op = node.operator as string
  g.emit(node.prefix === false ? '' : op)
  if (/^[a-z]/.test(op)) g.emit(' ')
  g.visit(node.argument as AstNode)
  if (node.prefix === false) g.emit(op)
})

register('UpdateExpression', (g, node) => {
  if (node.prefix) g.emit(node.operator as string)
  g.visit(node.argument as AstNode)
  if (!node.prefix) g.emit(node.operator as string)
})

register('ConditionalExpression', (g, node) => {
  g.emit('(')
  g.visit(node.test as AstNode)
  g.emit(' ? ')
  g.visit(node.consequent as AstNode)
  g.emit(' : ')
  g.visit(node.alternate as AstNode)
  g.emit(')')
})

register('SequenceExpression', (g, node) => {
  g.emit('(')
  g.joinList(node.expressions as AstNode[], ', ')
  g.emit(')')
})

register('CallExpression', (g, node) => {
  g.visit(node.callee as AstNode)
  if (node.optional) g.emit('?.')
  g.emit('(')
  g.joinList((node.arguments as AstNode[]) || [], ', ')
  g.emit(')')
})

register('NewExpression', (g, node) => {
  g.emit('new ')
  g.visit(node.callee as AstNode)
  g.emit('(')
  g.joinList((node.arguments as AstNode[]) || [], ', ')
  g.emit(')')
})

register('MemberExpression', (g, node) => {
  g.visit(node.object as AstNode)
  if (node.computed) {
    g.emit(node.optional ? '?.[' : '[')
    g.visit(node.property as AstNode)
    g.emit(']')
  } else {
    g.emit(node.optional ? '?.' : '.')
    g.visit(node.property as AstNode)
  }
})

register('ChainExpression', (g, node) => g.visit(node.expression as AstNode))

register('SpreadElement', (g, node) => {
  g.emit('...')
  g.visit(node.argument as AstNode)
})

register('RestElement', (g, node) => {
  g.emit('...')
  g.visit(node.argument as AstNode)
})

register('AwaitExpression', (g, node) => {
  g.emit('await ')
  g.visit(node.argument as AstNode)
})

register('YieldExpression', (g, node) => {
  g.emit(node.delegate ? 'yield* ' : 'yield ')
  if (node.argument) g.visit(node.argument as AstNode)
})

register('ArrayExpression', (g, node) => {
  g.emit('[')
  const elements = (node.elements as Array<AstNode | null>) || []
  elements.forEach((el, i) => {
    if (i > 0) g.emit(', ')
    if (el) g.visit(el)
  })
  g.emit(']')
})

register('ObjectExpression', (g, node) => {
  const props = (node.properties as AstNode[]) || []
  if (props.length === 0) {
    g.emit('{}')
    return
  }
  g.emit('{ ')
  g.joinList(props, ', ')
  g.emit(' }')
})

register('Property', (g, node) => {
  if (node.shorthand) {
    g.visit(node.value as AstNode)
    return
  }
  if (node.computed) {
    g.emit('[')
    g.visit(node.key as AstNode)
    g.emit(']')
  } else {
    g.visit(node.key as AstNode)
  }
  if (node.kind === 'get' || node.kind === 'set') {
    // accessor — print method-style
    g.emit(' ')
    g.visit(node.value as AstNode)
    return
  }
  g.emit(': ')
  g.visit(node.value as AstNode)
})

register('AssignmentPattern', (g, node) => {
  g.visit(node.left as AstNode)
  g.emit(' = ')
  g.visit(node.right as AstNode)
})

register('ArrayPattern', (g, node) => {
  g.emit('[')
  const elements = (node.elements as Array<AstNode | null>) || []
  elements.forEach((el, i) => {
    if (i > 0) g.emit(', ')
    if (el) g.visit(el)
  })
  g.emit(']')
})

register('ObjectPattern', (g, node) => {
  g.emit('{ ')
  g.joinList((node.properties as AstNode[]) || [], ', ')
  g.emit(' }')
})

// -- JSX --------------------------------------------------------------------

register('JSXFragment', (g, node) => {
  g.visit(node.openingFragment as AstNode)
  for (const child of (node.children as AstNode[]) || []) g.visit(child)
  g.visit(node.closingFragment as AstNode)
})

register('JSXOpeningFragment', (g) => g.emit('<>'))
register('JSXClosingFragment', (g) => g.emit('</>'))

register('JSXElement', (g, node) => {
  g.visit(node.openingElement as AstNode)
  for (const child of (node.children as AstNode[]) || []) g.visit(child)
  if (node.closingElement) g.visit(node.closingElement as AstNode)
})

register('JSXOpeningElement', (g, node) => {
  g.emit('<')
  g.visit(node.name as AstNode)
  for (const attr of (node.attributes as AstNode[]) || []) {
    g.emit(' ')
    g.visit(attr)
  }
  g.emit(node.selfClosing ? ' />' : '>')
})

register('JSXClosingElement', (g, node) => {
  const name = node.name as AstNode | null
  if (!name || (name.type === 'JSXIdentifier' && name.name === '')) {
    g.emit('</>')
    return
  }
  g.emit('</')
  g.visit(name)
  g.emit('>')
})

register('JSXIdentifier', (g, node) => g.emit((node.name as string) ?? ''))

register('JSXMemberExpression', (g, node) => {
  g.visit(node.object as AstNode)
  g.emit('.')
  g.visit(node.property as AstNode)
})

register('JSXNamespacedName', (g, node) => {
  g.visit(node.namespace as AstNode)
  g.emit(':')
  g.visit(node.name as AstNode)
})

register('JSXAttribute', (g, node) => {
  g.visit(node.name as AstNode)
  if (node.value) {
    g.emit('=')
    g.visit(node.value as AstNode)
  }
})

register('JSXSpreadAttribute', (g, node) => {
  g.emit('{...')
  g.visit(node.argument as AstNode)
  g.emit('}')
})

register('JSXExpressionContainer', (g, node) => {
  g.emit('{')
  g.visit(node.expression as AstNode)
  g.emit('}')
})

register('JSXEmptyExpression', () => {
  /* nothing */
})

register('JSXText', (g, node) => g.emit((node.raw as string) ?? (node.value as string) ?? ''))

// -- TypeScript (pragmatic subset) ------------------------------------------

register('TSTypeAnnotation', (g, node) => {
  g.emit(': ')
  g.visit(node.typeAnnotation as AstNode)
})

const TS_KEYWORDS: Record<string, string> = {
  TSAnyKeyword: 'any',
  TSBigIntKeyword: 'bigint',
  TSBooleanKeyword: 'boolean',
  TSIntrinsicKeyword: 'intrinsic',
  TSNeverKeyword: 'never',
  TSNullKeyword: 'null',
  TSNumberKeyword: 'number',
  TSObjectKeyword: 'object',
  TSStringKeyword: 'string',
  TSSymbolKeyword: 'symbol',
  TSThisType: 'this',
  TSUndefinedKeyword: 'undefined',
  TSUnknownKeyword: 'unknown',
  TSVoidKeyword: 'void',
}

for (const [type, text] of Object.entries(TS_KEYWORDS)) {
  register(type, (g) => g.emit(text))
}

register('TSTypeReference', (g, node) => {
  g.visit(node.typeName as AstNode)
  if (node.typeArguments) g.visit(node.typeArguments as AstNode)
})

register('TSQualifiedName', (g, node) => {
  g.visit(node.left as AstNode)
  g.emit('.')
  g.visit(node.right as AstNode)
})

register('TSTypeParameterInstantiation', (g, node) => {
  g.emit('<')
  g.joinList((node.params as AstNode[]) || [], ', ')
  g.emit('>')
})

register('TSAsExpression', (g, node) => {
  g.visit(node.expression as AstNode)
  g.emit(' as ')
  g.visit(node.typeAnnotation as AstNode)
})

register('TSSatisfiesExpression', (g, node) => {
  g.visit(node.expression as AstNode)
  g.emit(' satisfies ')
  g.visit(node.typeAnnotation as AstNode)
})

register('TSNonNullExpression', (g, node) => {
  g.visit(node.expression as AstNode)
  g.emit('!')
})

register('TSTypeAssertion', (g, node) => {
  g.emit('<')
  g.visit(node.typeAnnotation as AstNode)
  g.emit('>')
  g.visit(node.expression as AstNode)
})
