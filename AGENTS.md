# AGENTS.md — working on mistc

Guide for AI agents and contributors. Read this before changing code. `README.md`
covers the user-facing story; this file covers how the repo actually works.

## What this is

`mistc` compiles Astro-flavored `.mist` single-file components (TS frontmatter +
JSX-ish template + Tailwind) into native WeChat Mini Program code
(WXML/WXSS/JS/JSON). Core thesis: **everything statically analyzable → every
state mutation compiles to a path-precise `setData`** — no vdom, no runtime tree
diffing, ~6 KB runtime. `SPEC.md` is the language design; `benchmark/` proves the
thesis (49 B/toggle in the Node harness, 26 B on-device, guarded by `tests/bench.rs`).

## Architecture map

~3.6k lines of Rust (3,574 in `src/`). Deps: `oxc_{allocator,parser,ast,span}` **pinned 0.36**
(bump in lockstep; span semantics are load-bearing) + `regex`. No serde — JSON is
hand-emitted.

| Module | Lines | Owns |
|---|---|---|
| `src/lib.rs` | ~590 | Orchestration: `compile_project_dir` (directory → `Layout::Nested`), `compile_project` (entry file → `Layout::Flat`), `compile_rec` (per-unit recursion, inline decisions, store compilation, style merging), `assemble_wxss`, `build_json`. Embeds the runtime via `include_str!("../runtime/mist-rt.js")`. |
| `src/frontmatter.rs` | ~1350 | The heart. oxc-parses frontmatter TS; `Analysis` (states/deriveds/methods/lifecycles/props/imports/store_imports/config); **span-based rewriting** (never AST codegen): `MutationCollector` (oxc `Visit`) produces precise `Edit`s for writes, `Rewriter` regex-sweeps reads/calls; `emit_js` (Page/Component), `emit_app_js`, `compile_store_module`, `config_literal_to_json`. Also **dead-data elimination** (`StateDecl::bound` — state the template never reads becomes `this._x`, never entering `data`) and `hoisted_deriveds` (generated deriveds for hoisted template expressions). |
| `src/template.rs` | ~430 | Hand-rolled recursive-descent template parser → `Node` tree (`Element/Text/Expr/For/If`); `.map()`→For, `&&`→If via top-level-aware scanning; `wx:key` validation (M1003); tree queries (`for_lists`, `has_slot`, `has_events`). |
| `src/wxml.rs` | ~500 | WXML emission: tag mapping (`div`→`view`…, `a href`→`navigator url`), event compilation (`onTap[:catch\|:mut]`, inline arrows → `_eN` handlers + `data-a*`), component vs inline-template use sites, class sanitization routing, **`value:bind`** (→ `model:value` + `__vb_<name>` handler), **expression hoisting** (page-scope `_h<i>`, per-item `_hl<i>` lists carrying `_c<i>` computed fields). `Handler` is the wxml↔js contract. |
| `src/tailwind.rs` | ~100 | Class extraction from templates + name sanitization (`w-[32px]`→`w-_32px_`) — must stay byte-identical between markup and CSS selectors. |
| `src/tailwind_cli.rs` | ~440 | Runs real `@tailwindcss/cli` v4 (npm-installed into `$TMPDIR/mistc-tw4`, per-invocation `io-<pid>-<counter>` subdirs) and rewrites v4 CSS for WXSS: `@layer` unwrap, `@property`→var substitution, `:root,:host`→`page`, `oklch()`→hex, `color-mix`→`rgba`, media ranges→min/max-width, rem→rpx (1rem=32rpx), allowlist selector filter, `page{}` theme split. |
| `src/sfc.rs` | ~47 | Splits `---` frontmatter / template / `<style>`; records 1-based line offsets so diagnostics report real file positions. |
| `src/main.rs` | ~100 | CLI: `mistc build <dir\|file> [-o out] [--app]`; writes the dist tree; prints M1002/M1006 warnings. |
| `runtime/mist-rt.js` | ~230 | `set/touch/flush` (microtask-batched setData; `touch` = derive-only flush for unbound state), `derive` (keyed **field-level** diff vs `__prev` snapshots), `applyPath`, `store/bindStores/unbindStores` (cross-page path-precise notifications), `observePerf/perfEntries` (launch metrics, wired into generated `app.js`). |

### Pipeline (directory build)

```
main.rs → compile_project_dir: read app.mist, discover pages/*.mist (index first)
  └─ per page: compile_rec
       ├─ resolver closure (reads+classifies store modules relative to unit dir)
       ├─ inline-decision pass: is_inlinable(child)? (pure-render only)
       ├─ compile_unit_full:
       │    sfc::split → frontmatter::analyze_with_stores → template::parse_at
       │    → wxml::emit (handlers, used components/inline)
       │    → derived_keys from template::for_lists (wx:key per derived)
       │    → frontmatter::emit_js → build_json → assemble_wxss
       ├─ compile store modules (deduped; out-path collision check)
       ├─ recurse into used components (Component or Template kind)
       └─ merge inlined-child styles into parent wxss
  └─ compile_app (app.js/app.json/app.wxss) → finish_project (tailwind over union
     of classes; unknown/dropped reporting) → main writes files
```

### Contracts to never break

- **`reactive` name list** (states + deriveds + store mirrors) drives *both* the
  template's `.value`-stripping and the JS read rewriting — markup and JS must
  agree on what lives in `data`.
- **`Handler`**: `wxml::emit_event` mints `_eN` names + `data-aN` attrs;
  `emit_js` materializes matching methods (`from_detail` ⇒ unpack `e.detail.args`).
- **`derived_keys`** aligns positionally with `analysis.deriveds` and carries the
  template's `wx:key` into `rt.derive` — it decides per-index vs whole-key writes.
- **`Layout`** answers every path question (rt require, tw imports, component
  refs, out paths) for Flat vs Nested; never hardcode a `../..` outside it.
- **Class sanitization** must remain identical in markup (`tailwind::sanitize`)
  and CSS selectors (`tailwind_cli::transform_selector` via `sanitize_char`).
- **Path-precise mutation table** (the product): assignment/update/compound/`push`
  on `x.value…` → `this.__set(path, …)`; store targets → `__S<n>.<store>.__set`.
  `pop/splice/shift/unshift/sort/reverse` are M1004 errors by design.

## Workflows

```sh
cargo build                                   # debug binary
cargo run -- build examples/project/src -o dist    # canonical example
cargo test                                    # 100+ tests — spawns node, npm, npx
cargo test --test compile                     # pure-Rust subset (no node needed)
node benchmark/bench.js                       # bridge-traffic benchmark
```

DevTools: Import Project → **repo root** (tracked `project.config.json` has
`miniprogramRoot: "dist/"`, AppID `touristappid`). After a rebuild, hit compile
in DevTools.

Commits: **Conventional Commits, subject line only.** History pattern per
milestone: `feat:` → subagent review → `fix: address … review findings`.

## Public API boundary

`src/lib.rs` keeps the compiler's modules `pub(crate)` unless the `internals`
feature is on. The stable surface is `compile`, `compile_unit`, `compile_project`,
`compile_project_dir`, `RUNTIME`, and the output types. Integration tests reach
into internals via the `dev-dependencies` self-reference with `features =
["internals"]` — if you add a test that touches a private module, that's why it
resolves.

## Gotchas (hard-won — respect these)

1. **Tests hit real external tools.** `npm`/`npx` (Tailwind v4 install + run) and
   `node` (runtime, stores, bench tests). No network or no Node ⇒ those suites
   fail *environmentally*, not because you broke something. The npm cache lives
   at `$TMPDIR/mistc-tw4` (outside the repo; survives `git clean`; delete it if
   Tailwind output seems stale).
2. **Never delete `dist/` while WeChat DevTools has the project open** — it
   orphans the DevTools watcher ("fork process timeout") and loses
   `dist/project.private.config.json`. Rebuild in place; note `mistc` overwrites
   but does not clean, so renamed/removed pages leave stale files.
3. **`runtime/mist-rt.js` is embedded at Rust compile time.** Editing it requires
   a `cargo build` before `dist/mist-rt.js` changes. It must keep
   `tests/runtime.rs` and the `tests/bench.rs` regression guard green; the
   benchmark numbers are quoted in **six** places — `README.md`, `benchmark/README.md`,
   `benchmark/devtools/README.md`, `benchmark/devtools/EVAL.md`, `docs/api.md`,
   `BLOG.md` — update together if they move.
4. **SPEC.md ≠ documentation of behavior.** It predates the code and ~half is
   design-only. Grep `src/` before claiming a feature exists. Conversely, SPEC
   §14 records resolved design decisions (boxes over `$state` magic, `value:bind`
   planned, per-item hoisting, `miniprogram-api-typings`) — don't relitigate.
5. **Diagnostics are prefixed `String`s, not a typed enum.** Allocated codes:
   M1002 (unknown class, warn), M1003 (bad wx:key), M1004 (non-compilable
   mutation, has line:col + help), M1005 (name collision), M1006 (dropped
   selector, warn), M1010 (template syntax, has line). **M1001 is reserved** for
   aliased-mutation analysis (unimplemented); M1007–M1009 are unallocated.
   `tests/diagnostics.rs` asserts on message substrings — reformatting breaks it.
6. **Test temp dirs are fixed-name** (`$TMPDIR/mist-store-writes`,
   `mist-app-guards`, …). New tests needing temp dirs must pick unique names;
   stale dirs from aborted runs can cross-contaminate.
7. **Tailwind v3 and the builtin CSS engine were deliberately removed**
   (commit `d71b8e1`). v4 CLI is the only path; don't reintroduce fallbacks.
   Two output sheets by design: `tw-shared.wxss` (utilities, imported by every
   unit — the style-isolation fix) and `tw-theme.wxss` (`page{}` variables,
   **pages only** — tag selectors are illegal in component WXSS).
8. **Two `project.config.json`s**: the tracked repo-root one (DevTools entry,
   points at `dist/`) and the one `--app` synthesizes inside flat builds. Don't
   conflate or "fix" either.
9. **oxc 0.36 pinned** across four crates; upgrade all together and expect
   span/API churn — the span-based rewriting makes this the riskiest dependency.
10. **PascalCase component filenames** → kebab-case output (`TodoItem.mist` →
    `components/todo-item/todo-item.*`); pages keep their stem.

## Test suite map (~115 tests; counts drift — trust cargo)

| File | Covers |
|---|---|
| `compile.rs` (33) | mutation compilation, derived, lifecycles, template emission basics |
| `runtime.rs` (9) | real Node execution: batching, keyed diff, stores notify/unbind |
| `stores.rs` (10) | store module compilation, page wiring, collisions, export rules |
| `tailwind_cli.rs` (10) + `tailwind_v4.rs` (7) + `tailwind.rs` (4) | WXSS post-processor fixtures + real-CLI e2e (skips gracefully offline) |
| `project.rs` (9) | directory builds, app shell, nested paths, config JSON, guards |
| `inline.rs` (7) | pure-render inlining, slots, multipleSlots |
| `components.rs` (4) | properties, callback events, usingComponents |
| `diagnostics.rs` (6) | M-code positions and messages |
| `bench.rs` (1) | performance regression guard over `benchmark/bench.js` |
| `todo.rs` (1) | single-file smoke test |
| `edge_cases.rs` (14) | error paths, unicode, deep paths, config edge cases |

## Implemented vs SPEC (summary)

**Implemented:** §1–§3 (files, SFC, state/derived/props/lifecycles/config,
path-precise mutations, batching, keyed **field-level** diff, stores,
**dead-data elimination §8.5**), §4.1/4.3/4.4 (tags, control flow, events),
**§4.2 all tiers** (inline + page-scope hoisted `_h<i>` + per-item `_c<i>`
computed fields; nested loops excluded), **§4.5 `value:bind`** (model:value +
sync handler), §5 (components, callback props, slots), §6 (Tailwind v4;
warnings not errors), §7 (app shell; navigator + query-param routing; tab bar
via app.mist config), §8.1–8.3, §9, §11 (partial M-codes).

**Design-only (do not assume these exist):** M1001 alias analysis; static *subtree*
hoisting (§8.4 — distinct from §4.2 expression hoisting, which ships); setData budget chunking; `[id].mist` route files; `navigate()`
helper; `mist.config.ts` as a file; `class:list`; `<style>` scoping / `global`;
inlining opt-out config; `mist trace`; package-size budgets; LSP; npm interop;
snapshot testing; hoisting inside nested loops.
