import { parseSync, plus100 } from '../bindings/index.js'
import type * as VAst from './ast'

export { plus100 }
export type * from './ast'

export interface ParseResult {
  document: VAst.VDocumentFragment
  scripts: VAst.ParsedScript[]
}

/**
 * Parse a Vue SFC source string into a Vue + JS AST tree.
 *
 * The Rust side returns a JSON string; this wrapper performs a single
 * `JSON.parse` and returns the typed result. There is no further hydration:
 * the JSON is the AST.
 */
export function parse(source: string): ParseResult {
  const raw = parseSync(source)
  return JSON.parse(raw) as ParseResult
}
