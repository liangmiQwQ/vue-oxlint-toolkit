import type { LocationConvertor } from '../location'
import type { Token } from '../ast'
import type { NativeNode, NativeSfc } from './types'
import { fixNativeNode } from './fix'

export function rebuildTemplate(
  sfc: NativeSfc,
  convertor: LocationConvertor,
  children = fixNativeNode(sfc.children, convertor) as NativeNode[],
) {
  const tokens = fixNativeNode(sfc.templateTokens, convertor) as Token[]
  const comments = templateComments(sfc, children, convertor)
  const prepared = children.filter((child) => child.type !== 'VPureScript')
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
