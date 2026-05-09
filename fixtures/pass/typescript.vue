<script lang="ts" setup>
const a = someFunction<{ msg: 1 }>()
const b = someFunction<SomeType>() // It's not a tag
</script>

<template>
  <div>{{ a.msg }}</div>
  <div> I think 1 < 2, isn't it? </div>
</template>
