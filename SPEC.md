# Mist — Language Spec (Draft 0.2)

> Working name: **Mist** (Mini-app Static Templates). A single-file component language,
> Astro-flavored syntax, compiled by a Rust compiler directly to WeChat Mini Program
> WXML / WXSS / JS with near-zero runtime. Target: simple apps, maximum performance.
>
> Changelog 0.2: incorporates review findings — derived-list keyed diffing, corrected
> element mapping, per-component Tailwind emission, per-item hoisting, stricter aliasing
> rules, `store()` for shared state, callback-prop wire format, platform limits, resolved
> open questions.

## Design principles

1. **Everything statically analyzable.** No dynamic component types, no eval-like patterns.
   If the compiler can't see it, it's a compile error with a clear diagnostic — never a
   silent slow path.
2. **The compiler's job is minimizing `setData`.** State mutations compile to exact
   data-path writes. No tree diffing at runtime; the single exception is a keyed shallow
   diff for derived arrays (§3.2), which is O(n) reference comparison, not reconciliation.
3. **Zero-ish runtime.** A fixed helper library < 3KB min (setData batching, event arg
   unwrap, keyed array diff). App code compiles to plain `Page()` / `Component()` modules.
4. **Web-familiar authoring.** HTML-like tags, Tailwind classes, TS in frontmatter.

---

## 1. File model

```
src/
  app.mist          # app-level: global config, app lifecycle, global styles
  pages/
    index.mist      # page (route: /pages/index/index)
    todo/[id].mist  # page with route param → onLoad query param `id`
  components/
    TodoItem.mist   # component (PascalCase filename required)
  stores/
    user.ts         # shared state modules (plain TS, uses store())
mist.config.ts      # compiler config: tailwind, rpx ratio, subpackages, renderer
```

- `pages/**` compile to mini program **pages** (`Page()` + entry in `app.json`).
- `components/**` compile to **custom components** (`Component()`) — or are **inlined**
  at the call site when profitable (§8.3).
- `app.mist` emits `app.js` / `app.json` / `app.wxss`.
- **Renderer target:** WebView only in v1. `renderer` key reserved in `mist.config.ts`
  for a future Skyline mode (stricter WXSS subset, different scroll model).

## 2. Single-file component anatomy

```astro
---
import TodoItem from '@components/TodoItem.mist'
import { state, derived, onShow } from 'mist'

export const config = { navigationBarTitleText: 'Todos' }  // → index.json

const filter = state<'all' | 'open'>('all')
const todos = state([{ id: 1, title: 'Ship spec', done: false }])

const visible = derived(() =>
  filter.value === 'all' ? todos.value : todos.value.filter(t => !t.done)
)

function toggle(id: number) {
  const i = todos.value.findIndex(t => t.id === id)
  todos.value[i].done = !todos.value[i].done   // → setData({'todos[i].done': ...})
}

onShow(() => { /* page lifecycle */ })
---
<div class="p-4 flex flex-col gap-2">
  <span class="text-lg font-bold">Todos ({visible.value.length})</span>

  {visible.value.map(t => (
    <TodoItem key={t.id} todo={t} onToggle={() => toggle(t.id)} />
  ))}

  {visible.value.length === 0 && <span class="text-gray-400">Nothing here</span>}

  <button onTap={() => filter.value = filter.value === 'all' ? 'open' : 'all'}>
    Filter: {filter.value}
  </button>
</div>

<style>
  /* optional; scoped by default; compiled to WXSS alongside Tailwind output */
  button { border-radius: 8rpx; }
</style>
```

## 3. Frontmatter semantics

| Construct | Compiles to |
|---|---|
| `state(init)` | key in `data`, mutations → path-precise `setData` |
| `derived(fn)` | key in `data`, recomputed only when a tracked dependency changes; arrays keyed-diffed (§3.2) |
| `props<T>()` (components only) | `properties` block (§5) |
| plain `const`/`let` never mutated after init and not read reactively | module-scope constant |
| `let` mutated but never read by a template | instance field — **never enters `data`, zero bridge cost** |
| functions | methods on the Page/Component object |
| lifecycle hooks `onLoad/onShow/onHide/onUnload/onReady` (pages), `onAttach/onDetach` (components) | corresponding handlers |
| `export const config = {…}` | page/component `.json` (static object literal only) |

### 3.1 Reactivity — the core contract

`state()` returns a box; reads are `x.value`, writes are assignments through a
statically-visible path rooted at `x.value`:

```ts
count.value++                    // setData({ count: <new> })
user.value.name = 'Ada'          // setData({ 'user.name': 'Ada' })
todos.value[i].done = true       // setData({ [`todos[${i}].done`]: true })
todos.value.push(item)           // setData({ [`todos[${len}]`]: item })
```

**Mutation rules (compile errors when violated):**

- A write to state is legal **only** as a direct assignment expression whose target path
  is rooted at `x.value`, or a whitelisted mutating call on such a path
  (`push`, `pop`, `splice`, index assignment). `sort`/`reverse`/in-place `filter`:
  reassign `x.value = …` instead (one full-key write).
- **State-derived object values are read-only everywhere else.** Aliasing into a local,
  passing to a function, storing into another state, or mutating inside a callback
  (`forEach`, event handler closures over the alias) — any write the compiler cannot
  trace to a root path is error `M1001` with a direct-path fix-it. Passing state values
  to helpers is fine as long as the helper only reads; the compiler checks helpers
  defined in-project and conservatively rejects writes through parameters it cannot see
  (external functions receive a deep-clone in debug builds to catch violations early).
- Mutations are legal only in code the compiler owns (frontmatter functions, lifecycles,
  store modules).
- All writes within one synchronous task are **batched into a single `setData`** with
  merged keys. If a batch exceeds the platform budget (§10: 1MB/call, ~256KB/key), the
  runtime chunks it and the compiler warns at the site of large full-key reassignments.

### 3.2 `derived()` — including the list problem

Dependencies are determined **at compile time** from `state`/`store` reads inside the
function. On any mutation to a dependency, the derived is recomputed in the same batch.
How the result is written depends on its type:

- **Scalars/objects:** written as a whole key (skipped if `===` unchanged).
- **Arrays rendered with a keyed `map`:** the runtime performs a **keyed shallow diff**
  against the previous value (O(n) over the key property, reference-equality per item)
  and emits item-path writes only for changed/inserted/removed positions. Toggling one
  todo therefore sends `{'todos[3].done': true, 'visible[1].done': true}` — not the
  whole filtered array. Reordering/insertion falls back to writing from the first
  changed index. Arrays without a template key are written whole (with a compile warning).

This is the only runtime "diff" in Mist; it exists because filter/sort/map deriveds are
how real lists are written, and without it the path-precision claim would only hold for
raw state.

### 3.3 `store()` — shared state across pages

Defined in plain `.ts` modules under `stores/`:

```ts
// stores/user.ts
import { store, derived } from 'mist'
export const user = store({ name: '', vip: false })
export const greeting = derived(() => `Hi ${user.value.name}`)
```

Same read/write API and mutation rules as `state()`. The compiler tracks which pages and
components read which store keys and emits glue: subscribe on `onShow`/`attached`, apply
pending changes as one `setData`, live pages receive path-precise writes on mutation,
unsubscribe on hide/detach. Stores live in the logic thread as module singletons — no
`getApp()` juggling.

## 4. Template language

HTML-like, but a strict dialect parsed by the compiler — not HTML.

### 4.1 Elements

| Authored | Emitted | Notes |
|---|---|---|
| `div`, `section`, `header`, `footer`, `ul`, `li`, `p`, `h1–h6`, … (all block elements) | `view` | headings get default type-scale classes (removable) |
| `span`, bare text runs | `text` | **`text` may contain only text/`span` children — M1018 compile warning otherwise, may harden to an error later** (native `text` is inline-only and ignores box styling) |
| `img` | `image` | `mode` passthrough |
| `button`, `input`, `textarea`, `form`, `label` | same-named native components | |
| `a href="/pages/x/x"` | `navigator url="…"` | |
| `scroll-view`, `swiper`, `picker`, `video`, `map`, native tags | passthrough | |

### 4.2 Expressions `{…}`

Three tiers, decided per-expression by the compiler:

1. **WXML-native:** member access, arithmetic, comparisons, ternary, `&&`/`||`, string
   concat → inline `{{…}}`.
2. **Hoisted:** other page-scope expressions (function calls, template literals, optional
   chaining) → auto-generated `derived()` with a **deterministic source-derived name**
   (e.g. `_d$title_fmt`) visible in emitted JS and DevTools.
3. **Per-item hoisted:** expressions inside a `map` that depend on the loop item
   (`{formatDate(t.createdAt)}`) → the compiler injects **computed fields** into the
   list's `data` representation (`t._c$createdAt_fmt`), recomputed per item on that
   item's path writes and during keyed diffs. This is v1 — loop-body formatting is the
   common case, not an edge case.

No WXS. Render-thread expressions stay trivial; work happens once per change in the
logic thread.

### 4.3 Control flow

| Authored | Emitted |
|---|---|
| `{cond && <x/>}` | `wx:if` |
| `{cond ? <a/> : <b/>}` | `wx:if` / `wx:else` |
| `{list.map(item => <x key={item.id}/>)}` | `wx:for` + `wx:key` |

- `key` is **required** and restricted to what `wx:key` accepts: a direct property of the
  item (`key={item.id}`) or the item itself (`key={item}` → `*this`). Computed keys
  (`key={a + b}`) are a compile error; `key={index}` allowed with warning (disables
  keyed diffing for that list).
- Arbitrary statements in templates (IIFEs, `let`) are compile errors — logic lives in
  frontmatter.

### 4.4 Events

| Authored | Emitted |
|---|---|
| `onTap={fn}` | `bindtap="fn"` |
| `onTap:catch={fn}` | `catchtap="fn"` |
| `onTap:mut={fn}` | `mut-bind:tap="fn"` |
| `onInput`, `onChange`, `onConfirm`, `onScroll`, … | `bindinput`, `bindchange`, … |
| `onClick` (web alias) | `bindtap` |

Inline arrows with statically-known args — `onTap={() => toggle(t.id)}` — compile to a
generated method plus `data-` attributes carrying the args. The compiler **emits
lowercase-only dataset keys** (WXML lowercases them anyway) and always binds values via
`{{…}}` so types survive (a bare `data-id="3"` would arrive as a string). The runtime
helper unwraps args before invoking `toggle`. Event objects are native WeChat shapes
(`e.detail`, `e.currentTarget`), typed via `miniprogram-api-typings`.

### 4.5 Two-way input binding

`value:bind={text}` on `input`/`textarea` compiles to native `model:value="{{text}}"`
(base lib ≥ 2.9.3, declared as minimum in emitted config). This avoids the
setData-echo cursor-jump of manually controlled inputs. Manual `onInput` remains
available for transform-on-input cases; the compiler never round-trips input value
through `setData` on its own.

## 5. Components

```astro
---
// TodoItem.mist
import { props } from 'mist'
const { todo, onToggle } = props<{ todo: Todo; onToggle: (id: number) => void }>()
---
<div class="flex gap-2" onTap={() => onToggle(todo.id)}>
  <span class={todo.done ? 'line-through text-gray-400' : ''}>{todo.title}</span>
</div>
```

- **Props → `properties`.** WeChat property types are String/Number/Boolean/Object/Array/
  null only; the compiler maps the `props<T>()` type argument onto them — shipped:
  `string`/`number`/`boolean` and their literal-union forms, `T[]`/tuples → Array,
  object type literals and type references (interfaces, `Record<…>`) → Object; mixed
  unions and unresolvable/generic types fall back to `type: null`. Type-alias references
  to a primitive (e.g. `type Id = number`) also map to Object, not Number — the compiler
  cannot resolve aliases. Not shipped: a runtime dev-mode validator. Defaults via
  `props<T>({ done: false })`. Non-serializable prop values are compile errors —
  **except** `onXxx`-named function props.
- **Callback props** compile to component events. Wire format: child calls
  `onToggle(todo.id)` → emitted as `this.triggerEvent('toggle', { args: [todo.id] })`;
  the call site's `bind:toggle` handler unwraps `e.detail.args` and invokes the parent
  closure with the original arguments. Callback parameters must be serializable
  (compile error otherwise). Parent-side `e`-style event objects are not passed through —
  callbacks carry only their declared args.
- **Children:** `<slot/>` and `<slot name="x"/>` → WXML slots (`multipleSlots` auto).
  **WXML slots have no scoped slots and no fallback content** — both are compile errors
  with diagnostics, not silent differences (Vue/Astro users will expect them).
- No dynamic component types — compile error; escape hatch is an explicit conditional
  over statically known components.

## 6. Styling

- `class` accepts static strings, expressions, and `class:list={[…]}` (object syntax
  `{ cls: cond }` supported; compiled to a hoisted/per-item derived string).
- **Tailwind pipeline:** compiler scans class usage per component → runs Tailwind
  (CLI/oxide) → post-processes with lightningcss:
  - class-name sanitization for WXML/WXSS (`md:flex` → `md_flex`, `w-[32px]` → `w-_32px_`,
    `bg-black/50` → `bg-black_50`), rewritten consistently in markup and CSS;
  - `rem` → `rpx` (default `1rem = 32rpx`, configurable);
  - preflight stripped; minimal WXSS reset injected.
- **Style isolation (the part that silently breaks everyone else):** custom components
  default to `styleIsolation: isolated`, so page/app-level Tailwind never reaches them.
  Mist emits a **shared generated utility file** imported (`@import`) by each component's
  `.wxss` containing exactly the utilities that component's template uses (tree-shaken,
  deduped through the shared file so package size doesn't multiply), and sets
  `styleIsolation` explicitly. Components are self-sufficient for styling by
  construction.
- **Unsupported utilities are compile errors with fix-its**, not silent drops:
  `space-x-*`/`space-y-*`/`divide-*` (need `:not()` + child combinators — unreliable in
  WXSS) → "use `gap-*`"; universal/attribute selectors; `hover:` (dropped with warning;
  `button` hover maps to `hover-class` where possible).
- `<style>` blocks: scoped per component via class-prefix rewriting → component `.wxss`;
  `<style global>` opts out.

## 7. App shell & routing

- `app.mist`: `onLaunch`/`onShow` hooks; `export const config` merged with the discovered
  page list into `app.json`. Tab bar, window style, subpackages in `mist.config.ts`.
- `import { navigate } from 'mist'` — `navigate('/pages/todo/[id]', { id: 3 })` →
  `wx.navigateTo`, typed route table generated from `pages/`. Also `navigate.replace`,
  `.back`, `.switchTab`. Page-stack depth (10) exceeded → dev-mode warning.
- `wx.*` is not wrapped (non-goal). Types come from **`miniprogram-api-typings`**
  (official, maintained) as an ambient global: `mistc init` scaffolds it as a
  devDependency and adds it to the tsconfig `types` array, so `wx.*` is typed
  after `npm install`. There is no `mist/wx` module — the import guard admits
  only `mist`, relative store modules, and `.mist` components (decided:
  ambient global over re-export, §14).

## 8. Compiler-level optimizations (the performance thesis)

1. **Path-precise setData** for state (§3.1) + **keyed diff** for derived lists (§3.2).
2. **Batching** — one `setData` per synchronous task, merged keys, budget-aware chunking.
3. **Component inlining** — v1 auto-inlines only components with **no own state, no
   lifecycle, no slots, and no callback props** into parent WXML `<template>` partials
   (templates can't hold slots; callback rewiring would need a second codegen path —
   deferred). Styles merge with collision-free prefixes. Opt out:
   `export const config = { inline: false }`.
4. **Static hoisting** — binding-free subtrees emit plain WXML, invisible to updates.
5. **Dead-data elimination** — state never read by a template stays an instance field
   (§3), out of `data`, zero bridge cost.
6. **List discipline** — required keys + item-path writes + keyed derived diffs: toggling
   one row in a 1000-row filtered list sends two short key paths, not an array.

## 9. Emitted output

`pages/index.mist` →

```
dist/pages/index/index.wxml
dist/pages/index/index.wxss   # @import shared tailwind subset + scoped styles
dist/pages/index/index.js     # Page({...}) — deliberately readable, stable names
dist/pages/index/index.json
dist/styles/tw-shared.wxss    # deduped generated utilities
dist/mist-rt.js               # <3KB: batchSetData, keyed diff, event arg unwrap, store glue
```

**Readable output is a feature:** WeChat DevTools won't load external source maps, so
runtime debugging happens in generated JS. Generated methods and derived keys keep
source-derived names; `mist trace` translates a pasted runtime stack back to `.mist`
locations. Compile-time diagnostics use real source maps.

## 10. Platform limits the compiler enforces

- `setData`: ≤1MB per call, keep individual keys well under 256KB — runtime chunks,
  compiler warns at large full-array reassignment sites.
- `wx:key`: property name or `*this` only (grammar restricted at the `key` prop, §4.3).
- Main package ≤2MB, total ≤20MB — build fails over budget with a size report per page;
  subpackage mapping in `mist.config.ts`.
- Page stack ≤10; `navigateTo` beyond depth warns in dev.
- Base library minimum: 2.9.3 (for `model:value`), declared in emitted config.

## 11. Diagnostics philosophy

Every rejected pattern gets a numbered error with a "write it this way instead" fix-it,
pointing at original `.mist` source. The three errors users will hit first get the most
polish: `M1001` aliased state mutation, `M1002` unsupported Tailwind utility,
`M1003` non-property list key. This is the DX moat versus Taro 2's cryptic-lint failure.

## 12. v1 DX surface

- **Data loading idiom (blessed pattern, no framework):**
  ```ts
  const order = state<Order | null>(null)
  const loading = state(true)
  onLoad(async ({ id }) => {
    order.value = await api.getOrder(id)   // typed wx.request wrapper in userland
    loading.value = false
  })
  ```
  Async functions are supported anywhere; each `await` resumption flushes its own batch.
- **Editor/LSP:** `.mist` requires a language server regardless of syntax choices —
  frontmatter TS is delegated to tsserver via virtual files (Volar-style), template gets
  completions for tags/props/Tailwind and inline diagnostics. VS Code extension is part
  of v1; boxes-not-magic keeps the TS side plain.
- **npm interop:** frontmatter/store modules may import npm packages; the compiler
  bundles them (no `miniprogram_npm` dance) and fails with a diagnostic on packages that
  touch DOM/Node APIs. *Shipped design (boundary rule, spike 030-C): pages/components
  may bare-import; esbuild bundles to `dist/vendor/`; reactive values crossing into
  imported functions are M1026 errors — copy into plain locals first. Store-module
  imports ship; bundled output is scanned for browser APIs WeChat lacks
  (M1028 warning, `trustedPackages` allowlist). A `raw()` escape hatch
  remains future work.*
- **Testing:** compiler snapshot tests (`.mist` → emitted WXML/JS) are first-class
  (`mistc test --snapshots`); logic in stores/helpers is plain TS, unit-testable with
  vitest without WeChat.

## 13. Non-goals (v1)

- Full React/JSX semantics, hooks, context. (The constraint *is* the product.)
- Runtime-dynamic component trees, portals, scoped slots, slot fallbacks.
- CSS-in-JS.
- Cross-platform output — WeChat-only by design (no internal emitter abstraction exists; the performance thesis is built on WeChat's setData cost model).
- Skyline renderer (config key reserved).
- Build-time data/SSR — all data is runtime.

## 14. Resolved design questions

1. **Boxes (`x.value`) over `$state` compiler magic** — reactive reads/writes stay
   syntactically visible, which keeps path tracking and `M1001` diagnostics teachable;
   magic sugar can layer on in v2 once the LSP is mature.
2. **Two-way binding: yes** — `value:bind` → native `model:value` (§4.5).
3. **Per-item derived/hoisting: in v1** (§4.2 tier 3) — loop-body formatting is the
   common case.
4. **Hoisted expressions are nameable/visible** — deterministic source-derived names,
   because runtime debugging happens in generated code (§9).
5. **`wx.*` types: `miniprogram-api-typings` as ambient global, not re-exported** —
   `mistc init` scaffolds it as a devDependency and tsconfig `types` entry;
   no `mist/wx` module exists, since the import guard only admits `mist`,
   stores, and components, and that strictness is load-bearing for the
   static-analysis thesis (§7).
