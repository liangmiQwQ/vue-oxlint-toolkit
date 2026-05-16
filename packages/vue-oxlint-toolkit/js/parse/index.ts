import type { LocationConvertor } from '../location'
import type { NativeSfc, ParserOptions } from './types'
import { fixNativeNode } from './fix'
import { injectParents } from './parents'
import { rebuildScriptProgram } from './script'
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

  injectParents(program)

  return program
}
