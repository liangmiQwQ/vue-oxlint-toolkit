import { liangmi } from '@liangmi/vp-config'

export default await liangmi({
  pack: {
    platform: 'node',
    entry: {
      index: './js/index.ts'
    },
    deps: {
      neverBundle: ['@eslint/plugin-kit']
    }
  }
})
