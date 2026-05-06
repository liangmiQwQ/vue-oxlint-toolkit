import { AST } from 'vue-eslint-parser'
import type { Locator } from '../locator'
import { getArray, getRange, isAstNode } from './nodes'
import type { AstNode, Reference, Variable } from './nodes'

const visitorKeys = new Proxy(
  {
    ...AST.KEYS,
    VPureScript: [],
  } as Record<string, readonly string[]>,
  {
    get(keys, type) {
      if (typeof type !== 'string') {
        return Reflect.get(keys, type)
      }

      return keys[type] ?? []
    },
  },
)

export function attachAstMetadata(root: AstNode, locator: Locator, rootParent?: AstNode | null) {
  const hasRootParent = rootParent !== undefined

  AST.traverseNodes(root as AST.Node, {
    visitorKeys,
    enterNode(node, parent) {
      const astNode = node as AstNode
      const actualParent =
        astNode === root && hasRootParent ? rootParent : (parent as AstNode | null)
      attachNodeMetadata(astNode, locator, actualParent, astNode !== root || hasRootParent)
      attachReferenceLikeMetadata(astNode, locator)
    },
    leaveNode() {},
  })
}

export function attachListMetadata(values: unknown[], locator: Locator) {
  for (const value of values) {
    if (isAstNode(value)) {
      attachLocationMetadata(value, locator)
    }
  }
}

function attachReferenceLikeMetadata(node: AstNode, locator: Locator) {
  for (const reference of getArray<Reference>(node.references)) {
    attachDetachedIdMetadata(reference.id, node, locator)
  }
  for (const variable of getArray<Variable>(node.variables)) {
    attachDetachedIdMetadata(variable.id, node, locator)
  }
}

function attachDetachedIdMetadata(id: unknown, owner: AstNode, locator: Locator) {
  if (!isAstNode(id)) {
    return
  }

  if (!('parent' in id)) {
    attachParentMetadata(id, owner)
  }
  attachLocationMetadata(id, locator)
}

function attachNodeMetadata(
  node: AstNode,
  locator: Locator,
  parent: AstNode | null | undefined,
  shouldAttachParent: boolean,
) {
  if (shouldAttachParent) {
    attachParentMetadata(node, parent)
  }

  attachLocationMetadata(node, locator)
}

function attachParentMetadata(node: AstNode, parent: AstNode | null | undefined) {
  Object.defineProperty(node, 'parent', {
    configurable: true,
    enumerable: true,
    value: parent,
    writable: true,
  })
}

function attachLocationMetadata(value: AstNode, locator: Locator) {
  const range = getRange(value)
  if (!range) {
    return
  }

  value.range ??= range
  const [start, end] = value.range

  Object.defineProperty(value, 'loc', {
    configurable: true,
    enumerable: true,
    get() {
      return {
        start: locator(start),
        end: locator(end),
      }
    },
  })
}
