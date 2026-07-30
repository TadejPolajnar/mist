# Bridge-traffic benchmark

The mini-program bottleneck is the logic-thread → render-thread bridge: every
framework ultimately pays in `setData` payload bytes. This harness measures
exactly that, running the real `mist-rt.js` with the exact code shapes `mistc`
emits, against two baselines, on a 1000-row filtered todo list.

```sh
node benchmark/bench.js
```

## Results (Node 24, list = 1000 rows, 100 toggles)

| impl | setData calls | bytes/toggle | filter switch |
|---|---|---|---|
| **mist** (compiled) | 100 | **49 B** | 30.9 KB |
| hand-optimal (human-written perfect paths, `wx:if` filtering) | 100 | 24 B | 17 B |
| naive (resend list — common quick native code) | 100 | 96.6 KB | 79.1 KB |

> Numbers below are this Node harness. On-device measurement after dead-data
> elimination is lower still (26 B/toggle) — see `devtools/README.md`.

- **Toggling one row: mist sends ~2000× fewer bytes than the naive pattern**, and
  is within ~2× of the theoretical floor (the delta is the keyed-diff `visible[i]`
  item write; the floor skips a derived list entirely by `wx:if`-hiding rows,
  which costs render-thread work instead).
- Filter switch: the derived list changes length → mist correctly falls back to
  one full write of the *filtered* array (30.9 KB), still ~2.6× less than naive.
- Runtime shipped in the package: **~6 KB unminified** (batching + keyed diff +
  stores). Taro 3 ships React plus a DOM shim — on the order of 100 KB+ minified —
  before any app code.

## What this does and does not prove

**Does:** the compiler's founding claim — compile-time path tracking produces
near-hand-written bridge traffic with zero runtime diffing of trees. Bytes over
the bridge is the dominant, measurable cost that per-frame vdom frameworks pay.

**Does not:** wall-clock render times on a real device, or a head-to-head with an
actual Taro build — Taro's runtime renders through a recursive template with its
own data layout, which can't be faithfully simulated outside WeChat.

## Manual protocol: real Taro comparison in DevTools

1. Scaffold the equivalent app in Taro 3 + React (1000-row list, toggle handler,
   filter button).
2. Open both apps in WeChat DevTools → Details → enable **vConsole/Trace**;
   use the **Performance panel** → record while tapping 20 rows.
3. Compare: `setData` payload sizes (AppData tab shows sent data), script time
   per tap, and first-render time.
4. On-device: use the *Real-device debugging* profiler for launch time and
   FPS during fast repeated toggles.

The prediction from this harness: Taro's per-toggle payloads land between mist
and naive (it path-diffs its element tree, but payloads carry vdom node
structure, not bare values), and its launch cost includes parsing its runtime.
