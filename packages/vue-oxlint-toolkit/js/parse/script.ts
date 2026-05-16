import type { NativeSfc, ParserOptions, ScriptBlock } from './types'
import { collectScriptBlocks } from './template'
import type { LocationConvertor } from '../location'

export interface ScriptParseResult {
  body: any[]
  comments: any[]
  range: [number, number]
  sourceType: 'module' | 'script'
}

export function rebuildScriptProgram(
  sfc: NativeSfc,
  source: string,
  convertor: LocationConvertor,
  options: ParserOptions,
): ScriptParseResult {
  const blocks = collectScriptBlocks(sfc, convertor)
  const parser = resolveParser(options, blocks)
  const sourceType = options?.sourceType === 'script' ? 'script' : 'module'

  if (!parser) {
    const fallback = blocks[0]?.bodyEnd ?? 0
    return { body: [], comments: [], range: [fallback, fallback], sourceType }
  }

  const parsedBlocks = blocks.map((block) => parseScriptBlock(parser, source, block, options))
  const body = parsedBlocks.flatMap((block) => block.body)
  const comments = parsedBlocks.flatMap((block) => block.comments)

  if (body.length > 0) {
    const blocksWithBody = parsedBlocks.filter((block) => block.body.length > 0)
    const end =
      blocksWithBody.length === 1 ? blocksWithBody[0].block.bodyEnd : body[body.length - 1].range[1]

    return {
      body,
      comments,
      range: [body[0].range[0], end],
      sourceType,
    }
  }

  if (comments.length > 0) {
    return { body, comments, range: [commentOnlyProgramStart(source, comments), 0], sourceType }
  }

  const fallback = blocks[0]?.bodyEnd ?? 0
  return { body, comments, range: [fallback, fallback], sourceType }
}

function parseScriptBlock(parser: any, source: string, block: ScriptBlock, options: ParserOptions) {
  const ast = parseWithParser(parser, createPaddedSource(source, block), parserOptions(options))
  const body = ast.body ?? []

  if (block.setup) {
    for (const node of body) {
      delete node.directive
    }
  }

  return {
    block,
    body,
    comments: ast.comments ?? [],
  }
}

function parseWithParser(parser: any, source: string, options: Record<string, any>) {
  if (typeof parser.parseForESLint === 'function') {
    return parser.parseForESLint(source, options).ast
  }

  return parser.parse(source, options)
}

function parserOptions(options: ParserOptions) {
  return {
    ...options?.parserOptions,
    ecmaVersion: options?.ecmaVersion ?? options?.parserOptions?.ecmaVersion ?? 'latest',
    sourceType: options?.sourceType ?? options?.parserOptions?.sourceType ?? 'module',
    ecmaFeatures: options?.ecmaFeatures ?? options?.parserOptions?.ecmaFeatures,
    comment: true,
    loc: true,
    range: true,
    tokens: true,
  }
}

function resolveParser(options: ParserOptions, blocks: ScriptBlock[]) {
  const parser = options?.parser
  if (!parser) {
    return undefined
  }

  if (typeof parser.parse === 'function' || typeof parser.parseForESLint === 'function') {
    return parser
  }

  for (const block of blocks) {
    if (block.lang && parser[block.lang]) {
      return parser[block.lang]
    }
  }

  return parser.js ?? parser.ts ?? parser.jsx ?? parser.tsx
}

function createPaddedSource(source: string, block: ScriptBlock) {
  let padded = ''

  for (let index = 0; index < source.length; index++) {
    if (index >= block.bodyStart && index < block.bodyEnd) {
      padded += source[index]
    } else {
      padded += isLineBreak(source[index]) ? source[index] : ' '
    }
  }

  return padded
}

function isLineBreak(char: string) {
  return char === '\n' || char === '\r' || char === '\u2028' || char === '\u2029'
}

function commentOnlyProgramStart(source: string, comments: any[]) {
  // vue-eslint-parser exposes this invalid Program range for comment-only scripts.
  // Keeping it here makes the compatibility layer match callers that snapshot it.
  const block = comments.find(
    (comment) =>
      comment.type === 'Block' && source.slice(comment.range[0], comment.range[1]).includes('\n'),
  )
  if (!block) {
    return comments[0].range[0]
  }

  const raw = source.slice(block.range[0], block.range[1])
  const lines = raw.split(/\r\n|[\r\n\u2028\u2029]/u)
  let offset = 0
  let longest = { start: 0, value: '' }

  for (const line of lines) {
    if (line.length > longest.value.length) {
      longest = { start: offset, value: line }
    }
    offset += line.length + 1
  }

  const lastSpace = longest.value.lastIndexOf(' ')
  return block.range[0] + longest.start + (lastSpace === -1 ? 0 : lastSpace)
}
