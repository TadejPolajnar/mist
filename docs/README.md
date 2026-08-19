# Mist documentation

[中文 → README.zh-CN.md](README.zh-CN.md)

Mist is a **component language and compiler for WeChat Mini Programs**: you write
Astro-flavored single-file `.mist` components; `mistc` (Rust) compiles them to
plain WXML/WXSS/JS with path-precise `setData` and a ~9 KB runtime.

These docs describe **what is implemented today**. The full language design lives
in [../SPEC.md](../SPEC.md) — parts of it are still design-only; when the two
disagree, these docs are right.

- **[Getting started](#getting-started)** (below)
- **[Language guide](language.md)** — file anatomy, reactivity, templates, components, stores, styling
- **[API reference](api.md)** — everything importable from `'mist'`, the CLI, emitted output
- **[Testing](testing.md)** — `mistc test`: boot compiled pages in Node, assert on state and `setData` payload sizes
- **[Diagnostics](diagnostics.md)** — every `M`-code, with fixes

## Getting started

Prerequisites: Node.js + npm (Tailwind + tests), WeChat DevTools.

```sh
npm install -g mist-lang       # mistc + mistc-lsp, prebuilt for macOS/Linux/Windows
```

Or from source (needs Rust):

```sh
git clone https://github.com/TadejPolajnar/mist.git && cd mist
cargo install --path . && cargo install --path crates/mistc-lsp
```

Scaffold, build, and iterate:

```sh
mistc init my-app              # app.mist + a todo page + a sample test + project.config.json
cd my-app
mistc build src --watch        # rebuilds on every save; ctrl-c to quit
mistc test                     # run tests/*.test.js in a Node harness
# WeChat DevTools → Import Project → select my-app/
```

Editor support: install **[Mist for WeChat Mini Programs](https://marketplace.visualstudio.com/items?itemName=tadejpolajnar.mist-lang)**
from the VS Code Marketplace (`tadejpolajnar.mist-lang`) — `.mist` syntax
highlighting plus LSP diagnostics, completions, hover, go-to-definition,
signature help and rename, powered by the `mistc-lsp` binary that
`npm install -g mist-lang` puts on your PATH.

`mistc --help` / `mistc build --help` document every flag.

Or create the files by hand:

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
mistc build my-app -o dist            # one-shot
mistc build my-app -o dist --watch    # rebuild on save
# WeChat DevTools → Import Project → folder containing a project.config.json
# with "miniprogramRoot": "dist/" (mistc init writes one for you)
```

For a hand-made project, this minimal `project.config.json` next to `dist/`
is all DevTools needs:

```json
{
  "miniprogramRoot": "dist/",
  "projectname": "my-app",
  "appid": "touristappid",
  "compileType": "miniprogram"
}
```

Save → auto-rebuild → let DevTools recompile. That's the loop.

## What to read next

The full-featured example is [`examples/food`](../examples/food) — a 6-page
food-ordering app (persisted cart, SKU picker, share hooks, native-styled
components). Build it with `mistc build examples/food/src -o examples/food/dist`
and import `examples/food/` in DevTools.

Skim [language.md](language.md) top to bottom — it's short and covers everything
that exists. Keep [diagnostics.md](diagnostics.md) open the first time the
compiler rejects something; every error has a written fix.
