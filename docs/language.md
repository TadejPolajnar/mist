# Mist language guide

Everything on this page is implemented and tested. When SPEC.md promises more,
SPEC is the roadmap; this is the product.

## File anatomy

A `.mist` file has up to three sections:

```
---
// TypeScript frontmatter — parsed by a real TS parser (oxc)
---
<!-- template — JSX-ish markup -->
<style>/* optional, compiled to WXSS */</style>
```

Frontmatter is required. Files are pages (`pages/*.mist`), components
(PascalCase filenames, imported by pages), or `app.mist` (app shell only:
lifecycle + `config` + global `<style>`; state and templates are compile errors
there).

## Reactivity

```ts
import { state, derived } from 'mist'

const count = state(0)
const todos = state([{ id: 1, title: 'hi', done: false }])
const open  = derived(() => todos.value.filter(t => !t.done))
```

Reads are `x.value`. Writes are **compiled**, not observed — every mutation
becomes the exact `setData` path it changes:

| you write | what ships across the bridge |
|---|---|
| `count.value++` | `{ count: <new> }` |
| `todos.value[i].done = true` | `` { `todos[${i}].done`: true } `` |
| `todos.value.push(t)` | `` { `todos[${len}]`: t } `` |
| `todos.value = [...]` | one whole-key write |
| `todos.value.splice(...)` | **compile error M1004** — reassign instead |

All writes in one event handler batch into **one** `setData` per page.

**Derived values** recompute once per batch. A derived array rendered with a
`key` gets a keyed, field-level diff: toggling one item ships
`{ 'open[3].done': true }`, nothing else. Length/order changes fall back to one
whole-key write. Deriveds are read-only.

**Dead-data elimination:** state your template never reads (e.g. `todos` when
only `open` is rendered) never enters `data` at all — it lives as an instance
field with zero bridge cost, and mutations trigger only the derived recompute.
This is automatic.

**Rules the compiler enforces:** mutate only through `x.value...` paths
(aliasing into a local and writing through it is not compiled — it will
silently not update; the `M1001` alias check is on the roadmap). Array methods
other than `push`/index assignment are M1004 errors.

## Templates

Web-familiar tags map to native ones; native tags pass through untouched:

| authored | emitted |
|---|---|
| `div section header footer main article ul ol li p h1–h6 nav aside` | `view` |
| `span` | `text` (bare text/`{expr}` emit unwrapped) |
| `img` | `image` |
| `a href="/pages/x/x"` | `navigator url="…"` |
| `button input scroll-view swiper …` | passthrough |

**Bindings** — `{expr}` → `{{expr}}` with `.value` stripped. Member access,
arithmetic, comparisons, ternaries, `&&`/`||`, string concat run inline in WXML.
**Function calls are hoisted automatically**: `{fmt(total.value)}` compiles to a
generated derived recomputed per batch, and inside loops `{fmtDate(t.ts)}`
becomes a computed field on each item (`t._c0`) with keyed diffing preserved.
(Limit: calls inside *nested* loops aren't hoisted yet.)

**Lists** — `.map()` with a **required key**:

```jsx
{todos.value.map(t => (
  <div key={t.id} onTap={() => toggle(t.id)}>{t.title}</div>
))}
```

`key` must be a direct property (`t.id`), the item itself (`key={t}` → `*this`
for primitives), or `index` (allowed, disables keyed diffing). Computed or deep
keys are **M1003** errors.

**Conditionals** — `{cond && <jsx/>}` → `wx:if`. Non-JSX `&&` stays a binding.

**Events** — `on` + capitalized event name:

```jsx
<button onTap={save}>save</button>            // bindtap="save"
<div onTap:catch={f}>…</div>                  // catchtap (stops propagation)
<div onTap:mut={f}>…</div>                    // mut-bind:tap
<input onInput={setQuery} />                   // handler receives the native event
<div onTap={() => del(item.id, 'soft')}>…</div>
```

`onClick` is an alias for `onTap`. Bare identifiers bind directly (the method
receives the native WeChat event). Inline arrows may only be
`() => method(args…)` — the args are captured via `data-*` attributes, so they
must be serializable expressions. Arrows with an event parameter are not
supported; use a bare method and read `e.detail`/`e.currentTarget.dataset`.

**Classes** — Tailwind everywhere, including conditional expressions:

```jsx
<span class={t.done ? 'line-through text-gray-400' : ''}>{t.title}</span>
```

Class names with special characters are sanitized consistently in markup and
CSS (`w-[32px]` → `w-_32px_`), so arbitrary values work.

## Components

```jsx
---
// components/TodoItem.mist
import { props } from 'mist'
const { todo, onToggle } = props({ todo: null })
---
<div onTap={() => onToggle(todo.id)}>
  <span>{todo.title}</span>
</div>
```

- `props({...})` destructures with defaults → WeChat `properties`. Values must
  be serializable.
- Props named `onXxx` are **callback props**: the child calls them like
  functions; they compile to `triggerEvent('xxx', { args })`, and the parent's
  `<TodoItem onToggle={toggle} />` auto-generates the `bind:toggle` wrapper that
  unwraps the arguments. Callback args must be serializable.
- `<slot/>` and `<slot name="x"/>` work; named slots auto-enable
  `multipleSlots`. Scoped slots and slot fallback content don't exist in WXML.
- Components use their own state/derived/lifecycles exactly like pages
  (`onAttach` → `attached`, `onDetach` → `detached`, `onReady` → `ready`).

**Automatic inlining:** a strictly pure-render component — data props only; no
state, derived, functions, callback props, events, slots, lifecycles, imports or
`config` — never becomes a WeChat component. It compiles to
a WXML `<template>` partial inlined into its parents: no instance overhead, no
JS, styles merged. This is invisible unless you look at `dist/`.

## Stores — shared state across pages

```ts
// stores/cart.ts — plain TypeScript
import { store } from 'mist'

export const cart = store({ items: [], total: 0 })

export function add(item) {
  cart.value.items.push(item)
  cart.value.total += item.price
}
```

```jsx
---
import { cart, add } from '../stores/cart.ts'
---
<span>{cart.value.total}</span>
```

Any page/component importing a store gets a live mirror: reads bind like local
state, imported functions work everywhere (including as event handlers), and
every mutation — from any page — arrives as a **path-precise batched `setData`**
on all subscribed pages. Subscription lifecycle (`onLoad`/`onUnload`,
`attached`/`detached`) is generated.

Store module rules: import only from `'mist'`; export only `store()` values and
functions; the same mutation compilation applies inside store functions.

## Styling

- **Tailwind v4** — the real `@tailwindcss/cli` runs over your class usage; the
  output is rewritten for WXSS (`rem`→`rpx` at 1rem = 32rpx, `oklch()`→hex,
  media queries preserved, `page{}` theme vars split into a pages-only sheet).
  Selectors WXSS can't express (`hover:`, `space-x-*`, …) are dropped with
  **M1006** warnings — never silently.
- `<style>` blocks compile to the unit's `.wxss` verbatim (no scoping yet).
- `app.mist`'s `<style>` becomes `app.wxss` (global; `page { … }` is valid there).

## Configuration & navigation

- `export const config = {...}` (static literals only) → the page/app `.json`.
  In `app.mist` it merges with the generated page list.
- Navigate with `<a href="/pages/about/about">` (→ `navigator`) or `wx.*` APIs
  directly — `wx` is fully available; Mist wraps nothing.

## Inputs

`<input value:bind={text} />` two-way binds: keystrokes render via native
`model:value` (no setData echo) while a generated handler keeps the logic-side
mirror and deriveds in sync. Manual `onInput` remains available.

## Not yet (roadmap — will error or misbehave today)

`[id].mist` file-based route params (query strings + `onLoad(({id}) => …)` work
today — see examples/ledger's detail page), calls in nested loops, `<style>`
scoping, `class:list`, npm imports in frontmatter, alias-mutation detection
(M1001). Tab bar/window config: put `tabBar` in `app.mist`'s `config` — no
separate config file needed.
