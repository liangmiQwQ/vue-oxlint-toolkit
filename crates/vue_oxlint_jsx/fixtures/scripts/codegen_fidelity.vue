<script lang="tsx">
import { foo as foo, bar as baz } from './dep'
export { foo as foo }

const decimal = 1.0
const separated = 1_000_000
const hex = 0x10
const binary = 0b1010
const bigint = 0x10n
const escaped = '\x41'
const attrs = <div label={'\x42'} raw="\x43" />
const grouped = (a + b) + c
const object = { foo: foo, wrapped: (foo) }
const { foo: foo, bar: bar = 1 } = source
const arrow = (x) => x
const instance = new Foo()
</script>
