import { liangmi } from '@liangmi/vp-config'

const ignorePatterns = ['**/*.vue', 'packages/*/bindings/**', 'MAPPING.md', 'fixtures/**']

export default await liangmi({ lint: { ignores: ignorePatterns }, fmt: { ignorePatterns } })
