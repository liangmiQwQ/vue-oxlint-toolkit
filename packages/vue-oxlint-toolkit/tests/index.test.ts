import { plus100 } from '../bindings'
import { it, expect } from 'vite-plus/test'

it('should work', () => expect(plus100(1)).toBe(101))
