<p align="center">
  <img src="docs/assets/cover.png" alt="Mist — Mini-app Static Templates" width="720">
</p>

<h1 align="center">Mist</h1>

<p align="center">
  <strong>A component language and compiler for WeChat Mini Programs.</strong><br>
  Write Astro-flavored single-file components. Ship native mini-program code with
  path-precise <code>setData</code> and a ~6&nbsp;KB runtime.
</p>

<p align="center">
  <img alt="status" src="https://img.shields.io/badge/status-prototype-orange">
  <img alt="rust" src="https://img.shields.io/badge/built%20with-Rust-dea584">
  <img alt="tests" src="https://img.shields.io/badge/tests-115%20passing-brightgreen">
  <img alt="runtime" src="https://img.shields.io/badge/runtime-6%20KB-blue">
  <img alt="license" src="https://img.shields.io/badge/license-MIT-lightgrey">
</p>

---

```jsx
---
import { state, derived } from 'mist'

const filter = state('all')
const todos  = state([{ id: 1, title: 'Ship it', done: false }])
const open   = derived(() => todos.value.filter(t => !t.done))

function toggle(id) {
  const i = todos.value.findIndex(t => t.id === id)
  todos.value[i].done = !todos.value[i].done   // → setData({'todos[3].done': true})
}
---
<div class="p-4 flex flex-col gap-2">
  <span class="text-2xl font-bold">{open.value.length} open</span>
  {open.value.map(t => (
    <div key={t.id} onTap={() => toggle(t.id)}>{t.title}</div>
  ))}
</div>
```

## Why

WeChat Mini Programs run your JavaScript on one thread and render on another. The
only channel between them is `setData`, which serializes to JSON and crosses a
bridge. Every framework on this platform is ultimately judged by how many bytes
it pushes through that pipe.

Mist answers that at compile time. There is no virtual DOM and no runtime tree
diffing — the compiler statically tracks every mutation and emits the exact data
path it changes.

## Features

| | |
|---|---|
| **Path-precise `setData`** | `todos.value[i].done = x` compiles to a single path write, not a tree diff |
| **Dead-data elimination** | State your template never reads never enters `data` — zero bridge cost |
| **Keyed field-level diffing** | A changed list item ships only its changed fields |
| **Automatic batching** | Every mutation in one event tick merges into one `setData` |
| **Components & slots** | Props, callback props → native events, compile-time inlining of pure-render components |
| **Cross-page stores** | Shared reactive state in plain `.ts` modules; subscription glue generated |
| **Two-way inputs** | `value:bind={text}` → native `model:value`, no setData echo |
| **Expression hoisting** | Function calls in templates become generated deriveds, per-item inside loops |
| **Real Tailwind v4** | The actual CLI, post-processed for WXSS (`rem`→`rpx`, `oklch()`→hex, …) |
| **Diagnostics** | Numbered `M`-codes with source line:col and fix-it hints |

## Install

Requires **Rust**, **Node.js + npm** (Tailwind and tests), and **WeChat DevTools**.

```sh
git clone https://github.com/TadejPolajnar/mist && cd mist
cargo build --release        # → target/release/mistc
cargo install --path .       # optional: puts `mistc` on your PATH
```

## Quick start

```
my-app/
├── app.mist              # app lifecycle, global config, global styles
├── pages/index.mist      # index becomes the launch page
├── components/Card.mist  # PascalCase → kebab-case components
└── stores/cart.ts        # shared reactive state
```

```sh
mistc build my-app -o dist
```

Then open the project in WeChat DevTools — `project.config.json` points
`miniprogramRoot` at `dist/`.

```
mistc build <src-dir | entry.mist> [-o <outdir>] [--app]
```

## Benchmarks

Measured in real WeChat DevTools against **Taro 3.6.35 + React 18**, using one
instrument that hooks `setData` outside either framework.

| | Mist | Taro 3 |
|---|---|---|
| bytes per interaction *(form page)* | **15 B** | 180 B |
| bytes per toggle *(1000-row list)* | **26 B** | 67 B |
| tap latency p50 *(1000-row list)* | **68 ms** | 162 ms |
| initial page payload | **366 B** | 1,259 B |
| incremental build | **0.40 s** | 4.0 s |
| framework runtime | **6 KB** | ~230 KB |

Measured in DevTools, not on a physical device. Taro is *faster* on small lists
(57 ms vs 67 ms at 100 items) — the advantage opens up as data grows. Full
numbers, methodology, and the cases where Mist loses:
[benchmark/devtools/EVAL.md](benchmark/devtools/EVAL.md).

## Documentation

- **[Getting started](docs/README.md)** — install, first app, the build loop
- **[Language guide](docs/language.md)** — reactivity, templates, components, stores, styling
- **[API reference](docs/api.md)** — the `mist` module, CLI, emitted output, runtime
- **[Diagnostics](docs/diagnostics.md)** — every `M`-code with its fix
- **[Design spec](SPEC.md)** — full language design *(aspirational; `docs/` describes what ships)*
- **[Architecture](AGENTS.md)** — compiler internals, contracts, gotchas

## Examples

| | |
|---|---|
| [`examples/ledger`](examples/ledger) | Dark-themed expense tracker — stores, forms, tab bar, detail pages |
| [`examples/project`](examples/project) | Multi-page app with components, stores, navigation |
| [`examples/todo.mist`](examples/todo.mist) | Single-file page |

## How it works

```
.mist ──┬─ frontmatter (TypeScript) ──→ oxc parser ──→ span-based rewriting
        ├─ template (JSX-ish)       ──→ recursive-descent parser ──→ WXML
        └─ <style>                  ──→ WXSS  ⋯  Tailwind v4 CLI → post-processor
                                              ↓
                          pages/ · components/ · stores/ · mist-rt.js
```

~3,600 lines of Rust. No IR: source → AST walk → edit list → text, because every
transform is local and syntax-directed. Details in [AGENTS.md](AGENTS.md).

## Status

**Prototype.** The core language is complete and validated on-device — reactivity,
components, slots, inlining, stores, Tailwind v4, project builds, diagnostics —
with 115 tests. Not published to a registry; not recommended for production.

Not implemented: `M1001` alias analysis, `[id].mist` route files, `<style>`
scoping, `class:list`, npm imports in frontmatter, hoisting inside nested loops,
LSP. WeChat-only — the emitter is abstracted internally, but no other mini-program
platform is targeted.

## License

MIT
