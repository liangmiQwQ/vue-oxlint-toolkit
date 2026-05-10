import type { LineColumn, Location } from '@oxlint/plugins'

const LINE_BREAK_PATTERN = /\r\n|[\r\n\u2028\u2029]/gu

export interface LocationGetter {
  loc: (start: number, end: number) => Location
}

export function createLocationGetter(source: string): LocationGetter {
  const lineStartIndices = [0]

  for (const match of source.matchAll(LINE_BREAK_PATTERN)) {
    lineStartIndices.push(match.index + match[0].length)
  }

  const lineColumn = (offset: number): LineColumn => {
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

  return {
    loc: (start, end) => ({
      start: lineColumn(start),
      end: lineColumn(end),
    }),
  }
}
