// JS entry point for @vue-oxlint/plugin.
// The napi-rs CLI generates `index.js` / `index.d.ts` from the Rust bindings.
// This file re-exports them with any additional JS-side helpers.

export * from './vue-oxlint-plugin.node'
