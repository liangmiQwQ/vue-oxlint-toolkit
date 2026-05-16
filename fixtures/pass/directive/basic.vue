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
  <div :1foo="bar" />

  <div :foo=bar.baz />
  <div :foo={bar:baz} />
  <div :[foo].bar="baz" />
  <div v-bind:[foo].bar="baz" />
  <div @click.stop.prevent="foo" />
  <div v-on:click.stop.prevent="foo" />
  <Comp v-model.trim.lazy="foo" />
  <Comp v-model:title.trim="foo" />
  <Comp v-slot="{ message }" />
</template>
