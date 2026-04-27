<template>
  <div :class="'w-100'" />
  <div :[some]="2" />
  <Some #default="{ a }" />
  <input v-model="text" />
  <Some v-bind:some.none="1" />
  <div :id />
  <div :msg-id />
  <div v-bind="{ id: 'app', class: 'w-100' }" />
  <div :="{ id: 'app' }" />
</template>
