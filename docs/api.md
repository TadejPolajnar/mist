# API reference

## The `'mist'` module

Everything importable from `'mist'`. These are **compiler intrinsics** — they
exist at compile time and are rewritten away; there is no `mist` package at
runtime.

### `state(initial)` → box

Reactive page/component state. Read `x.value`; write through `x.value...` paths.
Bound state becomes a `data` key; state never read by the template becomes an
instance field (no bridge cost). Initial value: any expression.

### `derived(fn)` → box (read-only)

`const open = derived(() => todos.value.filter(t => !t.done))` — recomputed once
per update batch; dependencies are whatever state/store boxes the arrow reads.
Arrays rendered with a template `key` get keyed field-level diffing. Not allowed
in store modules.

### `props(defaults?)` → destructured props (components only)

`const { todo, onToggle } = props({ todo: null })` — must be destructured.
Plain names → `properties` (serializable values). `onXxx` names → callback
props (child calls them; parent receives a component event, wiring generated).

### `store(initial)` → shared box (store modules only)

Declared in `stores/*.ts` as `export const cart = store({...})`. Same read/write
contract as `state`. Every importing page/component gets a subscribed mirror
with path-precise batched updates.

Optional persistence: `store(init, { persist: 'key', version?: 1, migrate? })`
hydrates from `wx.getStorageSync('key')` at creation and writes back debounced
(~200 ms) on mutation, wrapped in a `{ v, data }` envelope. When the saved
version differs, `migrate(oldData, oldVersion)` produces the new shape (no
`migrate` ⇒ saved data is ignored and `init` is used). Storage errors are
swallowed — persistence is best-effort. Mind wx storage quotas (~1 MB/key).
A pending debounced write flushes on `wx.onAppHide`; a hard kill inside the
200 ms window can still lose the last mutation.

### Lifecycle hooks

Call at frontmatter top level with an arrow: `onLoad(({ id }) => { ... })`.
Async arrows supported.

| hook | pages | components (mapped) | app.mist |
|---|---|---|---|
| `onLoad` | ✓ (init + store-bind injected) | — | — |
| `onShow` / `onHide` | ✓ | — | ✓ |
| `onReady` | ✓ | `ready` | — |
| `onUnload` | ✓ (store-unbind injected) | — | — |
| `onAttach` / `onDetach` | — | `attached` / `detached` (init/bind injected) | — |
| `onLaunch` | — | — | ✓ |
| `onPullDownRefresh` / `onReachBottom` | ✓ | — | — |
| `onPageScroll` / `onTabItemTap` | ✓ | — | — |
| `onResize` | ✓ | `pageLifetimes.resize` | — |
| `onPageShow` / `onPageHide` | — | `pageLifetimes.show` / `.hide` | — |
| `onShareAppMessage` / `onShareTimeline` / `onAddToFavorites` | ✓ (callback's return value is the share config; expression bodies auto-return) | — | — |

### `value:bind` (inputs)

`<input value:bind={text} />` — two-way binding: native `model:value` renders
keystrokes without setData echo; a generated `__vb_text` handler syncs the
logic-side mirror and recomputes deriveds through the normal batch.

### Hoisted expressions

Template bindings WXML can't evaluate — function calls, template literals,
optional chaining — compile to generated deriveds: page scope → `_h<i>` keys;
inside loops → the list is rewritten to `_hl<i>` whose items carry computed
`_c<i>` fields (keyed diffing preserved). Names are stable and visible in
emitted JS for debugging.

### `export const config = { ... }`

Static object literal → the unit's `.json` (page window config, or app-level
`window`/etc. in `app.mist`). Non-literal values are compile errors.

## CLI

```
mistc init <name>
mistc build <src-dir | entry.mist> [-o <outdir>] [--app] [--watch]
mistc test [dir] [--filter <substring>] [--timeout <secs>]
```

- **`init`** → scaffolds `<name>/` (app.mist, a todo page, a sample test,
  project.config.json, `mist.d.ts` + `tsconfig.json` + `package.json` for
  editor types).
- **`--watch`** → rebuilds on every `.mist`/`.ts` save (debounced).
- **`test`** → compiles `<dir>/src` to a temp dir and runs each
  `<dir>/tests/*.test.js` in a Node harness (`bootPage`, `flush`, `setData`
  payload recorder, `wx` stub). See [testing.md](testing.md). Requires Node;
  exits non-zero on any failing file.

- **Directory** → project build. Requires `<dir>/app.mist` and
  `<dir>/pages/*.mist` (index becomes the launch page). Components and stores
  are discovered through imports. Output uses the WeChat layout.
- **Single file** → flat build of one page + its imports; `--app` adds a
  minimal openable app shell (`App({})`, `app.json`, tourist appid config).
- `-o` defaults to `dist`. Errors exit 1 with `M`-coded messages; `M1002`/`M1006`
  are non-fatal stderr warnings.

## Emitted output (project build)

```
dist/
├── app.js  app.json  app.wxss  sitemap.json
├── mist-rt.js                  # the runtime (~9.6 KB)
├── tw-shared.wxss              # tailwind utilities (imported by every unit)
├── tw-theme.wxss               # page{} theme vars (imported by pages only)
├── pages/<name>/<name>.{js,wxml,wxss,json}
├── components/<kebab>/<kebab>.{js,wxml,wxss,json}
│                               # pure-render components: .wxml template only
└── stores/<name>.js
```

Generated JS is deliberately readable (DevTools can't load source maps): plain
`Page({...})`/`Component({...})` objects with your names intact.

## Runtime (`mist-rt.js`)

You never import this — generated code does. For debugging, its surface:

- `set(page, path, value)` / `touch(page)` / `flush(page)` — batch a write / a
  derive-only pass / the once-per-microtask flush that issues one `setData`.
  A rejected `setData` (e.g. payload too large) rolls the page's local mirror
  back, and store-bound pages then reseed their mirrors from current store
  values — a failed batch never leaves a page desynced from its stores.
- `init(page)` — first-render derive seed (called from generated `onLoad`/`attached`).
- `applyPath(obj, path, value)` — path-string writer used by the batcher.
- `derive(page, out, name, key, compute, deps)` — recompute one derived with
  keyed field-level diffing against snapshots; `deps` drives per-derived
  dirty-bit skipping (null ⇒ always recompute).
- `store(init)`, `bindStores`, `unbindStores` — shared-state boxes and page
  subscription glue.
- `observePerf()` / `perfEntries` — `wx.getPerformance` observer installed by
  generated `app.js`; entries readable via `getApp().__perf`.

