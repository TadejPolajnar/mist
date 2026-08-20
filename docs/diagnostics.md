# Diagnostics

Errors carry a code and a fix; M1001, M1004, M1007, M1011, M1013, M1017 and
M1021 and M1022 include file line:col and M1010 the line (other codes report the file
path only; M1015 reports line:col for import-shape errors and file path only
for the `pluginComponents` value/collision checks). Warnings (`M1002`,
`M1006`, `M1008`, `M1012`, `M1016`, `M1018`, `M1019`, `M1020`, `M1023`, `M1024`, `M1027`, `M1028`) go to stderr and
never fail the build. M1020 is the exception among warnings above in name
only — it is a warning when the tab bar file exists without the config flag,
but an error when the flag is set without the file (see below).

## M1001 — aliased state mutation

Writing state through a local alias compiles to nothing reactive — the write
happens in memory but no `setData` is emitted.

```ts
const t = todos.value[0]
t.done = true                 // ✗ M1001
todos.value[0].done = true    // ✓ path-precise setData
```

Applies to member/index writes, `++`/`--`, and mutating array calls
(`push` etc.) through an alias of a `state()` or `store()` value — including
`for...of` loop variables and callback params of `forEach`/`map`/`filter`/
`find`/`some`/`every`/`flatMap` over state paths. Read-only aliases are fine.
Tracking is scope-aware: a shadowing param or local only suppresses the check
inside its own function, and rebinding an alias (`t = other`) drops it.
Copies (`.slice()`, spread) are never aliases. Writes the compiler cannot see
(aliases passed into helpers) remain your responsibility: always write
through the full `x.value…` path.

## M1002 — unknown class (warning)

A template class produced no CSS. Usually a typo or a Tailwind utility that
doesn't exist. Classes defined in a user `<style>` block — a unit's own block
or `app.mist`'s global block — are harvested from the selector text and never
warn, so hand-written CSS classes are fine.

## M1003 — invalid list key

`wx:key` accepts only a **direct property** of the loop item, the item itself,
or `index`.

```jsx
{items.value.map(t => <li key={t.a + t.b}>…</li>)}   // ✗ computed
{items.value.map(t => <li key={t.meta.id}>…</li>)}   // ✗ deep path
{items.value.map(t => <li key={t.id}>…</li>)}        // ✓
{tags.value.map(t => <li key={t}>…</li>)}            // ✓ primitives → *this
{xs.value.map(x => <li key={index}>…</li>)}          // ✓ but disables keyed diffing
```

Fix: put a stable id on your items.

## M1004 — non-compilable array mutation

`pop / splice / shift / unshift / sort / reverse` on state can't compile to a
precise `setData` path.

```ts
items.value.splice(i, 1)                        // ✗ M1004
items.value = items.value.filter((_, j) => j !== i)   // ✓ one whole-key write
```

`push` and index assignment stay precise and are allowed.

## M1005 — name collision

State, derived, props, methods, store imports and store functions share one
namespace (they all become keys of the same object). Rename one of the two
reported declarations.

## M1006 — dropped selector (warning)

Tailwind produced a selector WXSS can't express (`hover:`, `:not()`, sibling
combinators like `space-x-*`, `@container`). The rule was removed. Fixes:
`space-x-* → gap-*`; interaction states → WeChat's `hover-class`; container
queries → `@media`.

## M1007 — reactive value used without `.value`

State, derived and store boxes are read and written through `.value` — a bare
reference compiles to nothing reactive, so it is rejected. Checked in
frontmatter (with line:col) and in template expressions, including inline
event handlers.

```ts
const count = state(0)
count++            // ✗ M1007
count.value++      // ✓
```

```jsx
<span>{count}</span>          // ✗ M1007 — renders nothing
<span>{count.value}</span>    // ✓
<input value:bind={text} />   // ✓ value:bind takes the box by design
```

Locals that shadow a reactive name (e.g. a parameter called `count`) are not
flagged within their own scope.

## M1008 — keyless list (warning)

A `.map()` over a reactive array without `key=` disables keyed field-level
diffing: every update resends the whole array instead of per-item paths.

```jsx
{items.value.map(t => <li>{t.text}</li>)}            // ⚠ M1008 — whole-array writes
{items.value.map(t => <li key={t.id}>{t.text}</li>)} // ✓ path-precise
```

## M1009 — call in a nested loop

Calls inside nested loops would be hoisted into a page-scope derived that
captures the outer loop variable out of scope. Precompute in frontmatter
instead — e.g. a derived that maps the nested items to display-ready values.

## M1010 — template syntax error

Mismatched/unclosed tags and malformed attribute names — reported with the file
line. Common cause: a self-closing native tag written without `/>`.

## M1011 — invalid `'mist'` import

Named imports from `'mist'` are validated against the real export list —
`state`, `derived`, `store`, `props`, and the lifecycle hooks. Unknown names
(usually a WeChat hook mist doesn't support, or a typo), aliased imports
(`state as s`), and default/namespace imports all error at compile time
instead of silently breaking at page load.

## M1013 — lifecycle hook in the wrong unit kind

Each hook belongs to one unit kind. Pages reject `onPageShow`/`onPageHide`
(use `onShow`/`onHide`) and reject component-only hooks (`onCreate`, `onMove`).
Components reject page-only hooks (`onPullDownRefresh`, `onReachBottom`,
`onPageScroll`, `onTabItemTap`, share/favorites hooks, `onRouteDone`,
`onSaveExitState`) — WeChat never delivers them to components. `app.mist`
accepts only `onLaunch`, `onShow`, `onHide`, `onError`, `onPageNotFound`,
`onUnhandledRejection`, `onThemeChange`. `onResize` works in both pages and
components (→ `pageLifetimes.resize`).

## M1017 — state write inside `onCreate`

`onCreate` maps to WeChat's `created`, which runs **before** `properties` and
`data` exist on the component instance — a `setData`-backed write there
targets an object that isn't there yet. Use `onCreate` only to seed
non-reactive instance fields (`this._foo = ...` outside state); move reactive
writes to `onAttach`.

```ts
import { state, onCreate } from 'mist'
const n = state(0)
onCreate(() => { n.value = 1 })          // ✗ M1017 — created runs before data exists
onCreate(() => { console.log(n.value) }) // ✓ reads are fine
```

## M1012 — config feature without its handler (warning)

`enablePullDownRefresh: true` without `onPullDownRefresh`, or
`onReachBottomDistance` without `onReachBottom`: the config passes through but
nothing can respond — e.g. the refresh spinner never stops. Declare the hook
or drop the config key.

## M1014 — config key collides with a generated field

Unit JSON is assembled by splicing the user's `config` object next to
compiler-generated fields — a duplicate key would silently win or lose,
depending on WeChat's parser, with no diagnostic. mistc rejects the
collision instead:

- `app.mist`: `pages` (generated from `src/pages/`), `subPackages`
  (generated from `src/packages/`), and `sitemapLocation` (always
  `sitemap.json`).
- Components: `component` (mistc marks every unit under `src/components/`
  as a component automatically).
- Pages and components: `usingComponents`, **only when the unit also
  imports a `.mist` component** — mistc registers those imports
  automatically and does not merge them with a manual entry.

A manual `usingComponents` with **no** `.mist` component imports still
works — it is the supported way to hand-register native/third-party
components mistc cannot discover on its own.

## M1015 — invalid plugin specifier or component

`plugin://<name>` imports and `config.pluginComponents` entries are validated:

- Only a default import of the whole plugin is supported —
  `import { x } from 'plugin://calendar'` errors.
- The plugin name must be non-empty and alphanumeric/`-`/`_` —
  `import p from 'plugin://'` errors.
- `config.pluginComponents` values must be string literals starting with
  `'plugin://'`.
- A `pluginComponents` name colliding with an imported `.mist` component's
  tag errors.

```ts
import cal from 'plugin://calendar'          // ✓
import { x } from 'plugin://calendar'        // ✗ M1015 — named import
import p from 'plugin://'                    // ✗ M1015 — empty name
```

## M1016 — pages in a subdirectory are not compiled (warning)

Pages must sit directly in `src/pages/`, or in `src/packages/<pkg>/pages/`
as a subpackage. A `.mist` file anywhere else under `pages/` is silently
skipped — this warning tells you where.

```
src/pages/sub/extra.mist    // ✗ M1016 — dropped, not a page or subpackage
src/pages/index.mist        // ✓ main package page
src/packages/shop/pages/cart.mist   // ✓ subpackage page
```

## M1018 — box-styled child inside a `text`-mapped element (warning)

Native `text` renders inline-only and ignores box styling (padding, flex,
etc). An element that maps to `text` (`span`, or a literal `text` tag) may
contain only text/`{expr}` children or other `text`-mapped elements —
anything else (a `view`, an `image`, …) compiles fine but silently no-ops its
box styles.

```jsx
<span class="p-4 flex"><div>x</div></span>   // ⚠ M1018 — div's padding/flex ignored
<span>hi {name.value}</span>                 // ✓
<span><span>nested</span></span>             // ✓
```

The check recurses through `wx:if`/`wx:else` and list children at the same
position, so a conditional or looped box element inside a `span` still warns.

## M1019 — unknown tag (warning)

A tag that is neither a native WeChat component, a web alias (`div`, `span`,
`img`, …), nor a registered `.mist` component/plugin component/manual
`usingComponents` entry compiles fine but renders as nothing — WeChat drops
unrecognized tags silently, with no error of its own.

```jsx
<scroll-veiw>x</scroll-veiw>   // ⚠ M1019 — did you mean <scroll-view>?
<swipper />                    // ⚠ M1019 — did you mean <swiper>?
<scroll-view>x</scroll-view>   // ✓ native
```

The suggestion only appears when a native tag or web alias is within edit
distance 2; otherwise the warning omits the "did you mean" clause. Each
distinct unknown tag warns once per unit, no matter how many times it
appears.

If the tag is intentional — a third-party component registered only through
`config.usingComponents`, or one you don't want mistc to know about yet —
list it in `config.customTags` to suppress the warning:

```ts
export const config = { customTags: ['my-web-component'] }
```

`customTags` entries must contain only letters, digits, `-` and `_`; the key
is consumed at compile time and never reaches the emitted `.json`.

## M1020 — custom tab bar file/config mismatch

WeChat requires a custom tab bar component at the fixed dist path
`custom-tab-bar/index.*`. mistc compiles `src/custom-tab-bar.mist` there when
present. The file and the `tabBar.custom: true` config flag must agree:

```ts
// tabBar.custom: true, but src/custom-tab-bar.mist is missing
export const config = { tabBar: { custom: true, list: [...] } }
// ✗ M1020 (error) — WeChat would render a blank tab bar
```

```ts
// src/custom-tab-bar.mist exists, but config lacks tabBar.custom: true
export const config = { tabBar: { list: [...] } }
// ⚠ M1020 (warning) — WeChat ignores the file and renders the built-in tab bar; build still succeeds
```

Set `tabBar: { custom: true, ... }` in `app.mist` config whenever
`src/custom-tab-bar.mist` exists, and remove the file (or the flag) otherwise.

## M1021 — unknown navigate() route

`navigate(route)`, `navigate.replace(route)` and `navigate.switchTab(route)`
require a literal route string, because the compiler checks it against the
compiled page list. An identifier, a template literal with interpolation, or
string concatenation all fail with the same message:

```ts
navigate('/pages/index/index')          // ✓ literal
navigate(`/pages/index/index`)          // ✓ literal (no interpolation)
navigate(someVar)                       // ✗ M1021 — not a literal
navigate(`/pages/${id.value}`)          // ✗ M1021 — not a literal
navigate('/pages/' + id.value)          // ✗ M1021 — not a literal
```

A literal route that isn't in the compiled page list also fails, with a
suggestion when a known route is within edit distance 3:

```ts
navigate('/pages/abot/abot')
// ✗ M1021: unknown route '/pages/abot/abot' — not in the compiled page
//   list; did you mean '/pages/about/about'?
```

`navigate.switchTab(route)` additionally requires the route to be a tab-bar
page when `app.mist`'s `config.tabBar.list[].pagePath` is statically
extractable (every entry a plain string literal); otherwise it falls back to
the plain route-list check above.

Route validation only runs for directory builds (`mistc build <dir>`),
because only those have a full page list — `mistc build <file>` (flat/
single-entry builds) compiles `navigate()` calls without checking the route
against anything.

## Un-coded errors worth knowing

- **"store modules require a project build"** — relative non-`.mist` imports
  only resolve in directory builds (`mistc build <dir>`).
- **"plain values cannot cross the page boundary"** — store modules export only
  `store()` values and functions; export a getter function instead of a const.
- **"app.mist cannot declare state / have a template"** — app shell is
  lifecycle + config + global styles only.
- **"config must be a static object literal"** — no function calls or variables
  inside `export const config`.
- **store output path collision** — two store files with the same stem; rename one.
- **"TS enum … is not supported"** — enums are runtime constructs; use a const
  object or a string-literal union.
- **"npm packages are not supported"** — only `mist`, relative store modules
  and `.mist` components can be imported.

## Silent hazard — aliased writes M1001 cannot see

M1001 catches direct writes through single-assignment local aliases. Writes it
cannot trace — an alias passed into a helper that mutates its parameter, or a
mutation inside a callback over a derived copy — still compile to nothing
reactive. Always write through the full `x.value…` path.

## M1022 — template-bound state initialized by frontmatter code

A `state()` initializer that calls a frontmatter function (or reads other
state) cannot seed `data` — the `data: {}` literal is evaluated before any
page code runs, so `this` does not exist there. Unbound state is fine: it
seeds in `onLoad`, where the call compiles to a method invocation.

```ts
function generate() { return [1, 2, 3] }

const items = state(generate())
```

```jsx
<span>{items.value.length}</span>          // ✗ M1022 — items is template-bound
```

Fixes: precompute into a module-level const (`const INITIAL = [...]` outside
any function), or keep the state unbound and render it through a `derived`:

```ts
const INITIAL = [1, 2, 3]
const items = state(INITIAL)               // ✓ const seed works everywhere
```

## M1023 — unknown event on a native tag

`onXxx` on a native tag compiles blindly to `bindxxx` — and WeChat silently
ignores events it doesn't know, so a typo'd handler simply never fires. Tags
in the compiler's metadata table (the ~25 everyday components) get their
events checked; tags outside it are skipped entirely.

```jsx
<scroll-view onScrolToLower={more} />   // ✗ M1023 — did you mean onScrollToLower?
<scroll-view onScrollToLower={more} />  // ✓
```

A custom event you know exists (newer base library, self-rendered tag) can be
allowed with `config.customAttrs = ['onMyEvent']`.

## M1024 — unknown attribute on a native tag

Same silent-failure class as M1023: WeChat ignores attributes it doesn't
recognize, so `scrol-y` just no-ops. Attributes are checked only for tags
present in the metadata table; `data-*`, `aria-*`, namespaced attrs
(`class:list`, `value:bind`, `mark:*`) and universal attrs (`class`, `style`,
`id`, `hidden`, `hover-*`, …) are always allowed.

```jsx
<scroll-view scrol-y />    // ✗ M1024 — did you mean scroll-y?
<scroll-view scroll-y />   // ✓
```

An attribute the table doesn't know yet (WeChat ships new ones quarterly) can
be allowed with `config.customAttrs = ['the-new-attr']` — staleness never
breaks a build; both codes are warnings.

## M1025 — route-param page missing its state

A `pages/<dir>/[<param>].mist` route page must declare the param as state —
the compiler seeds it from the query and guards against missing params, but
the declaration is yours (it fixes the type and the initial value):

```ts
// pages/item/[id].mist
const id = state('')          // ✓ seeded from the query before onLoad runs
```

Without it the page has nowhere to put the param — M1025, with the exact
declaration to add. Query params are strings; convert in a `derived` if you
need a number.

## M1026 — reactive value passed to an npm import

npm imports are supported, but they are an **opaque boundary**: the compiler
bundles the library and cannot see inside it, so a reactive value (state,
derived, or store mirror) passed as an argument could be mutated invisibly —
exactly the silent-staleness class the compiler exists to prevent.

```ts
import { format } from 'date-fns'
const when = state({ ts: 0 })

format(when.value, 'yyyy')          // ✗ M1026 — reactive object crosses the boundary
const ts = when.value.ts
format(ts, 'yyyy')                  // ✓ plain local copy in, plain value out
```

Return values are ordinary data — assign them to state freely. The check
covers direct calls, including member calls (`dayjs.utc(...)`); routing a
reactive value through an alias or callback is the same untracked frontier
M1001 documents.

## M1027 — feature exceeds config.minLibVersion

Opt-in: declare `config.minLibVersion` (the base-library minimum you set in
the WeChat admin console) — once in `app.mist` for the whole project, or
per unit to override — and the compiler checks every used native feature
with a documented minimum against it:

```ts
export const config = { minLibVersion: '2.9.0' }
```

```jsx
<scroll-view refresher-enabled />   // ✗ M1027 — refresher-enabled needs ≥ 2.10.1
<input value:bind={text} />         // ✗ M1027 — value:bind needs ≥ 2.9.3
```

Fix by raising `minLibVersion` (and the console setting) or dropping the
feature. The version table is curated and deliberately incomplete — features
without a recorded minimum are never checked, and without `minLibVersion` no
version checks run at all. Warning-tier: staleness never breaks builds.

## M1028 — bundled npm package references browser APIs

Every vendor bundle is scanned for globals WeChat's JS runtime doesn't have —
`window`, `document`, `navigator`, `localStorage`, `sessionStorage`,
`XMLHttpRequest`. A hit means the package will throw when that code path
runs on device:

```
M1028: npm package 'domish' references window, document — these APIs don't
exist in WeChat's JS runtime and fail when reached
```

The scan is a token heuristic: bare existence checks
(`typeof window !== 'undefined'`) pass untouched, but defensive member reads
(`if (window.matchMedia)`) still hit. If you've verified the package
degrades safely, allowlist it in `app.mist` — the key is app-level only and
rejected elsewhere:

```ts
export const config = { trustedPackages: ['fuse.js'] }
```

Warning-tier: the build still succeeds either way.
