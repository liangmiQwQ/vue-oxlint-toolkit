<template>
  <div v-for="item in source" />
  <div v-for="item1 of source1" />
  <div v-for="(item2, index2) in source2" />
  <div v-for="(item3 = '1', index3 = 99) in source3" />
  <div v-for="{ id4, name4 } in users4" />
  <div v-for="({ id5, name5 }, index) in users5" />
  <div v-for="({ id6 = 1, name6 = 'Liang' }, index) in users6" />
  <div v-for="(key7, value7, index7) in someObj7" />
  <div v-for="[item8, index8] in someIter8" />
  <div v-for="[item9 = 'hi', index9] in someIter9" />
</template>
