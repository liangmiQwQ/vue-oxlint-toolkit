import type { VueSingleFileComponent } from './nodes'
import { isObject } from './nodes'

export function parseVueSingleFileComponent(astJson: string): VueSingleFileComponent {
  const parsed: unknown = JSON.parse(astJson)
  if (!isVueSingleFileComponent(parsed)) {
    throw new TypeError('Native Vue parser returned an invalid VueSingleFileComponent payload.')
  }

  return parsed
}

function isVueSingleFileComponent(value: unknown): value is VueSingleFileComponent {
  return (
    isObject(value) &&
    value.type === 'VueSingleFileComponent' &&
    Array.isArray(value.children) &&
    Array.isArray(value.script_comments) &&
    Array.isArray(value.template_comments) &&
    Array.isArray(value.scriptTokens) &&
    Array.isArray(value.templateTokens)
  )
}
