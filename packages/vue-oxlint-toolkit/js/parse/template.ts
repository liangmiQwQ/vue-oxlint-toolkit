import type { LocationConvertor } from '../location'
import type { Token, VText } from '../ast'
import type { NativeNode, NativeSfc, ScriptBlock } from './types'
import { fixNativeNode } from './fix'

const HTML_NS = 'http://www.w3.org/1999/xhtml'
const SVG_NS = 'http://www.w3.org/2000/svg'
const MATH_NS = 'http://www.w3.org/1998/Math/MathML'

export function rebuildTemplate(sfc: NativeSfc, source: string, convertor: LocationConvertor) {
  const children = fixNativeNode(sfc.children, convertor) as NativeNode[]
  const comments = templateComments(sfc, source, convertor)
  const prepared = children.map((child) => prepareTemplateNode(child, HTML_NS, convertor, comments))
  const templateBody = prepared.find(
    (node) => node.type === 'VElement' && node.rawName === 'template',
  )
  if (!templateBody) {
    return { templateBody: undefined, fragment: undefined, tokens: [], comments }
  }

  const tokens = fixNativeNode(sfc.templateTokens, convertor) as Token[]
  const fragment = {
    type: 'VDocumentFragment',
    range: [...sfc.range],
    loc: convertor.fix({ range: sfc.range }).loc,
    parent: null,
    children: prepared,
    tokens,
    comments,
  }

  templateBody.parent = fragment

  return { templateBody, fragment, tokens, comments }
}

export function collectScriptBlocks(sfc: NativeSfc, convertor: LocationConvertor): ScriptBlock[] {
  const children = fixNativeNode(sfc.children, convertor) as NativeNode[]

  return children
    .filter((node) => node.type === 'VElement' && node.name === 'script' && node.endTag)
    .map((node) => ({
      bodyStart: node.startTag.range[1],
      bodyEnd: node.endTag.range[0],
      lang: getAttributeValue(node, 'lang'),
      setup: hasAttribute(node, 'setup'),
    }))
}

function prepareTemplateNode(
  node: NativeNode,
  namespace: string,
  convertor: LocationConvertor,
  comments: Token[],
): NativeNode {
  if (node.type === 'VElement') {
    const childNamespace = elementNamespace(node, namespace)
    node.namespace = childNamespace
    node.variables ??= []
    if (node.name === 'style') {
      node.style = true
    }
    node.startTag = prepareTemplateNode(node.startTag, childNamespace, convertor, comments)
    node.endTag = node.endTag
      ? prepareTemplateNode(node.endTag, childNamespace, convertor, comments)
      : null
    node.children = mergeTextChildren(
      node.children.map((child: NativeNode) =>
        prepareTemplateNode(child, childNamespace, convertor, comments),
      ),
      convertor,
      comments,
    )
  }

  return node
}

function elementNamespace(node: NativeNode, current: string) {
  if (node.name === 'svg') {
    return SVG_NS
  }
  if (node.name === 'math') {
    return MATH_NS
  }
  return current
}

function hasAttribute(node: NativeNode, name: string) {
  return node.startTag.attributes.some((attribute: NativeNode) => attribute.key?.name === name)
}

function getAttributeValue(node: NativeNode, name: string): string | undefined {
  const attribute = node.startTag.attributes.find(
    (attribute: NativeNode) => attribute.key?.name === name,
  )
  return attribute?.value?.value
}

function mergeTextChildren(
  children: NativeNode[],
  convertor: LocationConvertor,
  comments: Token[],
): NativeNode[] {
  const merged: NativeNode[] = []

  for (const child of children) {
    const previous = merged.at(-1)
    if (
      previous?.type === 'VText' &&
      child.type === 'VText' &&
      previous.range &&
      child.range &&
      !hasCommentBetween(previous.range, child.range, comments)
    ) {
      const previousText = previous as VText
      previousText.range = [previousText.range[0], child.range[1]]
      previousText.loc = convertor.fix({ range: previousText.range }).loc
      previousText.value += child.value
    } else {
      merged.push(child)
    }
  }

  return merged
}

function templateComments(sfc: NativeSfc, source: string, convertor: LocationConvertor): Token[] {
  const comments = [
    ...(fixNativeNode(sfc.template_comments, convertor) as Token[]),
    ...(fixNativeNode(sfc.script_comments, convertor) as Token[])
      .filter((comment) => isTemplateComment(comment, sfc.children))
      .map((comment) => expandDelimitedComment(comment, source, convertor)),
  ]

  return dedupeTokens(comments).sort((a, b) => a.range[0] - b.range[0])
}

function isTemplateComment(comment: Token, children: NativeNode[]) {
  return children.some(
    (node) =>
      node.type === 'VElement' &&
      node.rawName === 'template' &&
      node.range &&
      comment.range[0] >= node.range[0] &&
      comment.range[1] <= node.range[1],
  )
}

function expandDelimitedComment(comment: Token, source: string, convertor: LocationConvertor) {
  if (comment.type !== 'Block') {
    return comment
  }

  const start = source.slice(comment.range[0] - 2, comment.range[0])
  const end = source.slice(comment.range[1], comment.range[1] + 2)
  if (start !== '/*' || end !== '*/') {
    return comment
  }

  const range: [number, number] = [comment.range[0] - 2, comment.range[1] + 2]
  return {
    ...comment,
    range,
    loc: convertor.fix({ range }).loc,
  }
}

function hasCommentBetween(left: [number, number], right: [number, number], comments: Token[]) {
  return comments.some((comment) => comment.range[0] >= left[1] && comment.range[1] <= right[0])
}

function dedupeTokens(tokens: Token[]) {
  const seen = new Set<string>()
  return tokens.filter((token) => {
    const key = `${token.type}:${token.range[0]}:${token.range[1]}`
    if (seen.has(key)) {
      return false
    }
    seen.add(key)
    return true
  })
}
