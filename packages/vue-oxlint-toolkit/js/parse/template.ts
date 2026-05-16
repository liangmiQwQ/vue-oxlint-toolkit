import type { LocationConvertor } from '../location'
import type { NativeNode, NativeSfc, ScriptBlock } from './types'
import { fixNativeNode } from './fix'

const HTML_NS = 'http://www.w3.org/1999/xhtml'
const SVG_NS = 'http://www.w3.org/2000/svg'
const MATH_NS = 'http://www.w3.org/1998/Math/MathML'

export function rebuildTemplateBody(sfc: NativeSfc, convertor: LocationConvertor) {
  const children = fixNativeNode(sfc.children, convertor) as NativeNode[]
  const prepared = children.map((child) => prepareTemplateNode(child, HTML_NS))

  return prepared.find((node) => node.type === 'VElement' && node.rawName === 'template')
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

function prepareTemplateNode(node: NativeNode, namespace: string): NativeNode {
  if (node.type === 'VElement') {
    const childNamespace = elementNamespace(node, namespace)
    node.namespace = childNamespace
    node.variables ??= []
    node.startTag = prepareTemplateNode(node.startTag, childNamespace)
    node.endTag = node.endTag ? prepareTemplateNode(node.endTag, childNamespace) : null
    node.children = node.children.map((child: NativeNode) =>
      prepareTemplateNode(child, childNamespace),
    )
  } else if (node.type === 'VStartTag') {
    node.attributes = node.attributes.map((attribute: NativeNode) => prepareAttribute(attribute))
  }

  return node
}

function prepareAttribute(attribute: NativeNode): NativeNode {
  if (attribute.directive && attribute.key?.name?.name === 'bind' && attribute.value === null) {
    const argument = attribute.key.argument
    if (argument?.type === 'VIdentifier') {
      attribute.value = {
        type: 'VExpressionContainer',
        range: [...argument.range],
        start: argument.range[0],
        end: argument.range[1],
        expression: {
          type: 'Identifier',
          range: [...argument.range],
          start: argument.range[0],
          end: argument.range[1],
          name: camelize(argument.rawName),
        },
        references: [],
      }
    }
  }

  return attribute
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

function camelize(value: string) {
  return value.replace(/-([a-z])/gu, (_, char: string) => char.toUpperCase())
}
