import { defineConfig } from 'vite-plus'

export default defineConfig({
  run: {
    tasks: {
      // Build the native napi-rs module for the current platform.
      // The Rust compilation itself is handled by `cargo build` inside napi.
      'build:native': {
        command: 'vp run --filter @vue-oxlint/plugin build:native',
      },
    },
  },
  lint: {
 options: {
      typeAware: true,
      typeCheck: true,
    },
  },
  fmt: {
    singleQuote: true,
    semi: false,
    sortPackageJson: true,
    exclude: ['**/*.vue'],
  },
  staged: {
    '*.{js,ts,tsx,vue,svelte}': 'vp check --fix',
    '*.{rs}': 'just fix',
  },
})
