# Testing mist apps

[中文 → testing.zh-CN.md](testing.zh-CN.md)

`mistc test` compiles your `src/` and runs every `tests/*.test.js` file in a
Node harness that boots your compiled pages with `Page`/`wx` stubs. No
device, no DevTools, no extra dependencies beyond Node.

```sh
mistc test                   # from the project root (src/ + tests/)
mistc test --filter cart     # only files whose file name (not path) contains "cart"
mistc test --timeout 60      # per-file timeout in seconds (default 30)
mistc test --watch           # rerun when src/ or tests/ files change
```

A file that exceeds the timeout is killed and reported as `FAIL … timed out`.

Each test file is a plain Node script. Export an async function; use
`node:assert` (or any assertion style you like):

```js
const assert = require('node:assert');

module.exports = async () => {
  const app = bootPage('index');
  assert.equal(app.data().open.length, 2);

  app.page.toggle(1);
  await flush();
  assert.equal(app.data().open.length, 1);
  assert.ok(app.lastPatch().size < 300);
};
```

A file fails when the exported function throws or rejects. `mistc test`
prints `PASS`/`FAIL` per file and exits non-zero if any file failed.

## The harness API

These globals are available in every test file:

- **`bootPage(name, options?)`** — requires the compiled page and calls its
  `onLoad`. `name` is a page short name (`'index'` → `pages/index/index.js`)
  or a dist-relative path (`'packages/shop/pages/cart/cart'`). Options:
  - `query` — the object passed to `onLoad` (route query params).
  - `setDataLimit` — bytes (default 1 MB, like the real WeChat limit). An
    oversized patch throws *inside the runtime's batched flush*, which
    catches it and rolls back — so the rollback path is exercised for
    real, but your test never sees the throw. Assert on the handle's
    `rejected` array instead. Lower the limit to enforce a stricter
    payload budget. Note the runtime chunks multi-key batches over its own
    ~900KB budget into several sequential `setData` calls — each lands in
    `patches` separately; only a single key too large to split can be
    rejected.

  Returns a handle:
  - `page` — the registered Page object: call your methods
    (`app.page.toggle(1)`), read `page.data`.
  - `data()` — shortcut for `page.data`.
  - `patches` — every `setData` so far: `{ keys, size, patch }` with `size`
    in bytes. **Payload-size assertions are the point** — a path-precise
    toggle should be tens of bytes, and a test pinning `size < 300` catches
    the regression that re-sends a whole list.
  - `rejected` — patches refused for exceeding `setDataLimit` (same
    `{ keys, size, patch }` shape); they never enter `patches`. Note the
    runtime's rollback then re-syncs store mirrors with a small *accepted*
    patch, so `patches` may still grow by one after a rejection.
  - `lastPatch()` — the most recent patch, or `null`.
  - `totalBytes()` — sum of all patch sizes.
- **`flush(ms = 0)`** — await after a mutation: the runtime batches
  `setData` in microtasks, so assert only after `await flush()`.
- **`load(name)`** — `require` any compiled module by dist-relative path
  (e.g. `load('stores/cart')` for a store's live exports).
- **`resetModules()`** — clear the module cache for compiled files, so the
  next `bootPage`/`load` gets fresh store state.
- **`appHide()`** — fire `wx.onAppHide` callbacks (store persistence
  writes back on app hide; call this, then assert on `wx.__storage`).
- **`wx`** — the stub: `getStorageSync`/`setStorageSync` are backed by a
  real in-memory Map (`wx.__storage`), so persistence round-trips work.
  Every other `wx.*` call is a recorded no-op appended to `wx.__calls` as
  `{ name, args }` — assert on navigation, toasts, etc.

## What it is not

This is a **logic harness, not a renderer**. It runs your compiled page JS in
Node — state, deriveds, methods, stores, persistence, `setData` payloads.
There is no WXML rendering, no component tree, no event bubbling, and no
`wx` API behavior beyond storage: `wx.request` etc. are recorded no-ops you
assert on or stub further yourself. Only pages boot: `bootPage` on a
component unit fails with an explanation — test component logic through a
page that uses it. `getApp()` returns `{}` unless your test registers an
`App` itself. For pixel/interaction testing, use WeChat DevTools
(optionally driven by `miniprogram-automator`).

`mistc init` scaffolds a working `tests/index.test.js` — start from there.

## Snapshot tests — `mistc test --snapshots`

Pins the **emitted output** itself, catching what behavior tests can't: a
compiler upgrade quietly changing your generated WXML/JS/WXSS/JSON.

```sh
mistc test --snapshots     # compile src/, diff every emitted file vs snapshots/
mistc test --update        # accept current output as the new goldens
```

The first run writes `snapshots/` (commit it). After that, every run
recompiles and reports drift per file — `CHANGED` with the first differing
lines, `ADDED` for new output files, `REMOVED` for goldens nothing emits
anymore — and exits nonzero on any drift. `--filter <substr>` narrows the
comparison to matching paths. `--update` rewrites the goldens and prunes
stale ones (pruning is skipped under `--filter`); review the `git diff` of
`snapshots/` like any other change. Snapshot runs are one-shot — `--watch`
does not combine with `--snapshots`.

Intentional codegen changes (upgrading `mistc`, editing templates) will
drift by design — that is the point. Run `--snapshots` in CI next to
`mistc test`; the pair covers behavior and output.
