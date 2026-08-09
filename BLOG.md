# I wrote a compiler to find out how compilers work. It ships 12× fewer bytes than Taro.

*Should you use this? No — it's a prototype. Read it for the compiler tricks, the
bugs that only appear on real hardware, and one optimization a runtime
fundamentally cannot do.*

---

## 1. A confession, and where it came from

I don't actually know how compilers work.

Not "I couldn't pass a compilers exam" — I mean I had never watched source text
become a different program and understood every step in between. I've shipped a
lot of software on top of compilers while treating them as weather.

The itch became unbearable at Anthropic's conference in Tokyo, where I heard
about Jarred Sumner [rewriting Bun in Rust](https://bun.com/blog/bun-in-rust).
Reading more blog posts wasn't going to fix my gap. The only way I've ever
learned anything is to build the thing and let it break in my hands.

So: build a compiler. Not a toy that turns arithmetic into assembly. Something
that has to survive contact with a real platform, real constraints, and a real
competitor.

## 2. The target: WeChat Mini Programs

I've used WeChat daily for years, and its mini-app platform is a strange and
fascinating piece of engineering. A mini program is not a web page. It runs in a
**dual-thread architecture**: your JavaScript executes in one thread, rendering
happens in a separate WebView thread, and the only way to move data between them
is a function called `setData` that **serializes to JSON and pushes it across a
bridge**.

There is no DOM. You cannot create elements at runtime. Your UI is a static
template language (WXML) with data bindings — a bit like Vue templates carved in
stone.

That single constraint dominates everything. Every framework on this platform —
Taro, uni-app, Remax — is ultimately judged by one question: *how many bytes do
you push across that bridge, and how often?*

I'd heard of these frameworks. I'd never used them. That turned out to be an
advantage: I had no idea what was supposed to be hard.

## 3. The challenge

> **Build my own component language. Write the compiler in Rust. Make it faster
> than what exists.**

Plus one rule that shaped everything: **measure or it didn't happen.** No "feels
faster." Numbers, from the real platform, against a real competitor.

## 4. The insight that made it possible

Here's what I only understood after digging into how Taro works.

Taro 3 runs **React** on the logic thread. To do that on a platform with no DOM,
it ships a miniature DOM implementation, lets React reconcile against it, then
serializes the resulting element tree across the bridge into a **generic
recursive WXML template** that can render any node shape. I read that template —
it's a `<template name="taro_tmpl">` that loops over `root.cn` and recurses into
itself. It's 56 KB of scaffolding, and it's genuinely clever.

It's also, structurally, doing a lot of work at runtime to recover information
that was **available at compile time**.

If I know at compile time that `count.value++` mutates exactly one piece of
state, I don't need a virtual DOM to figure out what changed. I can emit:

```js
this.setData({ count: 5 })
```

Not a diff of a tree. Not an element patch. The state that changed. **The
compiler's real job isn't translating syntax — it's minimizing `setData`.**

That became the thesis of the entire project.

### "So it's Svelte?"

Yes, in the sense that Svelte proved compile-time reactivity beats runtime
reactivity, and I'm standing on that idea. The difference is what the payoff
buys you. On the web, Svelte's compiler saves you virtual-DOM work and bundle
size — real but incremental, because DOM operations are fast and local. On WeChat
the same idea pays out in **bytes across a serialized cross-thread bridge**,
where the cost is brutal, measurable, and scales with your data. Same trick,
much better odds.

## 5. Meet Mist

I called the language **Mist** — Mini-app Static Templates. A `.mist` file has
three parts: TypeScript frontmatter, a JSX-ish template, and optional styles. If
you've used **Astro**, this will feel immediately familiar — I stole the shape
shamelessly.

Here's a page from the demo app (lightly abbreviated):

```jsx
---
import { ledger, addTx } from '../stores/ledger.ts'
import { state, derived } from 'mist'

export const config = { navigationBarTitleText: 'Add expense' }

const title  = state('')
const amount = state('')
const cat    = state('Food')

const valid = derived(() => title.value.length > 0 && Number(amount.value) > 0)

function pick(c) {
  cat.value = c
}

function save() {
  if (!valid.value) return
  addTx(title.value, Number(amount.value), cat.value, Date.now())
  title.value = ''
  wx.switchTab({ url: '/pages/index/index' })
}
---
<div class="p-4 flex flex-col gap-5">
  <input class="text-4xl font-bold text-white" value:bind={amount} placeholder="¥0" />
  <div class="flex gap-2">
    <div class={cat.value === 'Food' ? 'pill-on' : 'pill'} onTap={() => pick('Food')}>🍜 Food</div>
    <div class={cat.value === 'Transit' ? 'pill-on' : 'pill'} onTap={() => pick('Transit')}>🚇 Transit</div>
  </div>
  <button class={valid.value ? 'btn-on' : 'btn-off'} onTap={save}>Add expense</button>
</div>
```

Reactive state in boxes, Tailwind classes, two-way input binding, a shared store
imported from a plain `.ts` file. Nothing exotic.

And here's what the compiler emits — plain, readable mini-program code:

```js
// generated by mistc — do not edit
const rt = require('../../mist-rt.js');
const __S0 = require('../../stores/ledger.js');
Page({
  data: { title: '', amount: '', cat: 'Food', valid: null, ledger: null },
  __derive() {
    const __o = {};
    rt.derive(this, __o, 'valid', null,
      () => this.data.title.length > 0 && Number(this.data.amount) > 0);
    return __o;
  },
  pick(c) {
    this.__set('cat', c)     // ← one path. one value.
  },
  ...
})
```

No virtual DOM. No reconciler. `cat.value = c` became `this.__set('cat', c)` at
**compile time**, and the entire runtime supporting this is **6 KB**.

## 6. The optimization a runtime can't do

My favourite trick in the whole project, and the one I'd put on a slide.

Consider a page that keeps a list of todos but only ever *renders* the filtered
view:

```js
const todos   = state([...1000 items...])
const visible = derived(() => todos.value.filter(t => !t.done))
// template renders {visible.value.map(...)} — never touches `todos`
```

A runtime framework has no choice: `todos` is state, state lives in the page's
data, and the page's data crosses the bridge. But a *compiler* can look at the
template and see that `todos` is never read there. So Mist doesn't put it in
`data` at all — it becomes a plain instance field:

```js
// before                          // after
data: {                            data: {
  todos: [ ...1000 items... ],       visible: null,
  visible: null,                   },
}                                  onLoad() { this._todos = [...] }
```

The array still lives in the logic thread. It just never crosses the bridge.
Initial page payload halved; per-interaction bytes dropped from 49 B to 26 B.

**A runtime can't see this, because "does the template read this?" is a
compile-time question.** That's the whole argument for compilers in one example.

## 7. The bugs

**The bug that only appeared on a real device.** My dead-data optimization
emitted mutations as `(this._todos[i].done = x, rt.touch(this))` — on the line
after an unterminated statement. JavaScript's automatic semicolon insertion
parsed it as *a function call on the previous line's result*. Every tap threw.
Tests passed (they had semicolons). Only the real device showed it. The fix is
one character: emit `;(…)`.

**A yuan sign broke my parser.** `¥` — the RMB sign, on a Chinese platform, in an
expense tracker: about as predictable as a character gets — is two bytes in UTF-8, and my
whitespace-trimming code sliced at a byte index — panicking mid-character. Found
by writing a demo app with prices in it. This is the argument for dogfooding in
one line.

**Is 115 tests enough?** Honestly: no, and I want to be precise about what they
do and don't buy me.

What they cover well: every compilation rule has a test that pins the *emitted
output* — not "did it compile" but "did it emit exactly this `setData` path."
That's the layer where a refactor silently degrades performance, and it's why the
benchmark also runs as a test: if a change makes the compiler emit fatter
payloads, CI fails on the number, not on a crash.

What they don't cover: I have 41 error paths in the compiler and dedicated tests
for maybe a third of them. No fuzzing, no property tests, no snapshot suite. When
I audited coverage while writing this post I found a dozen untested paths —
unquoted attributes, unknown event modifiers, deeply nested state mutations,
multibyte text in attributes — wrote tests for all of them, and every single one
already behaved correctly. That's a comfortable result and a slightly damning
one: those tests are regression insurance, not bug discovery. The bugs that
actually bit me were never found by unit tests. They were found by **running the
thing** — on a real device, with a real app, with real characters in it.

If I were taking this further, the highest-value additions would be property
tests over the mutation compiler (generate random state paths, assert the emitted
path round-trips) and a snapshot suite over emitted WXML/JS. Neither would have
caught the ASI bug. Only the device did.

**Four bugs an adversarial reviewer caught.** After each milestone I had a second
AI agent audit the work with no context on how I'd written it, specifically
hunting for bugs. It earned its keep four times:

- My "path-precise setData" claim was **quietly false for filtered lists** —
  every derived array was being resent whole. That's the entire thesis, broken,
  in the flagship case.
- Tailwind classes **silently did nothing inside components**, because WeChat
  isolates component styles and my utilities lived in the page sheet.
- Two store files with the same name would **silently overwrite each other**.
- A state name colliding with a store import silently discarded one — last object
  key wins, no error. (Now an `M1005` compile error.)

None of those would have thrown. All of them would have shipped.

**Tailwind v4 fought back.** Modern CSS is a wonderland of `@layer`, `@property`,
`oklch()`, and `color-mix()`. WXSS supports essentially none of it, so I wrote a
post-processor that unwraps layers, converts OKLCH to hex with a real
colour-space implementation, and rewrites `rem` to `rpx`. My favourite discovery:
`rounded-full` compiles to `border-radius: calc(infinity * 1px)`, which WXSS
simply refuses to parse.

**The measurement tooling was flakier than the compiler.** WeChat's official
automation SDK hangs indefinitely on `page.$$()` for these pages — no error, no
timeout, just silence (pages with `<input>` elements, as far as I could isolate).
Two hung processes then poisoned the automation port for everything else. I ended
up driving interactions through `evaluate()` instead. The compiler was never the
unreliable part.

## 8. What exists now

A Rust compiler, ~3,600 lines, with **115 tests**:

- **`mistc build src/ -o dist`** — a project tree to a complete, DevTools-ready
  mini program in milliseconds
- **Reactivity**: state, derived values, path-precise mutation compilation, keyed
  field-level diffing, dead-data elimination, microtask batching
- **Components**: props, callback props → native component events, slots, and
  automatic compile-time inlining of pure-render components into WXML templates
- **Stores**: shared reactive state in plain `.ts` modules, with path-precise
  batched updates to every subscribed page
- **Real Tailwind v4** via the actual CLI, post-processed for WXSS
- **Diagnostics** with source positions and fix-its:

```
error: M1004 at line 12:3: `items.value.splice(i, 1)` — only push/index
assignment compile to precise writes
  help: reassign `items.value = ...` instead
```

That error is the thesis defending itself: I can't compile `splice` to a precise
`setData`, so the language refuses it rather than silently getting slow.

## 9. The showdown: Mist vs Taro 3

I picked **Taro** because it's the strongest option: mature, maintained by JD.com,
React-based, and the default answer for "modern tooling for mini programs."
Beating a strawman proves nothing.

**Method.** I built *the same app twice* — a four-page expense tracker, same
features, same design — once in Mist, once in Taro 3.6.35 + React 18 (module
store with `useSyncExternalStore`, `useState` controlled inputs, `useRouter`).
Then I measured both with **one instrument** that hooks `setData` on the live
page object, *outside* either framework, driving the identical state change 30
times.

### Changing a category pill, ×30 (form page, 3 pills)

| metric | **Mist** | Taro 3 + React | ratio |
|---|---|---|---|
| **bytes per interaction** | **15 B** | 180 B | **12×** |
| setData calls | 30 | 29 | parity |
| total over 30 | 440 B | 5,394 B | 12× |

The payloads say everything:

```jsonc
// Mist — the state that changed
{"cat":"Food"}

// Taro — the rendered element-tree diff
{"root.cn.[0].cn.[0].cn.[1].p24":"",
 "root.cn.[0].cn.[2].cn.[1].cn.[0].cl":"pill bench-pill",
 "root.cn.[0].cn.[2].cn.[1].cn.[1].cl":"pill-on bench-pill", …}
```

**Mist ships state. Taro ships rendered output.** Mist's template computes all
three pills' classes from the single `cat` key. Taro's reconciler must send each
changed node's class string by tree path.

### Everything else

| metric (workload) | **Mist** | Taro |
|---|---|---|
| bytes/toggle (1000-row list) | **26 B** | 67 B |
| tap p50 (1000-row list) | **68 ms** | 162 ms |
| initial page payload (form page) | **366 B** | 1,259 B |
| total package (sum of file bytes) | **19.2 KB** | 302.5 KB (**16×**) |
| framework runtime | **6.0 KB** | 117.7 KB + 15.3 KB vendors + 94.5 KB bundled React |
| template scaffolding | per-page WXML (~1–2 KB) | `base.wxml` **56.5 KB** recursive renderer |

For scale on the bridge numbers: the naive pattern — recompute and resend the
list on every change, which is what a lot of hand-written mini-program code does
— costs **96.6 KB per toggle**. Both frameworks beat that by orders of magnitude.
My win is over a well-engineered competitor, not a broken one.

### Developer experience: the metrics nobody benchmarks

Runtime numbers get the attention, but you feel these every single day:

| metric | **Mist** | Taro | ratio |
|---|---|---|---|
| **incremental build** (same 4-page app) | **0.40 s** | 4.0 s | **10× faster** |
| cold build | 1.03 s | 5.5 s | 5× |
| build toolchain deps | **126 crates** | 870 npm packages | 7× |
| `node_modules` on disk | **none** (single 3.3 MB binary) | **442 MB** | — |
| output files emitted | 25 | 35 | — |

The build-time gap is the one I underestimated. Four seconds is exactly long
enough to lose your train of thought; 0.4 s is "save and look up." And Mist's
0.40 s *includes* shelling out to the real Tailwind CLI — the Rust compilation
itself is a rounding error inside it.

The `node_modules` line deserves a stare: 442 MB and 870 transitive packages to
build a four-page expense tracker, versus a single 3.3 MB static binary. That's
not a fair fight in Taro's favour on ecosystem — but it is 442 MB of supply chain
you're carrying.

### Where Taro wins

- **Taro is faster on small lists.** On a 100-item page, tap latency was 57 ms
  for Taro vs 67 ms for Mist. React reconciliation over 100 rows is cheap, and my
  advantage only opens up as data grows. If your app is small, this whole project
  buys you nothing.
- **Mist has a real architectural weak spot.** When a derived list changes
  *length*, the keyed diff can't do per-item writes and falls back to sending the
  whole array — 32 KB on a filter switch. Taro's tree diff degrades more
  gracefully there.
- **The Taro codebase was shorter** — 248 lines vs my 276.
- **React's ecosystem is Taro's entire point.** Hooks, libraries, hiring, Stack
  Overflow answers at 3am. Mist has me.
- **Taro is cross-platform** (Alipay, Douyin, and more). Mist is WeChat-only.

### A footnote on batching

Driving 30 changes *synchronously* in one tick: Mist emits 13 B, Taro emits
2,791 B — a 215× gap. No real app does this, so treat it as an architectural
curiosity rather than a benchmark: it shows what each batching model collapses
*to*. Mist's batch collapses to the final state; Taro's to a full re-render.

### Making sure the numbers are real

- Same instrument, both apps, `setData` hooked outside either framework
- Both apps verified working by hand in DevTools before measuring
- I read the emitted WXML and JS directly to confirm the numbers reflect what I
  think they do
- The benchmark runs in CI as a regression guard
- **Limits, stated plainly:** measured in WeChat DevTools, *not* on a physical
  device (real hardware would likely widen the gaps, since parse and bridge costs
  hurt more there); one Taro version; interactions driven through page handlers
  because the official automation SDK hangs on these pages

## 10. What I learned

**Build speed compounds more than I expected.** I set out to optimize bytes over
a bridge and accidentally got a 10× faster edit-refresh loop, because a Rust
binary with no `node_modules` doesn't have to wake a bundler up. That changed how
I worked more than any runtime number did.

**The best optimization is not doing the work.** Every big win came from deleting
runtime work, not speeding it up. No virtual DOM. No tree diffing. Data that
never enters `data`. The fastest code is code that doesn't run.

**Constraints are a gift.** I got a 12× win partly because I *refused* to support
arbitrary JavaScript semantics. `array.splice()` is a compile error in Mist. That
looks like a limitation on the feature list and reads like a superpower in the
benchmark.

**Measure or you're guessing.** I was certain field-level diffing would be a
minor tweak — it nearly halved payloads. I was certain dead-data elimination
would only affect the initial payload — it halved per-interaction bytes too. My
intuition about my own compiler was wrong repeatedly, in both directions.

**Adversarial review is worth more than more code.** The most valuable minutes of
this project were spent having a second agent attack work I thought was finished.
Four times it caught something that would have silently shipped — including the
bug that falsified the project's core claim.

**And the thing I actually wanted:** I now know how a compiler works, because I
have the scar tissue. I know what a span-based rewrite is because I chose it over
codegen to preserve users' formatting. I know why automatic semicolon insertion
is a menace, because it broke my output on a real device. I know what a keyed
diff costs, because I wrote one and then made it cheaper twice.

That was the whole point. The 12× was a bonus.

---

*Mist is a prototype. The language spec, compiler, demo app, benchmark harness,
and the Taro twin used for these comparisons are all in the repo — including the
eval methodology, so you can tell me where I'm wrong.*
