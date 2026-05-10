import type { LineColumn, Location, Ranged } from '@oxlint/plugins'

// JavaScript uses UTF-16 for internal string representation, while Rust uses UTF-8 for string representation.
// We have to convert it when we get location metadata from Rust side.

const LINE_BREAK_PATTERN = /\r\n|[\r\n\u2028\u2029]/gu

export interface LocationConvertor {
  sourceText: string
  // For ASCII chars, utf-8 and utf-16 have the same span, so we only record non utf-8 chars' checkpoints.
  // So that `toUtf16` can directly find the nearest checkpoint and calculate offset.
  checkPoints: CheckPoint[]
  fix: <T extends Ranged | { start: number; end: number }>(node: T) => any
  range: (range: [number, number]) => [number, number]
  toUtf16: ({ start, end }: { start: number; end: number }) => {
    start: number
    end: number
  }
}

interface CheckPoint {
  utf8: number
  utf16: number
}

export function getConvertor(sourceText: string): LocationConvertor {
  const checkPoints = createCheckPoints(sourceText)

  const convertor: LocationConvertor = {
    sourceText,
    checkPoints,
    toUtf16: ({ start, end }) => ({
      start: toUtf16Offset(sourceText, checkPoints, start),
      end: toUtf16Offset(sourceText, checkPoints, end),
    }),
    range: ([start, end]) => {
      const fixed = convertor.toUtf16({ start, end })

      return [fixed.start, fixed.end]
    },
    fix: (node) => {
      const hasRange = 'range' in node
      const [utf8Start, utf8End] = hasRange ? node.range : [node.start, node.end]
      // We should use utf16 location for location creation.
      const { start, end } = convertor.toUtf16({ start: utf8Start, end: utf8End })

      let loc: Location | undefined

      if ('type' in node && 'value' in node) {
        const {
          start: _start,
          end: _end,
          ...rest
        } = node as typeof node & {
          start?: number
          end?: number
        }
        return {
          ...rest,
          range: [start, end],
          get loc() {
            return (loc ??= createLocation(sourceText, start, end))
          },
        }
      }

      return {
        ...node,
        start,
        end,
        range: [start, end],
        get loc() {
          return (loc ??= createLocation(sourceText, start, end))
        },
      }
    },
  }

  return convertor
}

function createCheckPoints(sourceText: string): CheckPoint[] {
  const checkPoints: CheckPoint[] = [{ utf8: 0, utf16: 0 }]
  let utf8 = 0
  let utf16 = 0

  while (utf16 < sourceText.length) {
    const codePoint = sourceText.codePointAt(utf16)!
    const utf16Length = codePoint > 0xffff ? 2 : 1
    const utf8Length = getUtf8Length(codePoint)

    if (codePoint > 0x7f) {
      checkPoints.push({ utf8, utf16 })
    }

    utf8 += utf8Length
    utf16 += utf16Length
  }

  return checkPoints
}

function toUtf16Offset(sourceText: string, checkPoints: CheckPoint[], offset: number): number {
  let { utf8, utf16 } = findCheckPoint(checkPoints, offset)

  while (utf8 < offset && utf16 < sourceText.length) {
    const codePoint = sourceText.codePointAt(utf16)!
    const utf16Length = codePoint > 0xffff ? 2 : 1
    const utf8Length = getUtf8Length(codePoint)

    if (utf8 + utf8Length > offset) {
      break
    }

    utf8 += utf8Length
    utf16 += utf16Length
  }

  return utf16
}

function findCheckPoint(checkPoints: CheckPoint[], offset: number): CheckPoint {
  let low = 0
  let high = checkPoints.length

  while (low < high) {
    const mid = (low + high) >> 1

    if (checkPoints[mid].utf8 <= offset) {
      low = mid + 1
    } else {
      high = mid
    }
  }

  return checkPoints[low - 1]
}

function getUtf8Length(codePoint: number): number {
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

function createLocation(sourceText: string, start: number, end: number): Location {
  const lineStartIndices = getLineStartIndices(sourceText)

  return {
    start: lineColumn(lineStartIndices, start),
    end: lineColumn(lineStartIndices, end),
  }
}

function getLineStartIndices(sourceText: string): number[] {
  const lineStartIndices = [0]

  for (const match of sourceText.matchAll(LINE_BREAK_PATTERN)) {
    lineStartIndices.push(match.index + match[0].length)
  }

  return lineStartIndices
}

function lineColumn(lineStartIndices: number[], offset: number): LineColumn {
  let low = 0
  let high = lineStartIndices.length

  while (low < high) {
    const mid = (low + high) >> 1

    if (offset < lineStartIndices[mid]) {
      high = mid
    } else {
      low = mid + 1
    }
  }

  return {
    line: low,
    column: offset - lineStartIndices[low - 1],
  }
}
