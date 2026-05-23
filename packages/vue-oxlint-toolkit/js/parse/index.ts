import type { LocationConvertor } from '../location'
import type { NativeNode, NativeSfc } from './types'
import { fixNativeNode } from './fix'
import { rebuildTemplate } from './template'
import { visitorKeys } from './visitorKeys'

export function rebuildProgram(sfc: NativeSfc, convertor: LocationConvertor) {
  const children = fixNativeNode(sfc.children, convertor) as NativeNode[]
  const body = scriptBody(children)
  const template = rebuildTemplate(sfc, convertor, children)
  const blocks = scriptBlocks(template.fragment.children)
  const comments = (fixNativeNode(sfc.script_comments, convertor) as NativeNode[]).filter(
    (comment) =>
      blocks.some(
        (block) => comment.range![0] >= block.bodyStart && comment.range![1] <= block.bodyEnd,
      ),
  )
  const range = programRange(body, comments, blocks)
  const program: any = {
    type: 'Program',
    body,
    sourceType: sfc.source_type === 'script' ? 'script' : 'module',
    comments,
    tokens: fixNativeNode(sfc.scriptTokens, convertor),
    range,
    loc: convertor.fix({ range }).loc,
  }

  program.templateBody = template.templateBody

  visit(program, null)
  if (template.fragment) {
    visit(template.fragment, null)
  }

  return program
}

function scriptBody(children: NativeNode[]) {
  const body: NativeNode[] = []
  collectScriptBody(children, body)
  return body
}

function collectScriptBody(nodes: NativeNode[], body: NativeNode[]) {
  for (const node of nodes) {
    if (node.type === 'VPureScript') {
      body.push(...node.body)
    } else if (Array.isArray(node.children)) {
      collectScriptBody(node.children, body)
    }
  }
}

function programRange(
  body: NativeNode[],
  comments: NativeNode[],
  blocks: ReturnType<typeof scriptBlocks>,
) {
  if (body.length > 0) {
    const blocksWithBody = blocks.filter((block) =>
      body.some(
        (node) => node.range && node.range[0] >= block.bodyStart && node.range[1] <= block.bodyEnd,
      ),
    )
    const end =
      blocksWithBody.length === 1 ? blocksWithBody[0].bodyEnd : body[body.length - 1].range![1]

    return [body[0].range![0], end] as [number, number]
  }

  if (comments.length > 0) {
    return [commentOnlyProgramStart(comments), 0] as [number, number]
  }

  const fallback = blocks[0]?.bodyEnd ?? 0
  return [fallback, fallback] as [number, number]
}

function commentOnlyProgramStart(comments: NativeNode[]) {
  const block = comments.find(
    (comment) =>
      comment.type === 'Block' && typeof comment.value === 'string' && comment.value.includes('\n'),
  )
  if (!block) {
    return comments[0].range![0]
  }

  const lines = block.value.split(/\r\n|[\r\n\u2028\u2029]/u)
  let offset = block.range![0] + 2
  let longest = { start: offset, value: '' }
  for (const line of lines) {
    if (line.length > longest.value.length) {
      longest = { start: offset, value: line }
    }
    offset += line.length + 1
  }

  const lastSpace = longest.value.lastIndexOf(' ')
  return longest.start + (lastSpace === -1 ? 0 : lastSpace)
}

function scriptBlocks(children: NativeNode[]) {
  return children
    .filter((node) => node.type === 'VElement' && node.name === 'script' && node.endTag)
    .map((node) => ({
      bodyStart: node.startTag.range[1],
      bodyEnd: node.endTag.range[0],
    }))
}

function visit(node: unknown, parent: NativeNode | null) {
  if (!isNode(node)) {
    return
  }

  node.parent = parent

  for (const key of childKeys(node)) {
    const child = node[key]
    if (Array.isArray(child)) {
      for (const item of child) {
        visit(item, node)
      }
    } else {
      visit(child, node)
    }
  }

  reconnectReferences(node)
  reconnectVariables(node)
}

function isNode(value: unknown): value is Record<string, any> & { type: string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { type?: unknown }).type === 'string'
  )
}

function reconnectReferences(node: Record<string, any>) {
  if (node.type !== 'VExpressionContainer' || !Array.isArray(node.references)) {
    return
  }

  const identifiers = new Map<string, NativeNode>()
  collectIdentifiers(node.expression, identifiers)
  for (const reference of node.references) {
    const id = reference.id
    if (id?.type !== 'Identifier' || !id.range) {
      continue
    }
    reference.id = identifiers.get(identifierKey(id)) ?? id
  }
}

function reconnectVariables(node: Record<string, any>) {
  if (node.type !== 'VElement' || !Array.isArray(node.variables) || node.variables.length === 0) {
    return
  }

  const identifiers = new Map<string, NativeNode>()
  collectIdentifiers(node.startTag, identifiers)
  for (const variable of node.variables) {
    const id = variable.id
    if (id?.type !== 'Identifier' || !id.range) {
      continue
    }
    variable.id = identifiers.get(identifierKey(id)) ?? id
  }
}

function collectIdentifiers(node: unknown, identifiers: Map<string, NativeNode>) {
  if (!isNode(node)) {
    return
  }

  if (node.type === 'Identifier' && node.range) {
    identifiers.set(identifierKey(node), node)
  }

  for (const key of childKeys(node)) {
    const child = node[key]
    if (Array.isArray(child)) {
      for (const item of child) {
        collectIdentifiers(item, identifiers)
      }
    } else {
      collectIdentifiers(child, identifiers)
    }
  }
}

function identifierKey(node: NativeNode) {
  return `${node.name}:${node.range![0]}:${node.range![1]}`
}

function childKeys(node: Record<string, any>) {
  const keys = visitorKeys[node.type as keyof typeof visitorKeys]
  const fallbackKeys = Object.keys(node).filter((key) => {
    if (RANGE_OR_META_KEYS.has(key)) {
      return false
    }
    const value = node[key]
    return isNode(value) || (Array.isArray(value) && value.some(isNode))
  })
  if (!keys) {
    return fallbackKeys
  }

  return [...new Set([...keys, ...fallbackKeys])]
}

const RANGE_OR_META_KEYS = new Set(['comments', 'loc', 'parent', 'range', 'tokens'])
