import type { LocationConvertor } from '../location'

export interface NativeSfc {
  type: 'VueSingleFileComponent'
  children: NativeNode[]
  script_comments: NativeNode[]
  template_comments: NativeNode[]
  scriptTokens: NativeNode[]
  templateTokens: NativeNode[]
  source_type?: string
  range: [number, number]
  start: number
  end: number
}

export type NativeNode = Record<string, any> & {
  type: string
  range?: [number, number]
  start?: number
  end?: number
}

export interface RebuildContext {
  source: string
  convertor: LocationConvertor
  options: ParserOptions
}

export type ParserOptions = Record<string, any> | undefined

export interface ScriptBlock {
  bodyStart: number
  bodyEnd: number
  lang?: string
  setup: boolean
}
