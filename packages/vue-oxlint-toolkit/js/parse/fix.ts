import type { Ranged } from '@oxlint/plugins'
import type { LocationConvertor } from '../location'
import type { NativeNode } from './types'

const RANGE_KEYS = new Set(['range', 'start', 'end', 'loc', 'parent'])
const ABSENT_WHEN_NULL_KEYS = new Set([
  'accessibility',
  'directive',
  'phase',
  'returnType',
  'typeAnnotation',
  'typeArguments',
  'typeParameters',
])

export function fixNativeNode<T>(value: T, convertor: LocationConvertor): T {
  if (Array.isArray(value)) {
    return value.map((item) => fixNativeNode(item, convertor)) as T
  }

  if (!isRecord(value)) {
    return value
  }

  if (typeof value.type !== 'string') {
    return fixPlainObject(value, convertor) as T
  }

  const fixed = hasRange(value)
    ? withoutStartEnd(convertor.fix(value as Ranged) as unknown as NativeNode)
    : ({ ...value } as NativeNode)

  for (const [key, child] of Object.entries(value)) {
    if (!RANGE_KEYS.has(key)) {
      fixed[key] = fixNativeNode(child, convertor)
    }
  }

  for (const key of ABSENT_WHEN_NULL_KEYS) {
    if (fixed[key] === null) {
      fixed[key] = undefined
    }
  }

  if (fixed.type === 'ParenthesizedExpression') {
    return fixed.expression as T
  }

  if (fixed.type === 'Literal' && fixed.value === null && typeof fixed.bigint === 'string') {
    fixed.value = BigInt(fixed.bigint)
  }

  return fixed as T
}

export function fixNativeRange(range: [number, number], convertor: LocationConvertor) {
  return convertor.range(range)
}

function fixPlainObject(value: Record<string, any>, convertor: LocationConvertor) {
  const fixed: Record<string, any> = {}

  for (const [key, child] of Object.entries(value)) {
    fixed[key] = fixNativeNode(child, convertor)
  }

  return fixed
}

function hasRange(value: Record<string, any>): value is Ranged {
  return (
    Array.isArray(value.range) || (typeof value.start === 'number' && typeof value.end === 'number')
  )
}

function withoutStartEnd<T extends NativeNode>(node: T): T {
  const { start: _start, end: _end, ...rest } = node
  return rest as T
}

function isRecord(value: unknown): value is Record<string, any> {
  return typeof value === 'object' && value !== null
}
