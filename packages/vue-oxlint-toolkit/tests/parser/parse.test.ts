import { describe, expect, it } from 'vite-plus/test'
import { parse } from '../../js'
import type {
  VAttribute,
  VDirectiveKey,
  VElement,
  VExpressionContainer,
  VIdentifier,
  VRootChild,
  VText,
} from '../../js/ast'

function getElement(child: VRootChild, name: string): VElement {
  if (child.type !== 'VElement') throw new Error(`expected VElement, got ${child.type}`)
  if (child.name !== name) throw new Error(`expected <${name}>, got <${child.name}>`)
  return child
}

describe('parse() — top-level layout', () => {
  it('returns a VDocumentFragment with template + script blocks', () => {
    const src = '<template><div /></template>\n<script>let x = 1</script>\n'
    const r = parse(src)
    expect(r.document.type).toBe('VDocumentFragment')
    const blocks = r.document.children.filter(
      (c: VRootChild): c is VElement => c.type === 'VElement',
    )
    expect(blocks.map((b: VElement) => b.name)).toEqual(['template', 'script'])
  })

  it('preserves source-relative byte ranges on the document fragment', () => {
    const src = '<template></template>'
    const r = parse(src)
    expect(r.document.range).toEqual({ start: 0, end: src.length })
  })
})

describe('parse() — template body', () => {
  it('parses elements, text, and mustache interpolations', () => {
    const src = '<template><div class="a">hi {{ count + 1 }}!</div></template>'
    const r = parse(src)
    const tpl = getElement(r.document.children[0], 'template')
    const div = (tpl.children[0] as { VElement: VElement }).VElement
    expect(div.name).toBe('div')
    expect(div.children).toHaveLength(3)

    const text1 = (div.children[0] as { VText: VText }).VText
    expect(text1.value).toBe('hi ')

    const expr = (div.children[1] as { VExpressionContainer: VExpressionContainer })
      .VExpressionContainer
    expect(expr.type).toBe('VExpressionContainer')
    expect(expr.raw_expression).toBe('count + 1')

    const text2 = (div.children[2] as { VText: VText }).VText
    expect(text2.value).toBe('!')
  })

  it('classifies directive shorthands (`:`, `@`, `#`) as directives', () => {
    const src = '<template><c :foo="bar" @click.stop="onClick" #default /></template>'
    const r = parse(src)
    const tpl = getElement(r.document.children[0], 'template')
    const c = (tpl.children[0] as { VElement: VElement }).VElement
    const attrs = c.start_tag.attributes
    expect(attrs).toHaveLength(3)
    const [bind, on, slot] = attrs as VAttribute[]
    expect(bind.directive).toBe(true)
    expect((bind.key as VDirectiveKey).name).toBe('bind')
    expect((bind.key as VDirectiveKey).argument).toBe('foo')
    expect(on.directive).toBe(true)
    expect((on.key as VDirectiveKey).name).toBe('on')
    expect((on.key as VDirectiveKey).argument).toBe('click')
    expect((on.key as VDirectiveKey).modifiers).toEqual(['stop'])
    expect(slot.directive).toBe(true)
    expect((slot.key as VDirectiveKey).name).toBe('slot')
    expect((slot.key as VDirectiveKey).argument).toBe('default')
  })

  it('keeps plain HTML attributes as VIdentifier keys', () => {
    const src = '<template><div id="x" /></template>'
    const r = parse(src)
    const tpl = getElement(r.document.children[0], 'template')
    const div = (tpl.children[0] as { VElement: VElement }).VElement
    const id = div.start_tag.attributes[0]
    expect(id.directive).toBe(false)
    expect((id.key as VIdentifier).name).toBe('id')
  })
})

describe('parse() — <script> blocks', () => {
  it('parses script content via oxc into a Program', () => {
    const src = '<script>const a = 1</script>'
    const r = parse(src)
    expect(r.scripts).toHaveLength(1)
    const program = r.scripts[0].program as { type?: string; body?: unknown[] }
    expect(program.type).toBe('Program')
    expect(Array.isArray(program.body)).toBe(true)
  })

  it('detects setup and lang attributes', () => {
    const src = '<script setup lang="ts">const x: number = 1</script>'
    const r = parse(src)
    expect(r.scripts[0].setup).toBe(true)
    expect(r.scripts[0].lang).toBe('ts')
  })
})
