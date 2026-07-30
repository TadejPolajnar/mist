# I wrote a compiler to find out how compilers work. Builds got 10× faster, updates got 12× smaller.

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

So: build a compiler. Not a toy that turns arithmetic into assembly — something
that has to survive contact with a real platform and a real competitor.

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

## 3. The insight that made it possible

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

## 4. Meet Mist

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

## 5. How the compiler actually works

I wrote a post about benchmarking a compiler and almost forgot to describe the
compiler. Here's the shape of it, ~3,600 lines of Rust.

**Two frontends.** The frontmatter is TypeScript, so I parse it with
[oxc](https://oxc.rs) — a fast Rust JS/TS parser with arena allocation. The
template isn't JavaScript (it's JSX-ish markup with WXML control flow), so that's
a hand-rolled recursive-descent parser producing a small `Node` enum:
`Element / Text / Expr / For / If`.

**No IR, deliberately.** Source → AST walk → edit list → text. Every transform
here is local and syntax-directed: a mutation becomes a path write, a `.map()`
becomes a `wx:for`. There's no pass that needs to reorder statements or reason
about control flow globally, so there's nothing for an IR to buy. That's also why
it's 3,600 lines and compiles a project in 0.4 s.

**Span-based rewriting instead of codegen** — the decision the whole compiler
hangs on. I parse with oxc but never print from the AST. Instead a visitor
collects surgical edits:

```rust
struct Edit { start: u32, end: u32, text: String }
```

...which get sorted descending and spliced into the *original source text*. The
win: your formatting, comments, and TypeScript syntax survive untouched, and I
don't need a codegen dependency. The cost is real, and it bit me — see the ASI
bug below. A codegen-based compiler literally cannot emit an unterminated
statement; a text-splicing one can, and did.

**How reactivity is tracked.** Three analyses feed each other:

1. Scan the template for which names it reads (`{visible.value...}` → `visible`).
2. Walk the frontmatter AST for mutations; each becomes a path expression
   (`todos.value[i].done = x` → `` `todos[${i}].done` ``).
3. Any state the template *doesn't* read gets demoted out of `data` entirely.

You can read the decision in the output. Two mutation strategies, chosen at
compile time:

```js
this.__set('filter', 'open')                  // crosses the bridge, path-precise
;(this._todos[i].done = x, rt.touch(this))    // stays on the logic thread
```

**One honest disclosure**: derived values are *not* dependency-tracked. Every
flush recomputes all of them and the runtime diffs the results to decide what to
send. That's Vue's `computed` without the dependency graph — it costs O(deriveds)
per update and I'd fix it with real tracking if this were production.

**Where the analysis gives up.** I can model `push` and `arr[i] = x` as paths. I
cannot model `splice`. So the language rejects it rather than silently falling
back to sending the whole array — a whitelist over statically-modelable mutation
forms, with the boundary made visible to the user as a compile error.

### Prior art, before you tell me about it

I'm not first here. Svelte 3's `$$invalidate` is the direct ancestor of my
`__set` — compile-time assignment interception, with a bitmask where I use a
string path. Vue's compiled templates already hoist static subtrees and mark
dynamic bindings with patch flags. Million.js compiles templates into blocks with
flat dynamic holes, which is very close to this whole thesis. And on *this
platform*, **Mpx** has done compile-time dependency analysis to minimize `setData`
for years.

What I haven't seen elsewhere is the next section: not optimizing what crosses the
bridge, but proving some of it never needs to cross at all.

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

**This is dead-code elimination's data twin.** A classic DCE pass asks "is this
value ever used?" Mine asks "is this value ever used *by the other thread*?" The
template is the liveness boundary; `data` is the live-out set; anything not live
across that boundary gets demoted to a thread-local field.

A runtime provably cannot do this, because it doesn't have the template's
read-set before the data is committed. That's the whole argument for compilers in
one example.

## 7. The bugs

**The bug that only appeared on a real device — and the price of span-rewriting.**
My dead-data optimization emitted mutations as
`(this._todos[i].done = x, rt.touch(this))` — spliced in on the line after an
unterminated statement. JavaScript's automatic semicolon insertion
parsed it as *a function call on the previous line's result*. Every tap threw.
Tests passed (they had semicolons). Only the real device showed it. The fix is
one character: emit `;(…)`.

**A yuan sign broke my parser.** `¥` — the RMB sign, on a Chinese platform, in an
expense tracker: about as predictable as a character gets — is two bytes in UTF-8, and my
whitespace-trimming code sliced at a byte index — panicking mid-character. Found
by writing a demo app with prices in it. This is the argument for dogfooding in
one line.

**Is 115 tests enough?** No. Every compilation rule has a test pinning the
*emitted output* — and the benchmark runs as a test too, so CI fails on a number,
not a crash. But I have 41 error paths and dedicated tests for maybe a third; no
fuzzing, no property tests. When I audited coverage for this post I wrote tests
for a dozen untested paths and every one already passed — which makes them
regression insurance, not bug discovery. **None of the bugs that actually bit me
were found by a unit test.** They were found by running the thing on a real
device.

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

### Where the speed actually comes from

Here's a correction I owe you, because I got it wrong in my own head first.

Looking at the 1000-row list — **26 B vs 67 B** per toggle, **68 ms vs 162 ms**
tap latency — it's tempting to draw a line between them. Don't. **Forty-one bytes
cannot cost 94 milliseconds.** At that scale the bridge crossing is a fixed cost
both frameworks pay once per tap; the marginal cost of 41 extra bytes is
microseconds.

The latency comes from somewhere better. Per tap, Taro runs:

```js
setTodos((ts) => ts.map((t) => (t.id === id ? { ...t, done: !t.done } : t)))
```

That allocates **1000 new objects**, invalidates the `useMemo`, re-filters 1000
items, re-renders 1000 elements, and reconciles 1000 vnodes against the DOM shim —
all on the logic thread. Mist's compiled equivalent:

```js
const i = this._todos.findIndex(t => t.id === id)
;(this._todos[i].done = !this._todos[i].done, rt.touch(this))
```

A `findIndex` and a boolean flip. Mist still does O(n) comparison work in the
diff, but it's *comparisons*, not allocations, and there's no vnode tree.

So there are **two independent wins with different causes**: fewer bytes (because
the compiler sends state instead of rendered output) and lower latency (because
the compiler deleted the reconciler). Conflating them would be the easy story;
they're separate, and the second one is the bigger deal.

This also explains the result that embarrassed me: **Taro is faster on a 100-item
list.** Reconciler cost scales with node count, so below some n it's cheaper than
my diff bookkeeping, and my advantage disappears. Under a "bytes are everything"
theory that data point is inexplicable. Under the right theory it's obvious.

### Everything else

| metric (workload) | **Mist** | Taro |
|---|---|---|
| bytes/toggle (1000-row list) | **26 B** | 67 B |
| tap p50 (1000-row list) | **68 ms** | 162 ms |
| initial page payload (form page) | **366 B** | 1,259 B |
| filter switch (list changes length) | 32 KB ✗ | smaller ✓ |
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
- **Mist has a real architectural weak spot** (it's in the table above). When a
  derived list changes *length*, my keyed diff can't do per-item writes and falls
  back to sending the whole array — 32 KB on a filter switch. Taro's tree diff
  degrades more gracefully. Filtering a list is not an edge case in an expense
  tracker, so this is a genuine hole, not a footnote.
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

*Mist is a prototype. The compiler, language spec, demo apps, benchmark harness,
and the Taro twin used for these comparisons are all at
**[github.com/TadejPolajnar/mist](https://github.com/TadejPolajnar/mist)** —
including the eval methodology, so you can tell me where I'm wrong.*
