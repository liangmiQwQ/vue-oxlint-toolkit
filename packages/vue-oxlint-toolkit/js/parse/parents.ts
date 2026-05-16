import { visitorKeys } from './visitorKeys'

export function injectParents(root: any) {
  visit(root, null)
}

function visit(node: any, parent: any) {
  if (!isNode(node)) {
    return
  }

  node.parent = parent

  const keys = visitorKeys[node.type as keyof typeof visitorKeys]
  if (!keys) {
    return
  }

  for (const key of keys) {
    const child = node[key]
    if (Array.isArray(child)) {
      for (const item of child) {
        visit(item, node)
      }
    } else {
      visit(child, node)
    }
  }
}

function isNode(value: unknown): value is Record<string, any> & { type: string } {
  return (
    typeof value === 'object' &&
    value !== null &&
    typeof (value as { type?: unknown }).type === 'string'
  )
}
