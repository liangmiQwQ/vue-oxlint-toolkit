import { readdirSync, readFileSync } from 'node:fs'
import { join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const FIXTURES_ROOT = fileURLToPath(new URL('../../../fixtures', import.meta.url))

export function readTestFiles(): TestFiles {
  return {
    pass: _readFixtureGroup('pass'),
    error: _readFixtureGroup('error'),
    panic: _readFixtureGroup('panic'),
  }
}

function _readFixtureGroup(group: keyof TestFiles): TestFile[] {
  return _walkVueFixtures(join(FIXTURES_ROOT, group)).map((filePath) => ({
    path: relative(FIXTURES_ROOT, filePath),
    source_text: readFileSync(filePath, 'utf8'),
  }))
}

function _walkVueFixtures(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true })
    .sort((a, b) => a.name.localeCompare(b.name))
    .flatMap((entry) => {
      const filePath = join(dir, entry.name)

      if (entry.isDirectory()) {
        return _walkVueFixtures(filePath)
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
