# Mist documentation

Mist is a **component language and compiler for WeChat Mini Programs**: you write
Astro-flavored single-file `.mist` components; `mistc` (Rust) compiles them to
plain WXML/WXSS/JS with path-precise `setData` and a ~6 KB runtime.

These docs describe **what is implemented today**. The full language design lives
in [../SPEC.md](../SPEC.md) — parts of it are still design-only; when the two
disagree, these docs are right.

- **[Getting started](#getting-started)** (below)
- **[Language guide](language.md)** — file anatomy, reactivity, templates, components, stores, styling
- **[API reference](api.md)** — everything importable from `'mist'`, the CLI, emitted output
- **[Diagnostics](diagnostics.md)** — every `M`-code, with fixes

## Getting started

Prerequisites: Rust, Node.js + npm (Tailwind + tests), WeChat DevTools.

```sh
git clone https://github.com/TadejPolajnar/mist && cd mist
cargo build --release          # → target/release/mistc
cargo install --path .         # optional: puts `mistc` on your PATH
```

The examples below assume `mistc` is on your PATH. Without `cargo install`, use
`cargo run -- build …` or `./target/release/mistc build …` instead.

Create a project:

```
my-app/
├── app.mist
└── pages/
    └── index.mist
```

`app.mist` — app lifecycle, global config, global styles (no template, no state):

```
---
import { onLaunch } from 'mist'
export const config = { window: { navigationBarTitleText: 'My App' } }
onLaunch(() => console.log('launched'))
---
```

`pages/index.mist`:

```
---
import { state } from 'mist'
export const config = { navigationBarTitleText: 'Counter' }

const count = state(0)

function inc() {
  count.value++
}
---
<div class="p-4 flex flex-col gap-2">
  <span class="text-2xl font-bold">{count.value}</span>
  <button class="rounded-full bg-blue-500 text-white" onTap={inc}>+1</button>
</div>
```

Build and run:

```sh
mistc build my-app -o dist
# WeChat DevTools → Import Project → folder containing a project.config.json
# with "miniprogramRoot": "dist/" (or import dist/ directly after adding one)
```

Every save: rebuild, then let DevTools recompile. That's the loop.

## What to read next

Skim [language.md](language.md) top to bottom — it's short and covers everything
that exists. Keep [diagnostics.md](diagnostics.md) open the first time the
compiler rejects something; every error has a written fix.
