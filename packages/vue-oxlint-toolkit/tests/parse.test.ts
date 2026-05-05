import { readdirSync, readFileSync } from 'node:fs'
import { basename, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'
import type { AST } from 'vue-eslint-parser'
import { parse as parseWithVueEslintParser } from 'vue-eslint-parser'
import { expect, it } from 'vite-plus/test'
import { parse } from '../js'

const root = fileURLToPath(new URL('../../..', import.meta.url))

it('matches vue-eslint-parser for non-error fixtures', () => {
  for (const fixture of listVueFixtures(join(root, 'fixtures'))) {
    const source = readFileSync(fixture, 'utf8')
    if (shouldSkipFixture(fixture, source)) {
      continue
    }

    const relativeFixture = relative(root, fixture)
    const expected = parseWithVueEslintParser(source, { sourceType: 'module' }) as ParserProgram
    delete expected.errors

    expect(parse(relativeFixture, source).ast, relativeFixture).toEqual(expected)
  }
})

type ParserProgram = AST.ESLintProgram & { errors?: unknown }

function listVueFixtures(dir: string): string[] {
  const fixtures: string[] = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      fixtures.push(...listVueFixtures(path))
    } else if (entry.isFile() && path.endsWith('.vue')) {
      fixtures.push(path)
    }
  }
  return fixtures
}

function shouldSkipFixture(fixture: string, source: string) {
  const relativeFixture = relative(root, fixture)
  if (relativeFixture.includes('/error/') || basename(fixture).includes('error')) {
    return true
  }

  // vue-eslint-parser delegates non-JS script blocks to an external parser.
  return /<script\b[^>]*\blang=["']tsx?["']/i.test(source)
}
