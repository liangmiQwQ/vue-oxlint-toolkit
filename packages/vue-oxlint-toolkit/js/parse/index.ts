import type { LocationConvertor } from '../location'
import type { NativeSfc, ParserOptions } from './types'
import { fixNativeNode } from './fix'
import { rebuildScriptProgram } from './script'
import { visitorKeys } from './visitorKeys'

import { rebuildTemplateBody } from './template'

export function rebuildProgram(
  sfc: NativeSfc,
  source: string,
  convertor: LocationConvertor,
  options: ParserOptions,
) {
  const script = rebuildScriptProgram(sfc, source, convertor, options)
  const templateBody = rebuildTemplateBody(sfc, convertor)
  const program: any = {
    type: 'Program',
    body: script.body,
    sourceType: script.sourceType,
    comments: script.comments,
    tokens: fixNativeNode(sfc.scriptTokens, convertor),
    range: script.range,
    start: script.range[0],
    end: script.range[1],
    get loc() {
      return convertor.fix({ range: script.range }).loc
    },
  }

  if (templateBody) {
    templateBody.tokens = fixNativeNode(sfc.templateTokens, convertor)
    templateBody.comments = fixNativeNode(sfc.template_comments, convertor)
    templateBody.errors = []
    program.templateBody = templateBody
  }

  visit(program, null)

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
