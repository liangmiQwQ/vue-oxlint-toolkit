import { defineConfig } from 'vite-plus'

const ignorePatterns = ['**/*.vue', 'packages/*/bindings/**', 'MAPPING.md']

export default defineConfig({
  lint: {
    options: {
      typeAware: true,
      typeCheck: true,
    },
    ignorePatterns,
  },
  fmt: {
    singleQuote: true,
    semi: false,
    sortPackageJson: true,
    excludeFiles: [],
    ignorePatterns,
  },
  staged: {
    '*.{js,ts,tsx,vue,svelte}': 'vp check --fix',
  },
})
