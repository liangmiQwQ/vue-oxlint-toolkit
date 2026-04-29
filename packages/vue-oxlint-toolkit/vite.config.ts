import { defineConfig } from 'vite-plus'

export default defineConfig({
  pack: {
    fixedExtension: true,
    platform: 'node',
    entry: {
      index: './js/index.ts',
    },
    dts: true,
  },
})
