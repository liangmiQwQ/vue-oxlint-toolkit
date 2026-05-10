import type { LineColumn, Location, Ranged } from '@oxlint/plugins'

const LINE_BREAK_PATTERN = /\r\n|[\r\n\u2028\u2029]/gu

type HasRange = Ranged | { start: number; end: number }

export function withLoc<T extends HasRange>(sourceText: string, node: T): T & { loc: Location } {
  const [start, end] = 'range' in node ? node.range : [node.start, node.end]

  let loc: Location | undefined

  return {
    ...node,
    get loc() {
      return (loc ??= createLocation(sourceText, start, end))
    },
  }
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
