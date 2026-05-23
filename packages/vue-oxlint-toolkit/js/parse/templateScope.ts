import type { NativeNode } from './types'
import { visitorKeys } from './visitorKeys'

interface TemplateVariable {
  id: NativeNode
  kind: 'v-for' | 'scope' | 'generic'
  references: TemplateReference[]
}

interface TemplateReference {
  id: NativeNode
  mode: 'r'
  variable: TemplateVariable | null
  isValueReference: true
  isTypeReference: false
}

export function applyTemplateScope(root: NativeNode | null | undefined) {
  if (!root) {
    return
  }

  for (const element of collectElements(root)) {
    element.variables = collectElementVariables(element)
  }

  for (const container of collectExpressionContainers(root)) {
    container.references = collectContainerReferences(container)
    resolveReferences(container)
  }
}

function collectElements(root: NativeNode) {
  const elements: NativeNode[] = []
  walk(root, (node) => {
    if (node.type === 'VElement') {
      elements.push(node)
    }
  })
  return elements
}

function collectExpressionContainers(root: NativeNode) {
  const containers: NativeNode[] = []
  walk(root, (node) => {
    if (node.type === 'VExpressionContainer') {
      containers.push(node)
    }
  })
  return containers
}

function collectElementVariables(element: NativeNode): TemplateVariable[] {
  const variables: TemplateVariable[] = []
  const attributes = element.startTag?.attributes
  if (!Array.isArray(attributes)) {
    return variables
  }

  for (const attribute of attributes) {
    const expression = attribute.value?.expression
    if (expression?.type === 'VForExpression') {
      variables.push(...collectBindingVariables(expression.left, 'v-for'))
    } else if (expression?.type === 'VSlotScopeExpression') {
      variables.push(...collectBindingVariables(expression.params, 'scope'))
    }
  }

  return variables
}

function collectBindingVariables(value: unknown, kind: TemplateVariable['kind']) {
  const variables: TemplateVariable[] = []
  collectBindingIdentifiers(value, (id) => {
    variables.push({ id, kind, references: [] })
  })
  return variables
}

function collectContainerReferences(container: NativeNode): TemplateReference[] {
  const expression = container.expression
  if (!isNode(expression)) {
    return []
  }

  if (expression.type === 'VForExpression') {
    return collectReferences(expression.right)
  }
  if (expression.type === 'VSlotScopeExpression' || expression.type === 'VGenericExpression') {
    return []
  }
  if (expression.type === 'VOnExpression') {
    return collectReferences(expression.body)
  }

  return collectReferences(expression)
}

function collectReferences(value: unknown) {
  const references: TemplateReference[] = []
  walkExpression(value, null, null, (node, parent, key) => {
    if (node.type === 'Identifier' && isReferenceIdentifier(node, parent, key)) {
      references.push({
        id: node,
        mode: 'r',
        variable: null,
        isValueReference: true,
        isTypeReference: false,
      })
    }
  })
  return references
}

function resolveReferences(container: NativeNode) {
  for (const reference of container.references as TemplateReference[]) {
    let element = nearestElement(container.parent)
    while (element) {
      const variable = (element.variables as TemplateVariable[]).find(
        (variable) => variable.id.name === reference.id.name,
      )
      if (variable) {
        reference.variable = variable
        variable.references.push(reference)
        break
      }
      element = nearestElement(element.parent)
    }
  }
}

function nearestElement(node: NativeNode | null | undefined): NativeNode | null {
  let current = node
  while (current) {
    if (current.type === 'VElement') {
      return current
    }
    current = current.parent
  }
  return null
}

function collectBindingIdentifiers(value: unknown, onIdentifier: (node: NativeNode) => void) {
  walkExpression(value, null, null, (node, parent, key) => {
    if (node.type === 'Identifier' && isBindingIdentifier(node, parent, key)) {
      onIdentifier(node)
    }
  })
}

function isReferenceIdentifier(
  node: NativeNode,
  parent: NativeNode | null,
  key: string | null,
): boolean {
  if (!parent) {
    return true
  }
  if (isBindingIdentifier(node, parent, key)) {
    return false
  }
  if (
    (parent.type === 'Property' || parent.type === 'PropertyDefinition') &&
    key === 'key' &&
    !parent.computed
  ) {
    return false
  }
  if (parent.type === 'MemberExpression' && key === 'property' && !parent.computed) {
    return false
  }
  if (parent.type === 'MethodDefinition' && key === 'key' && !parent.computed) {
    return false
  }
  if (parent.type === 'LabeledStatement' && key === 'label') {
    return false
  }
  if (parent.type === 'BreakStatement' || parent.type === 'ContinueStatement') {
    return false
  }
  return true
}

function isBindingIdentifier(
  node: NativeNode,
  parent: NativeNode | null,
  key: string | null,
): boolean {
  if (!parent) {
    return false
  }
  if (key === 'id') {
    return (
      parent.type === 'VariableDeclarator' ||
      parent.type === 'FunctionDeclaration' ||
      parent.type === 'FunctionExpression' ||
      parent.type === 'ClassDeclaration' ||
      parent.type === 'ClassExpression' ||
      parent.type === 'CatchClause'
    )
  }
  if (key === 'params') {
    return true
  }
  if (parent.type === 'Property' && key === 'value' && parent.shorthand) {
    return false
  }
  return isBindingPatternParent(parent, key)
}

function isBindingPatternParent(parent: NativeNode, key: string | null) {
  return (
    (parent.type === 'ArrayPattern' && key === 'elements') ||
    (parent.type === 'ObjectPattern' && key === 'properties') ||
    (parent.type === 'Property' && key === 'value') ||
    (parent.type === 'AssignmentPattern' && key === 'left') ||
    (parent.type === 'RestElement' && key === 'argument')
  )
}

function walk(root: NativeNode, visitor: (node: NativeNode) => void) {
  visitor(root)
  for (const key of childKeys(root)) {
    const child = root[key]
    if (Array.isArray(child)) {
      for (const item of child) {
        if (isNode(item)) {
          walk(item, visitor)
        }
      }
    } else if (isNode(child)) {
      walk(child, visitor)
    }
  }
}

function walkExpression(
  value: unknown,
  parent: NativeNode | null,
  key: string | null,
  visitor: (node: NativeNode, parent: NativeNode | null, key: string | null) => void,
) {
  if (Array.isArray(value)) {
    for (const item of value) {
      walkExpression(item, parent, key, visitor)
    }
    return
  }
  if (!isNode(value)) {
    return
  }

  visitor(value, parent, key)
  for (const childKey of childKeys(value)) {
    walkExpression(value[childKey], value, childKey, visitor)
  }
}

function childKeys(node: NativeNode) {
  const keys = visitorKeys[node.type as keyof typeof visitorKeys]
  if (keys) {
    return keys
  }
  return Object.keys(node).filter((key) => {
    if (META_KEYS.has(key)) {
      return false
    }
    const value = node[key]
    return isNode(value) || (Array.isArray(value) && value.some(isNode))
  })
}

function isNode(value: unknown): value is NativeNode {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { type?: unknown }).type === 'string'
  )
}

const META_KEYS = new Set([
  'comments',
  'loc',
  'parent',
  'range',
  'references',
  'tokens',
  'variables',
])
