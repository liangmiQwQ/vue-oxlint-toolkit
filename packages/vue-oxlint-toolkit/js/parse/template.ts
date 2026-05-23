import type { LocationConvertor } from '../location'
import type { Token, VText } from '../ast'
import type { NativeNode, NativeSfc } from './types'
import { fixNativeNode } from './fix'

const HTML_NS = 'http://www.w3.org/1999/xhtml'
const SVG_NS = 'http://www.w3.org/2000/svg'
const MATH_NS = 'http://www.w3.org/1998/Math/MathML'

export function rebuildTemplate(
  sfc: NativeSfc,
  convertor: LocationConvertor,
  children = fixNativeNode(sfc.children, convertor) as NativeNode[],
) {
  const tokens = fixNativeNode(sfc.templateTokens, convertor) as Token[]
  const comments = templateComments(sfc, children, convertor)
  const prepared = mergeTextChildren(
    children
      .filter((child) => child.type !== 'VPureScript')
      .map((child) => prepareTemplateNode(child, HTML_NS, convertor, comments)),
    convertor,
    comments,
  )
  const fragment = {
    type: 'VDocumentFragment',
    range: convertor.range(sfc.range),
    loc: convertor.fix({ range: sfc.range }).loc,
    parent: null,
    children: prepared,
    tokens,
    comments,
  }
  const templateBody = findTemplateBody(prepared)
  if (templateBody) {
    templateBody.tokens = tokens
    templateBody.comments = comments
  }

  return {
    fragment,
    templateBody,
  }
}

function findTemplateBody(children: NativeNode[]) {
  return children.find((node) => node.type === 'VElement' && node.rawName === 'template')
}

function prepareTemplateNode(
  node: NativeNode,
  namespace: string,
  convertor: LocationConvertor,
  comments: Token[],
): NativeNode {
  if (node.type !== 'VElement') {
    return node
  }

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
    node.children
      .filter((child: NativeNode) => child.type !== 'VPureScript')
      .map((child: NativeNode) => prepareTemplateNode(child, childNamespace, convertor, comments)),
    convertor,
    comments,
  )

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

function templateComments(sfc: NativeSfc, children: NativeNode[], convertor: LocationConvertor) {
  const scriptBlocks = children
    .filter((node) => node.type === 'VElement' && node.name === 'script' && node.endTag)
    .map((node) => [node.startTag.range[1], node.endTag.range[0]] as [number, number])
  const templateComments = fixNativeNode(sfc.template_comments, convertor) as Token[]
  const expressionComments = (fixNativeNode(sfc.script_comments, convertor) as Token[]).filter(
    (comment) =>
      !scriptBlocks.some((block) => comment.range[0] >= block[0] && comment.range[1] <= block[1]),
  )

  return dedupeTokens([...templateComments, ...expressionComments]).sort(
    (a, b) => a.range[0] - b.range[0],
  )
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
