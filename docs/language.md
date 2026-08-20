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

**Plain constants:** a top-level `const` that is not `state()`/`derived()` can
be referenced directly in the template (no `.value`) — `{TABS.map(t => …)}`
works for static lookup tables. Template-referenced consts are seeded into
`data` once as static values: no reactivity, no diffing, and later mutation
does nothing. Consts the template never references stay plain JS and never
enter `data`.

**Rules the compiler enforces:** mutate only through `x.value...` paths.
Using a reactive name without `.value` is an M1007 error. Aliasing into a
local and writing through it (`const t = todos.value[0]; t.done = true`) — or
mutating through a `for...of` variable or `forEach` callback param — is an
M1001 error with a fix-it. Array methods other than `push`/index assignment
are M1004 errors. A `.map()` without `key=` warns (M1008): it falls back to
whole-array writes.

## Templates

Web-familiar tags map to native ones; native tags pass through untouched:

| authored | emitted |
|---|---|
| `div section header footer main article ul ol li p h1–h6 nav aside` | `view` |
| `span` | `text` (bare text/`{expr}` emit unwrapped) — a box-styled child (`div`, etc.) inside it warns M1018, since native `text` ignores box styling |
| `img` | `image` |
| `a href="/pages/x/x"` | `navigator url="…"` |
| `button input scroll-view swiper …` | passthrough |

**Bindings** — `{expr}` → `{{expr}}` with `.value` stripped. Member access,
arithmetic, comparisons, ternaries, `&&`/`||`, string concat run inline in WXML.
**Anything WXML can't evaluate is hoisted automatically** — function calls
(including `Math.*` and calls with non-reactive arguments), template literals,
and optional chaining compile to generated deriveds recomputed per batch;
inside loops `{fmtDate(t.ts)}` becomes a computed field on each item (`t._c0`)
with keyed diffing preserved. Limits (M1009 errors, never silent): calls in
*nested* loops, and template literals/optional chaining that reference a loop
item — precompute those as computed fields or deriveds.

**Lists** — `.map()` with a **required key**:

```jsx
{todos.value.map(t => (
  <div key={t.id} onTap={() => toggle(t.id)}>{t.title}</div>
))}
```

`key` must be a direct property (`t.id`), the item itself (`key={t}` → `*this`
for primitives), or `index` (allowed, disables keyed diffing). Computed or deep
keys are **M1003** errors.

A second callback parameter binds the loop index and emits `wx:for-index`:

```jsx
{todos.value.map((t, i) => (
  <div key={t.id}>{i}: {t.title}</div>
))}
```

`key={i}` (the index param's own name) is also treated like `key={index}`.
Only `(item)` and `(item, index)` forms are supported — destructured params
and extra arguments are **M1010** errors.

**Conditionals** — `{cond && <jsx/>}` → `wx:if`. Non-JSX `&&` stays a binding.

`{cond ? <a/> : <b/>}` → `wx:if` / `wx:else`. Both branches must be JSX
elements (wrap a text branch in `<span>`, or use `&&` for a single branch).
A ternary in the `else` branch — `{a ? <x/> : b ? <y/> : <z/>}` — chains into
`wx:elif`. Non-JSX ternaries (`{cond ? 'a' : 'b'}`) stay a binding.

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

When you need both per-item args and the native event, bind a bare method and
carry the args yourself:

```jsx
{todos.value.map(t => (
  <input key={t.id} data-id={t.id} onInput={rename} />
))}
```

```ts
function rename(e) {
  const id = e.currentTarget.dataset.id
  const text = e.detail.value
}
```

(Compiler-generated inline-arrow args use the same mechanism under generated
`data-a0`, `data-a1`, … names.) A child component's callback-prop arguments
arrive the same way on the parent side — the generated wrapper unwraps
`e.detail.args` back into your handler's parameters automatically.

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
- `props<T>()` maps the type argument onto WeChat's `type:` field: `string` →
  String, `number` → Number, `boolean` → Boolean, `T[]`/tuples → Array, object
  type literals and type references (interfaces, `Record<...>`) → Object.
  Literal-union props (e.g. `'sm' | 'lg'`) map to their shared primitive type.
  Mixed unions, `any`/`unknown`, and other unresolvable types fall back to
  `type: null` (WeChat's own default — no conversion, no mismatch warning).
  A type alias to a primitive (e.g. `type Id = number`) still maps to Object,
  since aliases can't be resolved back to their underlying type.
- Props named `onXxx` are **callback props**: the child calls them like
  functions; they compile to `triggerEvent('xxx', { args })`, and the parent's
  `<TodoItem onToggle={toggle} />` auto-generates the `bind:toggle` wrapper that
  unwraps the arguments. Callback args must be serializable.
- `<slot/>` and `<slot name="x"/>` work; named slots auto-enable
  `multipleSlots`. Scoped slots and slot fallback content don't exist in WXML.
- Components use their own state/derived/lifecycles exactly like pages
  (`onCreate` → `created`, `onAttach` → `attached`, `onReady` → `ready`,
  `onMove` → `moved`, `onDetach` → `detached`). `onCreate` runs before
  `properties`/`data` exist — state writes there are a compile error (M1017);
  use it only to seed non-reactive instance fields. Pages get their own
  `onRouteDone` (fires after the route's enter animation) and
  `onSaveExitState` (return `{ data, expireTimeStamp? }` to hand WeChat a
  restore snapshot); both are page-only.

**Automatic inlining:** a strictly pure-render component — data props only; no
state, derived, functions, callback props, events, slots, lifecycles, imports or
`config` — never becomes a WeChat component. It compiles to
a WXML `<template>` partial inlined into its parents: no instance overhead, no
JS, styles merged. **Note the styling consequence**: an inlined component's
plain `<style>` merges into the parent page (page-wide scope), while a real
component gets WeChat's per-component style isolation — use `<style scoped>`
(see Styling) to keep an inlined component's classes to itself, or opt out of
inlining. This is otherwise invisible unless you look at
`dist/`. Opt out with
`export const config = { inline: false }` — the component then compiles as a
real `Component()`; the `inline` key is compiler-only and never reaches the
emitted `.json`.

Non-inlined components emit `"styleIsolation": "isolated"` by default; set
`styleIsolation` in `config` to override, for example `'apply-shared'` to let
page styles cascade in.

**Component options — `virtualHost`, `pureDataPattern`, `externalClasses`:**
component-only `config` keys, compiler-only (never reach the emitted `.json`):

```jsx
export const config = {
  virtualHost: true,
  pureDataPattern: '^_',
  externalClasses: ['x-class'],
}
```

- `virtualHost: true` removes the component's own wrapper node — the child's
  root elements render directly into the parent, with no extra layer. Useful
  for list-item-style components with real render cost. mistc does not change
  `styleIsolation` for you when you set this; check WeChat's docs for how
  `virtualHost` and `styleIsolation` interact before combining them.
- `pureDataPattern: '<pattern>'` marks data fields matching the regex as
  non-render fields — WeChat skips them for the WXML re-render pass. The
  string must not contain `/` or `\` (it compiles to a JS regex literal, e.g.
  `/^_/`, and a trailing backslash would escape the closing delimiter);
  compilation fails if it does — stick to simple prefixes like `'^_'`.
- `externalClasses: ['x-class', ...]` declares class slots a parent can fill:
  `<my-comp x-class="red-text" />` in the parent's template is plain WXML
  attribute passthrough — no special mist syntax. Each entry must be
  letters/digits/`-`/`_` only.
- All three are component-only; using them in a page or `app.mist` is a
  compile error.

**Bubbling callback events:** by default a callback event only reaches the
direct parent. Set `events` in `config` to add `triggerEvent` options for a
callback prop, so a grandchild can notify a grandparent without every
intermediate component forwarding the callback by hand:

```jsx
export const config = { events: { onToggle: { bubbles: true, composed: true } } }
```

This compiles to `this.triggerEvent('toggle', { args }, { bubbles: true, composed: true })`.
WeChat's `bubbles` lets the event climb through ancestor nodes; `composed`
is required in addition for it to cross a component boundary — without it,
`bubbles: true` alone stops at the enclosing component. `events` keys must
name a declared callback prop; the `events` key is compiler-only and never
reaches the emitted `.json`.

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

**Persistence** — opt in per store:

```ts
export const cart = store({ lines: [] }, { persist: 'app.cart', version: 2, migrate })

function migrate(old, oldVersion) {
  return { lines: old.lines || [] }
}
```

`persist` names the `wx` storage key. The store hydrates from storage at
module load; mutations write back debounced (~200 ms), with a final flush on
`wx.onAppHide`. When a saved envelope's `version` differs, `migrate(old,
oldVersion)` maps the old data to the current shape — its return value is
saved back immediately, and returning `undefined` falls back to `init`.
Without `migrate`, a version mismatch discards the saved data.

## Plugins — WeChat native components/APIs

WeChat plugins (maps, payment vendors, live streaming, customer service) are
opaque runtime externals — never reactive state, never bundled.

Declare the plugin in `app.mist`'s `config` (passes through verbatim):

```ts
export const config = {
  plugins: {
    calendarPlugin: { version: '1.0.0', provider: 'wx1234567890abcdef' },
  },
}
```

Import the plugin's JS surface with a default import from `plugin://<name>`:

```ts
import calendar from 'plugin://calendarPlugin'

function open() {
  calendar.select()
}
```

compiles to:

```js
const calendar = requirePlugin('calendarPlugin');
```

Only a default import of the whole plugin is supported — named imports
(`import { x } from 'plugin://...'`) and empty/invalid names are **M1015**
errors.

Register a plugin **component** for template use with `config.pluginComponents`
— extracted at compile time, merged into the generated `usingComponents`, and
never itself reaching the `.json`:

```ts
export const config = {
  pluginComponents: { calendar: 'plugin://calendarPlugin/calendar' },
}
```

```jsx
<calendar />
```

A `pluginComponents` name colliding with an imported `.mist` component's tag
is an M1015 error.

Any tag mistc can't place — not a native WeChat component, not a web alias,
and not registered through a `.mist` import, `pluginComponents`, or manual
`usingComponents` — gets an **M1019** warning (WeChat renders unknown tags as
nothing). Silence a tag you know is handled elsewhere with
`config.customTags`:

```ts
export const config = {
  customTags: ['my-web-component'],
}
```

`customTags` entries are consumed at compile time (letters/digits/`-`/`_`
only) and never reach the `.json`.

Declare your app's base-library floor to get since-version checks
(**M1027**) on top:

```ts
export const config = {
  minLibVersion: '2.11.0',
}
```

Declare it once in `app.mist` and every page and component (including
inlined ones) inherits the floor; a unit's own `minLibVersion` overrides it.
With it set, using a feature whose documented WeChat minimum is higher —
`refresher-enabled` needs 2.10.1, `value:bind` needs 2.9.3 — warns with the
exact versions. Without it, no version checks run. Compile-time only, never
reaches the `.json`; keep it in sync with the minimum you set in the WeChat
admin console.

Events and attributes on everyday native tags are checked the same way:
a typo'd `onScrolToLower` or `scrol-y` — which WeChat would silently
ignore — gets an **M1023**/**M1024** warning with a did-you-mean. Only tags
in the compiler's metadata table are checked, and `config.customAttrs`
(same shape as `customTags`) allowlists names the table doesn't know.

## Styling

- **Tailwind v4** — the real `@tailwindcss/cli` runs over your class usage; the
  output is rewritten for WXSS (`rem`→`rpx` at 1rem = 32rpx, `oklch()`→hex,
  media queries preserved, `page{}` theme vars split into a pages-only sheet).
  Selectors WXSS can't express (`hover:`, `space-x-*`, …) are dropped with
  **M1006** warnings — never silently.
- `bg-gradient-to-*`/`from-*`/`via-*`/`to-*` gradients interpolate in sRGB —
  color-interpolation hints (`in oklab`, …) are stripped for device
  compatibility with older WeChat webviews.
- `<style>` blocks compile to the unit's `.wxss` verbatim.
- **`<style scoped>`** scopes the block to its unit: every class selector gets
  a readable per-unit suffix (`.card` → `.card--todo-item`), rewritten
  identically in the WXSS and in the unit's markup (`class`, `hover-class`,
  `placeholder-class`,
  including string literals inside ternaries, `class:list`, template
  literals, and hoisted class expressions). This is what makes **inlined**
  components safe to style — their merged styles can no longer collide with
  the parent. `@media`/`@supports` bodies are scoped recursively;
  `@keyframes` and tag selectors are left untouched (keyframe names stay
  global). One limit: a class name **returned from a frontmatter function**
  (e.g. `class={cls()}` where `cls` builds the string) is not rewritten —
  keep scoped class literals in the template. `app.mist`'s style cannot be
  scoped — it is global by definition.
- `app.mist`'s `<style>` becomes `app.wxss` (global; `page { … }` is valid there).
- **Design tokens** — place `src/theme.css` next to `app.mist` to define
  Tailwind v4 tokens and custom utilities for the whole project:

  ```css
  @theme {
    --color-primary: #07c160;
    --text-cell: 17px;
  }
  @utility pb-safe {
    padding-bottom: env(safe-area-inset-bottom);
  }
  ```

  Templates then use `bg-primary`, `text-cell`, `pb-safe` like any utility.
  The file is spliced into the Tailwind build input; token definitions ship as
  `page { --… }` variables, so they cascade into components too. Do not
  confuse it with `theme.json` (the WeChat dark-mode file below). Theme edits
  invalidate the memoized CSS cache automatically.
- `class:list={[...]}` composes classes: string literals, `cond && 'classes'`,
  and `{ class: cond }` objects — all literals reach Tailwind generation, and
  conditionals compile to WXML ternaries. Use it *instead of* `class` on an
  element, not alongside it.

```jsx
<div class:list={['p-4', open.value && 'font-bold', { hidden: done.value }]} />
```

## Configuration & navigation

- `export const config = {...}` (static literals only) → the page/app `.json`.
  In `app.mist` it merges with the generated page list. The list orders
  `index` first (it becomes the launch page), then the rest alphabetically —
  set `entryPagePath` in `app.mist`'s `config` to launch a different page.
- `app.mist` accepts `onLaunch`, `onShow`, `onHide`, `onError`,
  `onPageNotFound`, `onUnhandledRejection`, `onThemeChange` — any other hook
  is rejected (M1013). The last four (`onError` through `onThemeChange`) are
  app-only; declaring them in a page or component is also rejected (M1013).
- Navigate with `<a href="/pages/about/about">` (→ `navigator`), `wx.*` APIs
  directly (`wx` is fully available; Mist wraps nothing), or the typed
  `navigate()` intrinsic below.
- Place a `sitemap.json` next to `app.mist` to control WeChat search
  indexing; without it, mistc emits an empty rule set.
- For dark mode, set `darkmode: true` and `themeLocation: "theme.json"` in
  `app.mist`'s `config`, and place `src/theme.json` next to `app.mist`. mistc
  copies it verbatim to `dist/theme.json`. Without a source file, no
  `theme.json` reaches `dist` and the build does not fail or warn.

### `navigate()` — typed routes

`import { navigate } from 'mist'` compiles route calls to the matching `wx.*`
navigation API, and — for directory builds (`mistc build <dir>`) — checks the
route against the compiled page list at compile time (M1021):

```ts
import { navigate } from 'mist'

navigate('/pages/detail/detail', { id: 3 })   // → wx.navigateTo({ url: '/pages/detail/detail' + <query> })
navigate.replace('/pages/detail/detail')       // → wx.redirectTo({ url: '/pages/detail/detail' })
navigate.back()                                // → wx.navigateBack()
navigate.back(2)                               // → wx.navigateBack({ delta: 2 })
navigate.switchTab('/pages/home/home')         // → wx.switchTab({ url: '/pages/home/home' })
```

The route argument must be a string literal (a plain template literal without
`${}` interpolation also counts) — the compiler needs to see the exact string
to check it. An identifier, an interpolated template literal, or string
concatenation all fail with M1021, as does a literal route that isn't in the
compiled page list. `navigate.switchTab` additionally requires the route to
be one of `app.mist`'s `tabBar.list[].pagePath` entries, when that list is a
static array of object literals with string `pagePath`s.

The optional `params` object (accepted by `navigate()` and
`navigate.replace()`) is serialized to a `?key=value&...` query string at
runtime and appended to the route; values pass through
`encodeURIComponent`.

Route checking only applies to directory builds — `mistc build <dir>` knows
the full page list (`src/pages/` + `src/packages/*/pages/`); a flat/
single-file build (`mistc build <file>`) has no page list to check against,
so `navigate()` calls there compile without route validation.

`mistc build` (directory builds only) also writes a `mist-routes.d.ts` file
next to an existing `mist.d.ts` at the project root (the file `mistc init`
scaffolds) — this narrows `navigate()`'s `route` parameter from `string` to a
union of every compiled page's route, so an unknown route is also a
type-checking error in your editor, not just a compile-time one. It's
regenerated on every build and skipped silently when there's no `mist.d.ts`
to sit next to. It's never written into `dist/` and never tracked in the
build manifest.

## Subpackages

Put pages under `src/packages/<pkg>/pages/*.mist` to build a WeChat
subpackage. Each `<pkg>` name must use only letters, digits, `-` or `_`, and
cannot be `pages`, `components`, `stores` or `assets` (those are reserved
dist paths). mistc discovers every `src/packages/<pkg>/pages/*.mist` file and
compiles it to `packages/<pkg>/pages/<name>/<name>.*`.

`app.mist`'s generated `app.json` splits pages by origin:

```json
{
  "pages": ["pages/index/index"],
  "subPackages": [
    { "root": "packages/shop", "name": "shop", "pages": ["pages/cart/cart"] }
  ]
}
```

- `pages` lists only main-package pages (`src/pages/*.mist`).
- `subPackages` groups subpackage pages by `<pkg>`; each entry's `pages` list
  is root-relative (`pages/cart/cart`, not `packages/shop/pages/cart/cart`).
- The main package needs at least one page in `src/pages/` — a project with
  only subpackage pages fails to build.
- `subPackages` is compiler-generated: setting it in `app.mist`'s `config`
  is rejected (M1014).

**`preloadRule`**: not generated by mistc — set it yourself in `app.mist`'s
`config` and it passes straight through to `app.json`:

```ts
export const config = {
  preloadRule: {
    "pages/index/index": { network: "all", packages: ["shop"] },
  },
}
```

**Dynamic loading**: set `lazyCodeLoading: "requiredComponents"` in
`app.mist`'s `config` (passthrough, not generated) — WeChat recommends it for
most projects. `componentPlaceholder` in a page's `config` also passes
through unchanged.

**Not supported today**: async-loaded subpackage *components* (a component
declared inside `src/packages/<pkg>/` that WeChat loads lazily with the
subpackage). A component imported by a subpackage page compiles once, at its
usual main-package path (`components/<k>/<k>.*`) — the subpackage page
references it with a depth-4 relative path, same as it references
`mist-rt.js`. There is no way today to declare a component that ships
*inside* a subpackage and loads only when that subpackage does. Independent
subpackages are also unsupported: they cannot `require` the main-package
runtime (`mist-rt.js`), which every compiled unit depends on.

## Assets

`src/assets/**` is copied verbatim to `dist/assets/**` on every build of a
project directory (not for flat single-file builds). Reference the files
from templates or `app.mist` config as `/assets/...` or a relative path, for
example `tabBar.list[].iconPath: "assets/tab-home.png"`. Hidden files (names
starting with `.`) are skipped. Removed source files are pruned from `dist`
on rebuild. Symlinks inside `assets/` are skipped.

## Workers

Set `workers: "workers"` in `app.mist`'s `config` to enable a WeChat worker
thread, and put plain JS files under `src/workers/**`. mistc mirrors the
directory verbatim to `dist/workers/**` on every build of a project directory
(not for flat single-file builds) — no compilation, no validation of the JS.
Removed source files (and emptied subdirectories) are pruned from `dist` on
rebuild.

## Custom tab bar

Put `src/custom-tab-bar.mist` in the project to build a branded tab bar.
mistc compiles it to the fixed dist path WeChat requires:
`custom-tab-bar/index.{js,json,wxml,wxss}`. It compiles like any other
component — state, imports of shared `.mist` components, and store imports
all work.

`app.mist`'s config must set `tabBar.custom: true` alongside the usual
`tabBar.list`:

```ts
export const config = {
  tabBar: {
    custom: true,
    list: [
      { pagePath: "pages/index/index", text: "Home", iconPath: "assets/tab-home.png" },
      { pagePath: "pages/cart/cart", text: "Cart", iconPath: "assets/tab-cart.png" },
    ],
  },
}
```

Each page that shows the tab bar calls `getTabBar()` in its own logic (a
native WeChat API, not a Mist wrapper) to sync the active tab, typically in
`onShow`:

```ts
import { onShow } from 'mist'

onShow(() => {
  const tabBar = getTabBar()
  if (tabBar) {
    tabBar.setData({ active: 0 })
  }
})
```

`wx.*` tab-bar APIs (`wx.setTabBarItem`, `wx.showTabBar`, …) remain available
unwrapped, same as everywhere else in Mist.

The file and the config flag must agree, or the build warns/errors (M1020):
`tabBar.custom: true` without the file is an error (WeChat would render a
blank tab bar); the file without the flag is a warning (WeChat ignores the
file and renders the built-in tab bar).

## Two-way binding

`<prop>:bind={state}` two-way binds a state to a native element property.
Changes render via native `model:<prop>` (no setData echo) while a generated
`__vb_<state>` handler keeps the logic-side mirror and deriveds in sync.
Manual event handlers (`onInput`, `onChange`) remain available instead.

| `<prop>:bind` | model attribute | companion event |
|---|---|---|
| `value:bind` | `model:value` | `bindinput` |
| `checked:bind` | `model:checked` | `bindchange` |

Example: `<input value:bind={text} />`, `<switch checked:bind={on} />`.

Only these two properties are supported. Any other `<ident>:bind` (for
example `foo:bind`) is a compile error. Two-way binding on `.mist` child
components (`model:` on a custom component) is not supported yet.

## Route-param pages — `pages/item/[id].mist`

A detail page can declare its query param in the filename:
`pages/item/[id].mist` compiles to the ordinary route `pages/item/item`
(WeChat has no dynamic paths — this is **sugar over query params**, nothing
more). The frontmatter must declare `const id = state(...)`; the compiler
then generates what every detail page otherwise hand-writes:

- a **missing-param guard** — `onLoad` without `id` logs an error and
  `wx.navigateBack()`s instead of rendering a broken page;
- **seeding** — `id.value` is set from the query before your `onLoad` body
  (which you usually no longer need) runs;
- a **typed route entry** — `mist-routes.d.ts` gains a `RouteParams` entry, so
  `navigate('/pages/item/item', { id })` requires the param and
  `navigate('/pages/item/item')` is a type error.

Query params arrive as strings — convert in a `derived` if you need numbers.
One `[param].mist` per directory; `pages/item.mist` alongside
`pages/item/[id].mist` is a compile error (they'd collide). Works in
subpackages too (`packages/<pkg>/pages/<dir>/[id].mist`). See
examples/portfolio's position page.

## npm imports — `import dayjs from 'dayjs'`

Bare npm imports work in pages, components and store modules.
`npm install` your dependency in the project root; the compiler bundles each
imported package into a self-contained `dist/vendor/<pkg>.js` via esbuild
(installed once into `~/.cache/mistc/`, like Tailwind) and emits plain
`require`s. Default, named and subpath imports are supported; `* as`
namespace imports are not.

The catch is deliberate — **npm code is an opaque boundary**. The compiler's
whole thesis is tracking every mutation of reactive state; it cannot see
inside a bundled library. So passing a reactive value (state, derived, or
store mirror) as an argument to an imported function is a compile error
(**M1026**). Copy what the function needs into a plain local first:

```ts
import { format } from 'date-fns'

const when = state({ ts: 0 })
const label = state('')

function f() {
  format(when.value, 'yyyy')       // ✗ M1026 — reactive object crosses the boundary
  const ts = when.value.ts
  label.value = format(ts, 'yyyy') // ✓ primitive copy in, plain return value out
}
```

Return values are plain data — assigning them to state is fine. The check
covers direct calls (including member calls like `dayjs.utc(...)`); routing
a reactive value through an alias or callback is the same untracked frontier
M1001 documents. Bundling runs in project builds only — single-file builds
emit the `require` but no vendor file. Bundled output is scanned for browser
APIs WeChat lacks (`window`, `document`, `navigator`, `localStorage`, …) —
hits warn with **M1028**. The scan is a heuristic — bare existence
checks (`typeof window`) pass, but member reads used defensively
(`if (window.foo)`) still hit — so allowlist verified-safe packages in
`app.mist` (app-level only; the key is rejected elsewhere):

```ts
export const config = { trustedPackages: ['fuse.js'] }
```

## Not yet (roadmap — these error cleanly today)

Calls in nested loops (M1009). Tab bar/window config:
put `tabBar` in `app.mist`'s `config` — no separate config file needed.

TypeScript annotations (param/return types, `interface`, `type`, `as`,
generics, `import type`) are stripped before emission — annotate freely; none
of it reaches the generated JS. `enum` is the exception: it is a runtime
construct and is rejected with a fix-it (use a const object or a
string-literal union).
