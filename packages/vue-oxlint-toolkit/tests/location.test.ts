import { expect, it } from 'vite-plus/test'
import { getConvertor } from '../js/location'

it('keeps ASCII offsets unchanged', () => {
  const convertor = getConvertor('const answer = 42')

  expect(convertor.checkPoints).toEqual([{ utf8: 0, utf16: 0 }])
  expect(convertor.toUtf16({ start: 6, end: 12 })).toEqual({ start: 6, end: 12 })
})

it('converts UTF-8 byte offsets from the nearest checkpoint', () => {
  const convertor = getConvertor('abc你好def😊xyz')

  expect(convertor.checkPoints).toEqual([
    { utf8: 0, utf16: 0 },
    { utf8: 3, utf16: 3 },
    { utf8: 6, utf16: 4 },
    { utf8: 12, utf16: 8 },
  ])
  expect(convertor.toUtf16({ start: 6, end: 10 })).toEqual({ start: 4, end: 6 })
  expect(convertor.toUtf16({ start: 17, end: 19 })).toEqual({ start: 11, end: 13 })
})

it('fixes ranges and locations with UTF-16 offsets', () => {
  const convertor = getConvertor('a你好\nbc')
  const fixed = convertor.fix({ start: 8, end: 10 })

  expect(fixed).toMatchObject({
    start: 4,
    end: 6,
    range: [4, 6],
    loc: {
      start: { line: 2, column: 0 },
      end: { line: 2, column: 2 },
    },
  })
})
