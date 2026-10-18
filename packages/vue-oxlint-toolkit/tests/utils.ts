import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const FIXTURES_ROOT = fileURLToPath(new URL('../../../fixtures', import.meta.url))

export function readTestFiles(): TestFiles {
  return {
    pass: readFixtureGroup('pass'),
    error: readFixtureGroup('error'),
    panic: readFixtureGroup('panic')
  }
}

function readFixtureGroup(group: keyof TestFiles): TestFile[] {
  return walkVueFixtures(join(FIXTURES_ROOT, group)).map(filePath => ({
    path: relative(FIXTURES_ROOT, filePath),
    source_text: readFileSync(filePath, 'utf8')
  }))
}

function walkVueFixtures(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true })
    .toSorted((a, b) => a.name.localeCompare(b.name))
    .flatMap(entry => {
      const filePath = join(dir, entry.name)

      if (entry.isDirectory()) {
        return walkVueFixtures(filePath)
      }

      return entry.isFile() && entry.name.endsWith('.vue') ? [filePath] : []
    })
}

interface TestFiles {
  pass: TestFile[]
  error: TestFile[]
  panic: TestFile[]
}

interface TestFile {
  path: string
  source_text: string
}
