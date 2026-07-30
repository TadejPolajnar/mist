# DevTools benchmark harness (mist vs Taro, same instrument)

Drives real WeChat DevTools via the official `miniprogram-automator` and measures
**identically for any framework**: `setData` is hooked at the live page object
(outside any framework's runtime), a scripted interaction run is timed, launch
entries come from `wx.getPerformance()`, and package size from the build output.

## One-time setup

1. Install WeChat DevTools.
2. DevTools → **Settings → Security → enable Service Port** (设置 → 安全 → 服务端口).
3. `npm install` in this directory.

If DevTools lives somewhere unusual, set `WX_CLI=/path/to/cli`.

## Measure the mist app

```sh
# from repo root: build the 1000-row bench app
cargo run -- build benchmark/devtools/mist-app/src -o benchmark/devtools/mist-app/dist

cd benchmark/devtools
npm install
npm run measure:mist          # or: TOGGLES=100 node measure.js mist-app
```

Output: JSON report — `setDataCalls`, `setDataBytes`, `bytesPerToggle`,
`maxPayloadBytes`, `msPerToggle`, `packageBytes`, launch/first-render entries.

## The Taro twin

`taro-app/` is committed and ready — a hand-rolled Taro 3.6.35 + React 18 project
(webpack pinned to exactly 5.78.0; newer webpack rejects Taro's ProgressPlugin
options). Do **not** run `taro init` over it; that would clobber the pinned setup.

```sh
cd benchmark/devtools/taro-app
npm install && npm run build:weapp
cd .. && npm run measure:taro
```

Its ledger pages (`src/pages/l*`) mirror `examples/ledger` feature-for-feature, and
`src/pages/index.jsx` / `shop.jsx` are the list and shop benchmarks. Taro is
MIT-licensed, © NervJS — nothing of Taro's is vendored here, only app code written
against it for comparison.

## Measured head-to-head (real DevTools — 2026-07-29)

1000 rows, 50 scripted toggles each, same instrument, DevTools 2.01.2510290 /
lib 3.17.0 (EVAL.md's ledger runs used lib 3.16.2 — different sessions). Taro twin: Taro 3.6.35 + React 18, webpack5, production build.

**List app** (1000 rows, 50 toggles):

| metric | mist | Taro 3 + React | delta |
|---|---|---|---|
| ms per toggle (p50 / p95) | **68 / 113** | 162 / 180 | **~2.4× faster** |
| setData calls per toggle | 1 | 1 | tie |
| bytes per toggle | **26 B** | 67 B | **2.6× smaller** |
| initial data payload | **49.5 KB** | 140 KB | 2.8× smaller |
| filter switch (ms / bytes) | **72 / 32 KB** | 78 / 80 KB | 2.5× less data |
| evaluateScript at launch | **1 ms** | (React parse+eval) | — |
| package size | **9.6 KB** | 292.9 KB | **30× smaller** |

**Shop app** (100 products, cart w/ quantities, component events, 3 deriveds; 50 add-to-cart taps):

| metric | mist | Taro 3 + React | delta |
|---|---|---|---|
| ms per tap (p50 / p95) | 67 / 81 | 57 / 89 | comparable (harness-dominated at this scale) |
| bytes per tap | **84 B** | 286 B | **3.4× smaller** |
| initial data payload | **5.2 KB** | 22.2 KB | 4.3× smaller |
| filter switch bytes | **1.3 KB** | 14.2 KB | **11× smaller** |
| package size | **11.6 KB** | 294.9 KB | 25× smaller |

`26 B` on the list app is hand-written setData parity — the result of dead-data
elimination (unbound `todos` lives as an instance field, so a toggle sends only
the derived `visible[i].done`). The shop numbers show the pattern generalizes:
payload advantages grow with data complexity (11× on structural changes), while
tap latency converges at small list sizes where React reconciliation is cheap
and automator overhead dominates. Row taps inside custom components use the
`'host|.inner'` selector form (host taps don't reach inner handlers).

The bytes-per-toggle flip vs the first measurement (87 B → 49 B) came from adding
**field-level keyed diffing** to `rt.derive`: a changed list item now emits only
its changed fields (`visible[3].done`) instead of the whole item object. The
Node harness (`../bench.js`) tracks the same number at 49 B — within 2× of the
hand-written floor (24 B, which forgoes a derived list entirely).

Honest read:

- **Interaction latency is the real differentiator**: both harness runs share the
  same automator websocket overhead, so the ~94 ms/tap delta is Taro's React
  reconciliation over the 1000-row list on the logic thread — work mist's
  compile-time path tracking eliminates entirely.
- **Steady-state bridge traffic is comparable** — Taro's element-tree diff sends
  one element patch (67 B); mist sends only the changed field of the
  derived item (26 B on-device). Taro 3 is genuinely good at this; the naive-resend pattern
  (96.6 KB/toggle, `../bench.js`) is what both frameworks beat.
- **Package size**: Taro ships `taro.js` (120.5 KB) + compiled React (96.7 KB in
  `app.js`) + a 56.5 KB recursive `base.wxml` before any app code; mist's entire
  app including its runtime is 9.6 KB. This also implies launch-time parse/eval
  cost not captured here (launch entries need a pre-launch observer — TODO).
- mist's exactly-one-batched-setData-per-tap at 26 B on-device improves on the Node harness figure of 49 B (the harness models the pre-dead-data-elimination path).

Reproduce: the Taro twin in `taro-app/` is hand-rolled (pinned Taro 3.6.35,
webpack pinned to exactly 5.78.0 — newer webpack rejects Taro's ProgressPlugin
options). `npm install && npm run build:weapp` in `taro-app/`, then
`npm run measure:taro`.

## Comparing

Run both with the same `TOGGLES`, and compare:

- `bytesPerToggle` / `setDataCalls` — bridge traffic per interaction (the metric
  the mist compiler optimizes; `benchmark/bench.js` predicts ~26 B and 1 call
  per toggle for mist)
- `msPerToggle` — end-to-end scripted tap latency (includes render)
- `launch` entries — `appLaunch`, `evaluateScript`, `firstRender`
- `packageBytes` — shipped code size (mist runtime is ~6 KB; Taro ships its
  React runtime + DOM shim)

Caveats: DevTools timings are not phone timings (use real-device debugging for
final numbers); automator taps serialize through the protocol, so `msPerToggle`
comparisons are only valid harness-vs-harness, not against manual tapping.
