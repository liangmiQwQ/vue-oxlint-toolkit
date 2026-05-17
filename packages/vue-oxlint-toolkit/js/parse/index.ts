import type { LocationConvertor } from '../location'
import type { NativeSfc, ParserOptions } from './types'
import { fixNativeNode } from './fix'
import { rebuildScriptProgram } from './script'
import { rebuildTemplate } from './template'
import { visitorKeys } from './visitorKeys'

export function rebuildProgram(
  sfc: NativeSfc,
  source: string,
  convertor: LocationConvertor,
  options: ParserOptions,
) {
  const script = rebuildScriptProgram(sfc, source, convertor, options)
  const template = rebuildTemplate(sfc, source, convertor)
  const program: any = {
    type: 'Program',
    body: script.body,
    sourceType: script.sourceType,
    comments: script.comments,
    tokens: fixNativeNode(sfc.scriptTokens, convertor),
    range: script.range,
    get loc() {
      return convertor.fix({ range: script.range }).loc
    },
  }

  if (template.templateBody) {
    const templateBody = template.templateBody
    templateBody.tokens = template.tokens
    templateBody.comments = template.comments
    program.templateBody = templateBody
  }

  visit(program, null)
  if (template.fragment) {
    visit(template.fragment, null)
  }

  return program
}

function visit(node: unknown, parent: unknown) {
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
