# Ledger eval — Mist vs Taro 3, same realistic app

The same four-page expense tracker, built twice, idiomatically in each framework:

- **Mist**: [`examples/ledger`](../../examples/ledger) — `.mist` SFCs, `store()`, `derived()`, `value:bind`, Tailwind
- **Taro 3.6.35 + React 18**: [`taro-app/src`](taro-app/src) — `pages/l*` + `ledger/store.js` (module store + `useSyncExternalStore`), `useState` controlled inputs, hand-written WXSS

Both apps: home with budget hero + transaction list, add form with validation and
category picker, stats with per-category bars, detail page with delete, tab bar,
query-param navigation, shared store, dark theme.

Measured in real WeChat DevTools (2.01.2510290 / lib 3.16.2) with one instrument
([`evalrun.js`](evalrun.js)) that hooks `page.setData` **outside either framework**
and drives the identical state change (category pick) 30 times, one per tick.

---

## 1. Interaction cost — category pill change ×30

| metric | Mist | Taro 3 + React | ratio |
|---|---|---|---|
| **bytes per interaction** | **15 B** | 180 B | **12× less** |
| setData calls | 30 (1 per interaction) | 29 | parity |
| max payload | 17 B | 186 B | 11× |
| total over 30 | 440 B | 5,394 B | 12× |

> Reproducibility caveat: `mist-pill/`'s `.mist` source was never committed, and
> `dist/` directories are gitignored — so the 15 B figure survives only in this
> table and in local working copies; a fresh clone cannot rebuild or re-measure
> it. The fully source-reproducible on-device number is the list app's 26 B vs
> 67 B (`mist-app/`, which has committed `src/`).

**What each framework actually sends:**

```jsonc
// Mist — the state that changed
{"cat":"Food"}

// Taro — the rendered element-tree diff
{"root.cn.[0].cn.[0].cn.[1].p24":"",
 "root.cn.[0].cn.[1].cn.[1].p24":"",
 "root.cn.[0].cn.[2].cn.[1].cn.[0].cl":"pill bench-pill",
 "root.cn.[0].cn.[2].cn.[1].cn.[1].cl":"pill-on bench-pill", …}
```

This is the architectural difference in one line: **Mist ships state, Taro ships
rendered output.** Mist's WXML evaluates the conditional classes from `cat`
itself (`class="{{cat === 'Food' ? '…bg-blue-600…' : '…card…'}}"`), so one data
key covers all three pills. Taro's reconciler diffs its element tree and must
send each changed node's class string by path.

Note also what Mist *doesn't* send: the `valid` derived is unchanged by a
category pick, so the keyed diff omits it entirely — 15 B is the whole write.

## 2. Batching behaviour (bonus finding)

Driving all 30 changes **synchronously** instead of one-per-tick:

| | Mist | Taro |
|---|---|---|
| setData calls | **1** | 1 |
| total bytes | **13 B** | 2,791 B |

Both frameworks batch, but Mist's batch collapses to the *final state*
(`{"cat":"Fun"}` — 13 B), while Taro's collapses to a full re-render of the tree
(2.8 KB). Burst updates — the pathological case for the bridge — favour Mist by
**215×**.

## 3. Initial page payload (Add page)

| metric | Mist | Taro | ratio |
|---|---|---|---|
| initial `data` bytes | **366 B** | 1,259 B | **3.4× less** |
| data keys | `title, amount, cat, valid, ledger` | `root` (serialized element tree) | — |

Mist's page data *is* the app state. Taro's is a serialized virtual tree; the
app's actual state (three strings) is a rounding error inside it.

## 4. Package size (sum of file bytes — not `du`, which block-rounds small files)

| artifact | Mist | Taro |
|---|---|---|
| **total build** | **19.2 KB** | 302.5 KB (**16×**) |
| framework runtime | `mist-rt.js` **6.0 KB** | `taro.js` 117.7 KB + `vendors.js` 15.3 KB + `runtime.js` 2.1 KB |
| app logic | `app.js` 0.3 KB + pages 1.2–1.5 KB each | `app.js` 94.5 KB (bundled React + app) |
| template scaffolding | per-page WXML (~1–2 KB) | `base.wxml` **56.5 KB** (recursive renderer) |
| styles | `tw-shared.wxss` 2.5 KB (generated) | hand-written CSS |
| files emitted | 25 | 35 |

## 4b. Build & toolchain

| metric | Mist | Taro |
|---|---|---|
| incremental build (4-page app) | **0.40 s** | 4.0 s (**10×**) |
| cold build | 1.03 s | 5.5 s |
| toolchain dependencies | 126 crates | 870 npm packages |
| toolchain on disk | 3.3 MB binary | 442 MB `node_modules` |

Mist's build time *includes* invoking the real Tailwind CLI subprocess.

Taro's 56.5 KB `base.wxml` is the generic recursive template every Taro page
renders through — the cost of retaining React semantics on a static template
platform. Mist emits plain per-page WXML instead.

## 5. Launch

| | Mist | Taro |
|---|---|---|
| DevTools launch time | ~4.9 s (cold, tourist mode) | ~5 s (cold) |
| `evaluateScript` (list bench) | 1 ms | React bundle parse+eval |

Launch timings in DevTools are dominated by tooling overhead; treat as
directional only. The meaningful proxy is the parse/eval surface: **6 KB vs
233 KB** of framework JS before app code runs.

## 6. Authoring cost — same app, both codebases

| | Mist | Taro |
|---|---|---|
| **total lines** (app + store + styles) | 276 | **248** |
| shared store | 26 lines, no plumbing | 30 lines + `useSyncExternalStore` wiring |
| form field | `value:bind={amount}` | `value={amount}` + `onInput={e => setAmount(e.detail.value)}` + `useState` |
| derived values | `derived(() => …)`, diffed by compiler | recomputed each render; payload decided by reconciler |
| styling | Tailwind classes inline (auto rpx + sanitization) | hand-written WXSS (Tailwind needs extra setup) |
| navigation | `<a href="/pages/x/x?id={{id}}">` | `Taro.navigateTo({ url })` + `useRouter()` |
| tab bar / config | `export const config` in `app.mist` | `app.config.js` |

Line counts are effectively a wash — Taro is slightly shorter here because its
hand-written CSS is terser than the equivalent utility classes. The differences
are qualitative: Mist needs no subscription plumbing and no `useState`+`onInput`
pair per field, and gets Tailwind natively; Taro brings React's ecosystem,
familiarity, hooks, and a far larger hiring pool.

---

## Method notes & limits

- One instrument, both apps: `setData` hooked on the live page object, outside
  either framework. Interactions are driven by invoking the page's own handler
  (`p.pick(c)` in Mist; a `__setCat` hook calling `setCat` in Taro) — identical
  state transitions, one per 40 ms tick.
- **Why not synthetic taps:** `miniprogram-automator`'s `page.$$()` hangs on both
  apps' Add pages (verified in isolation — `launch` and `evaluate` work fine).
  Driving handlers measures the same setData path minus tap dispatch, which is
  framework-independent anyway.
- DevTools ≠ device. Real hardware typically amplifies package-parse and bridge
  costs, so these ratios are likely conservative for Mist.
- Single Taro version (3.6.35 + React 18, webpack5 production build). Taro 4 and
  other frameworks are unmeasured.
- Reproduce: `node evalrun.js mist-pill mist` and
  `WANT_PAGE=pages/ladd/ladd node evalrun.js taro-app taro`.

## Companion: list benchmark

For a large-list workload (1000 rows, 50 row-toggles), see [README.md](README.md):
Mist 26 B/toggle vs Taro 67 B, tap p50 68 ms vs 162 ms, package 9.6 KB vs 293 KB.
