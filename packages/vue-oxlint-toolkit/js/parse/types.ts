export interface NativeSfc {
  type: 'VueSingleFileComponent'
  children: NativeNode[]
  script_comments: NativeNode[]
  scriptBody: NativeNode[]
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
