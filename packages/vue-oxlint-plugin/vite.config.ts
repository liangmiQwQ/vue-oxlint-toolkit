import { defineConfig } from 'vite-plus';

export default defineConfig({
  // `vp pack` (tsdown) builds the JS wrapper and generates .d.ts from the
  // napi-rs auto-generated bindings.
  pack: {
    entry: ['index.ts'],
    dts: true,
    format: ['cjs'],
  },
  test: {
    include: ['__test__/**/*.spec.ts'],
  },
});
