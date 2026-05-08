<template>
  <Comp #="{ message }"></Comp>
  <Comp #header="{ message }">
    {{ message }}
  </Comp>
  <Comp v-slot:abc> abc </Comp>
  <Comp v-slot />
  <Comp v-slot:header="{ message }">
    {{ message }}
  </Comp>
  <Comp v-slot:="{ message }">
    {{ message }}
  </Comp>
  <Comp v-slot:[key]="{ message }">
    {{ message }}
  </Comp>
  <Comp v-slot:header="user">
    {{ user.name }}
  </Comp>
</template>
