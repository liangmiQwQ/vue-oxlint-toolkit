import type { Locator } from './locator'
import { attachAstMetadata, attachListMetadata } from './ast/metadata'
import type {
  AstNode,
  DocumentFragmentNode,
  OffsetRange,
  OxlintProgram,
  VueElementNode,
  VueSingleFileComponent,
  VueSfcChild,
} from './ast/nodes'
import {
  getArray,
  getEnd,
  getNodeArray,
  getRange,
  getStart,
  isPureScriptNode,
  isScriptElement,
  isTemplateElement,
} from './ast/nodes'

export function toProgram(sfc: VueSingleFileComponent, locator: Locator): OxlintProgram {
  const children = sfc.children
  const body = collectProgramBody(children)
  const templateBody = findTemplateBody(children)
  const templateComments = sfc.template_comments
  const templateTokens = sfc.templateTokens
  const fragment = createDocumentFragment(sfc, children, templateComments, templateTokens)
  const programRange = getProgramRange(body, sfc.scriptTokens)
  const program: AstNode = {
    type: 'Program',
    sourceType: sfc.source_type ?? 'module',
    body,
    comments: sfc.script_comments,
    tokens: sfc.scriptTokens,
    templateBody: templateBody ?? undefined,
    start: programRange[0],
    end: programRange[1],
    range: programRange,
  }

  if (templateBody) {
    templateBody.comments = templateComments
    templateBody.tokens = templateTokens
    templateBody.errors = getArray(templateBody.errors)
  }

  attachAstMetadata(program, locator)
  attachAstMetadata(fragment, locator, null)
  attachListMetadata(getArray(program.comments), locator)
  attachListMetadata(getArray(program.tokens), locator)
  attachListMetadata(templateComments, locator)
  attachListMetadata(templateTokens, locator)

  return program as OxlintProgram
}

function collectProgramBody(children: AstNode[]): AstNode[] {
  const body: AstNode[] = []

  for (const child of children) {
    if (isPureScriptNode(child)) {
      body.push(...child.body)
      continue
    }

    if (isScriptElement(child)) {
      for (const scriptChild of getNodeArray(child.children)) {
        if (isPureScriptNode(scriptChild)) {
          body.push(...scriptChild.body)
        }
      }
    }
  }

  return body
}

function findTemplateBody(children: AstNode[]): VueElementNode | null {
  return children.find(isTemplateElement) ?? null
}

function createDocumentFragment(
  sfc: VueSingleFileComponent,
  children: VueSfcChild[],
  comments: VueSingleFileComponent['template_comments'],
  tokens: VueSingleFileComponent['templateTokens'],
): DocumentFragmentNode {
  return {
    type: 'VDocumentFragment',
    children,
    comments,
    errors: getArray(sfc.template_errors),
    parent: null,
    range: getRange(sfc) ?? [0, 0],
    start: sfc.start,
    end: sfc.end,
    tokens,
  }
}

function getProgramRange(body: AstNode[], scriptTokens: AstNode[]): OffsetRange {
  const firstBody = body[0]
  const lastBody = body.at(-1)
  const bodyStart = firstBody ? getStart(firstBody) : undefined
  const bodyEnd = lastBody ? getEnd(lastBody) : undefined

  if (typeof bodyStart === 'number' && typeof bodyEnd === 'number') {
    return [bodyStart, bodyEnd]
  }

  const firstToken = scriptTokens[0]
  const lastToken = scriptTokens.at(-1)
  const tokenStart = firstToken ? getEnd(firstToken) : undefined
  const tokenEnd = lastToken ? getStart(lastToken) : undefined

  if (typeof tokenStart === 'number' && typeof tokenEnd === 'number') {
    return [tokenStart, tokenEnd]
  }

  return [0, 0]
}
