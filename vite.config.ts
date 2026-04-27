import { defineConfig } from 'vite-plus';

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
    include: ['packages/**/*.{ts,js}'],
  },
  fmt: {
    include: ['packages/**/*.{ts,js}', '*.{ts,js}'],
  },
});
