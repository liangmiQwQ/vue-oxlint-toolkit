import type { NativeRange } from '../bindings'
import type { Locator } from './types'

export function createLocator(source: string): Locator {
  const lineStarts = [{ byte: 0, index: 0 }]
  const byteToIndex = new Map<number, number>([[0, 0]])
  let byteOffset = 0

  for (let index = 0; index < source.length; ) {
    const codePoint = source.codePointAt(index)
    if (codePoint === undefined) {
      break
    }

    const codeUnitLength = codePoint > 0xffff ? 2 : 1
    byteOffset += utf8ByteLength(codePoint)
    index += codeUnitLength
    byteToIndex.set(byteOffset, index)

    if (codePoint === 10) {
      lineStarts.push({ byte: byteOffset, index })
    }
  }

  const toIndex = (offset: number) => {
    const index = byteToIndex.get(offset)

    if (index === undefined) {
      throw new RangeError(`Offset ${offset} is not on a UTF-8 character boundary.`)
    }

    return index
  }

  const locator = ((offset: number) => {
    let low = 0
    let high = lineStarts.length - 1

    while (low <= high) {
      const mid = (low + high) >> 1

      if (lineStarts[mid].byte <= offset) {
        low = mid + 1
      } else {
        high = mid - 1
      }
    }

    const lineIndex = Math.max(0, high)
    const index = toIndex(offset)

    return {
      line: lineIndex + 1,
      column: index - lineStarts[lineIndex].index,
    }
  }) as Locator

  locator.toIndex = toIndex

  return locator
}

export function toRange(range: NativeRange, locator: Locator): [number, number] {
  return [locator.toIndex(range.start), locator.toIndex(range.end)]
}

export function toLocation(range: NativeRange, locator: Locator) {
  return {
    start: locator(range.start),
    end: locator(range.end),
  }
}

function utf8ByteLength(codePoint: number) {
  if (codePoint <= 0x7f) {
    return 1
  }

  if (codePoint <= 0x7ff) {
    return 2
  }

  if (codePoint <= 0xffff) {
    return 3
  }

  return 4
}
