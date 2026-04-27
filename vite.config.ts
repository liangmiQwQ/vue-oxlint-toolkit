import { defineConfig } from 'vite-plus'

const ignorePatterns = ['**/*.vue', '**/fixtures/**', 'packages/*/bindings/**']

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
