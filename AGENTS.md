# AGENTS.md — working on mistc

Guide for AI agents and contributors. Read this before changing code. `README.md`
covers the user-facing story; this file covers how the repo actually works.

## What this is

`mistc` compiles Astro-flavored `.mist` single-file components (TS frontmatter +
JSX-ish template + Tailwind) into native WeChat Mini Program code
(WXML/WXSS/JS/JSON). Core thesis: **everything statically analyzable → every
state mutation compiles to a path-precise `setData`** — no vdom, no runtime tree
diffing, ~9.6 KB runtime. `SPEC.md` is the language design; `benchmark/` proves the
thesis (49 B/toggle in the Node harness, 26 B on-device, guarded by `tests/bench.rs`).

## Architecture map

~8.7k lines of Rust (8,669 in `src/`). Deps: `oxc_{allocator,parser,ast,span}` **pinned 0.36**
(bump in lockstep; span semantics are load-bearing) + `regex`. No serde — JSON is
hand-emitted.

| Module | Lines | Owns |
|---|---|---|
| `src/lib.rs` | ~1411 | Orchestration: `compile_project_dir` (directory → `Layout::Nested`), `compile_project` (entry file → `Layout::Flat`), `compile_rec` (per-unit recursion, inline decisions, store compilation, style merging), `assemble_wxss`, `build_json`. Embeds the runtime via `include_str!("../runtime/mist-rt.js")`. |
| `src/frontmatter.rs` | ~3291 | The heart. oxc-parses frontmatter TS; `Analysis` (states/deriveds/methods/lifecycles/props/imports/store_imports/config); **span-based rewriting** (never AST codegen): `MutationCollector` (oxc `Visit`) produces precise `Edit`s for writes, `Rewriter` regex-sweeps reads/calls; `emit_js` (Page/Component), `emit_app_js`, `compile_store_module`, `config_literal_to_json`. Also **dead-data elimination** (`StateDecl::bound` — state the template never reads becomes `this._x`, never entering `data`) and `hoisted_deriveds` (generated deriveds for hoisted template expressions). |
| `src/template.rs` | ~975 | Hand-rolled recursive-descent template parser → `Node` tree (`Element/Text/Expr/For/If`); `.map()`→For, `&&`→If via top-level-aware scanning; `wx:key` validation (M1003); tree queries (`for_lists`, `has_slot`, `has_events`). |
| `src/wxml.rs` | ~842 | WXML emission: tag mapping (`div`→`view`…, `a href`→`navigator url`), event compilation (`onTap[:catch\|:mut]`, inline arrows → `_eN` handlers + `data-a*`), component vs inline-template use sites, class sanitization routing, **`value:bind`** (→ `model:value` + `__vb_<name>` handler), **`class:list`** (string/`cond && 'x'`/object entries → one class attr with WXML ternaries; exclusive with `class`), **expression hoisting** (page-scope `_h<i>`, per-item `_hl<i>` lists carrying `_c<i>` computed fields). `Handler` is the wxml↔js contract. |
| `src/tailwind.rs` | ~210 | Class extraction from templates + name sanitization (`w-[32px]`→`w-_32px_`) — must stay byte-identical between markup and CSS selectors. |
| `src/tailwind_cli.rs` | ~670 | Runs real `@tailwindcss/cli` v4 (npm-installed into `~/.cache/mistc/tw4`, per-invocation `io-<pid>-<counter>` subdirs; post-processed output **memoized per class-set hash** in `css-<hash>/` dirs) and rewrites v4 CSS for WXSS: `@layer` unwrap, `@property`→var substitution, `:root,:host`→`page`, `oklch()`→hex, `color-mix`→`rgba`, media ranges→min/max-width, rem→rpx (1rem=32rpx), allowlist selector filter, `page{}` theme split. |
| `src/sfc.rs` | ~51 | Splits `---` frontmatter / template / `<style>`; parses `<style scoped>`; records 1-based line offsets so diagnostics report real file positions. |
| `src/scope.rs` | ~237 | `<style scoped>`: per-unit class suffixing (`.card`→`.card--<name>`) applied identically to WXSS selectors (comments stripped first; recursing into `@media`/`@supports`, skipping `@keyframes`) and to markup `class`/`hover-class`/`placeholder-class` values (space-anchored attr match) incl. quoted literals inside WXML ternaries; `scope_class_expr` rewrites literals (incl. template-literal text) in hoisted class expressions (`WxmlOutput::class_hoists`) before `hoisted_deriveds` bakes them into JS. Class names built inside frontmatter functions are not rewritten (documented limit). |
| `src/tag_meta.rs` | ~429 | Hand-curated per-tag metadata for ~25 everyday native components (miniprogram-api-typings has no per-component attribute tables, so generation is impossible): `TagMeta { tag, attrs, events }`, `COMMON_EVENTS` (camelCase, matched lowercase so M1023 suggestions read as `onScrollToLower`), `UNIVERSAL_ATTRS`, `meta_for`/`valid_*`/`suggest_*` (Levenshtein ≤2). Absent tags skip validation — staleness never breaks builds. Shared surface for the compiler warnings and (future) LSP completions. |
| `src/main.rs` | ~539 | clap CLI: `mistc build <dir\|file> [-o out] [--app] [--watch]` (notify-based watcher, 120ms debounce, output dir excluded), `mistc init <name>` (scaffolds src/app.mist + todo page + sample test + project.config.json), and `mistc test [dir] [--filter s] [--timeout secs]` (compiles src/ to a temp dir, runs tests/*.test.js via node with the embedded `runtime/mist-test.js` harness — per-file timeout kills hung tests, default 30s — exits non-zero on failure); writes the dist tree, records it in `dist/.mist-manifest` and prunes stale outputs; prints warnings. |
| `runtime/mist-test.js` | ~154 | The `mistc test` Node harness (embedded via `include_str!`, written into the temp dist as `.mist-test-runner.js`): globals `bootPage(name, {query, setDataLimit})` (requires the compiled page, installs a recording `setData` that applies patches path-precisely and throws over the limit to exercise runtime rollback; returns `{page, data(), patches, lastPatch(), totalBytes()}`), `flush(ms)`, `load(name)`, `resetModules()`, `appHide()`, and a Proxy `wx` stub (Map-backed storage as `wx.__storage`, every other call a recorded no-op in `wx.__calls`). Runs one `tests/*.test.js` file passed as argv, awaiting its `module.exports`. Node-only logic harness — no WXML rendering (see docs/testing.md). |
| `runtime/mist-rt.js` | ~380 | `set/touch/flush` (microtask-batched setData; `touch(page, name?)` = derive-only flush for unbound state), `derive(page, out, name, key, compute, deps)` (keyed **field-level** diff vs `__prev` snapshots; **per-derived dirty bits** — `deps` from `frontmatter::derived_deps` (resolves through pure methods and store accessors), clean deriveds skipped, changed deriveds re-dirty for chains; `deps` omitted/null ⇒ always recompute; **transactional flush** — mirror/`__prev` roll back if `setData` throws, next flush recomputes all), `applyPath`, `store/bindStores/unbindStores` (cross-page path-precise notifications), `observePerf/perfEntries` (launch metrics, wired into generated `app.js`). |

### Pipeline (directory build)

```
main.rs → compile_project_dir: read app.mist, discover pages/*.mist (index first),
  discover pages/<dir>/[<param>].mist route-param pages (discover_route_param_pages;
  error on collision with pages/<dir>.mist; repeated per subpackage),
  warn M1016 on dropped pages/<dir>/*.mist, discover src/packages/<pkg>/pages/*.mist
  (each pkg validated: alphanumeric/-/_, not a reserved dist path),
  discover optional src/custom-tab-bar.mist (compiled at forced out_path
  "custom-tab-bar/index", CUSTOM_TAB_BAR_DEPTH=1, via compile_rec_at)
  └─ per page: compile_rec (subpackage pages compiled at SUBPKG_DEPTH via Layout)
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
  └─ compile_app (app.js/app.json/app.wxss — main-package pages → "pages",
     subpackage pages grouped by package → "subPackages"; M1020 checks
     config.tabBar.custom against custom-tab-bar.mist presence) → finish_project
     (tailwind over union of classes; unknown/dropped reporting) → main writes files
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
  refs, out paths) for Flat vs Nested; **depth-aware** — relative prefixes are
  computed from the out_path's depth below dist root, so subpackage pages at
  `packages/<pkg>/pages/<n>/<n>` (depth 4) get `../../../../` while main pages
  (depth 2) get `../../`; never hardcode a `../..` outside it.
- **Class sanitization** must remain identical in markup (`tailwind::sanitize`)
  and CSS selectors (`tailwind_cli::transform_selector` via `sanitize_char`).
- **Path-precise mutation table** (the product): assignment/update/compound/`push`
  on `x.value…` → `this.__set(path, …)`; store targets → `__S<n>.<store>.__set`.
  `pop/splice/shift/unshift/sort/reverse` are M1004 errors by design.

## Workflows

```sh
cargo build                                   # debug binary
cargo run -- build examples/project/src -o dist    # canonical example
cargo run -- build examples/project/src -o dist --watch   # rebuild on save
cargo run -- init my-app                      # scaffold a new project
cargo run -- test my-app                      # run the app's tests/*.test.js in the Node harness
cargo test                                    # 200+ tests — spawns node, npm, npx
cargo test --test compile                     # pure-Rust subset (no node needed)
node benchmark/bench.js                       # bridge-traffic benchmark
```

DevTools: Import Project → **repo root** (tracked `project.config.json` has
`miniprogramRoot: "dist/"`, AppID `touristappid`). After a rebuild, hit compile
in DevTools.

Editor support: `editors/vscode/` is a VS Code extension — TextMate grammar +
language config for TS/TSX/CSS highlighting via built-in grammars, plus a thin
LSP client (`client.js`, needs `npm install` for `vscode-languageclient`) that
spawns `mistc-lsp` over stdio (PATH or `mist.lspPath` setting; degrades to
highlighting-only if absent). `crates/mistc-lsp` is a workspace binary crate
(tower-lsp): **incremental** text sync (`apply_change` applies UTF-16 range
edits to the stored doc; on change, diagnostics are debounced 150ms with a
per-URI generation counter so stale computes never publish), then
`sfc::split` + `compile_unit_with_stores` (store imports resolved from disk
relative to the file; no Tailwind — that only runs in `finish_project`),
regex-parses `M#### at line L:C` out of the error string into LSP
diagnostics, and serves completions, hover, go-to-definition (store symbols
resolve into their module file), signature help, and whole-word rename from
a `Symbol` table built per request via `frontmatter::analyze_with_stores`
plus textual decl lookup — declaration positions and derived-source hovers
come from regex over the frontmatter text, not spans. **Cross-file store
rename**: renaming a store symbol from a page walks up to the `app.mist`
src root, scans `.mist`/`.ts` project files (open-buffer versions win over
disk), matches importers by canonical store path (`imports_store`), and
returns a multi-file `WorkspaceEdit` (`store_rename_edits`). Grammar and
LSP must track template/frontmatter syntax changes.

Commits: **Conventional Commits, subject line only.** History pattern per
milestone: `feat:` → subagent review → `fix: address … review findings`.

## Gotchas (hard-won — respect these)

1. **Tests hit real external tools.** `npm`/`npx` (Tailwind v4 install + run) and
   `node` (runtime, stores, bench tests). No network or no Node ⇒ those suites
   fail *environmentally*, not because you broke something. The npm cache lives
   at `~/.cache/mistc/tw4` ($TMPDIR fallback when HOME is unset; delete it if
   Tailwind output seems stale). Post-processed CSS is memoized there per
   class-set hash (`css-<hash>/`), so identical class sets skip the subprocess;
   a Tailwind/npm failure is a build **warning** (non-CSS files still emit),
   not an error.
2. **Never delete `dist/` while WeChat DevTools has the project open** — it
   orphans the DevTools watcher ("fork process timeout") and loses
   `dist/project.private.config.json`. Rebuild in place; `mistc` records what
   it emits in `dist/.mist-manifest` and prunes files a previous build emitted
   that the current one didn't — it never touches files it didn't write.
3. **`runtime/mist-rt.js` is embedded at Rust compile time.** Editing it requires
   a `cargo build` before `dist/mist-rt.js` changes. It must keep
   `tests/runtime.rs` and the `tests/bench.rs` regression guard green; the
   benchmark numbers are quoted in **seven** places — `README.md`, `README.zh-CN.md`, `benchmark/README.md`,
   `benchmark/devtools/README.md`, `benchmark/devtools/EVAL.md`, `docs/api.md`,
   `BLOG.md` — update together if they move.
4. **SPEC.md ≠ documentation of behavior.** It predates the code and ~half is
   design-only. Grep `src/` before claiming a feature exists. Conversely, SPEC
   §14 records resolved design decisions (boxes over `$state` magic, `value:bind`
   planned, per-item hoisting, `miniprogram-api-typings`) — don't relitigate.
5. **Diagnostics are prefixed `String`s, not a typed enum.** Allocated codes:
   M1001 (aliased state mutation incl. for-of/forEach params, line:col + help),
   M1002 (unknown class, warn), M1003 (bad wx:key), M1004 (non-compilable
   mutation, line:col + help), M1005 (name collision), M1006 (dropped selector,
   warn), M1007 (reactive use without `.value`, line:col + help), M1008
   (keyless reactive list, warn — flows through `Unit.warnings`/`Project.warnings`),
   M1009 (call in nested loop), M1010 (template syntax, has line), M1011 (unknown/aliased/default 'mist'
   import, line:col + help), M1012 (config enables a feature with no declared
   hook, warn), M1013 (hook declared in the wrong unit kind — page/component/app,
   line:col + help; app-only hooks are `onError`, `onPageNotFound`,
   `onUnhandledRejection`, `onThemeChange`; component-only hooks are `onCreate`,
   `onAttach`, `onDetach`, `onMove`, `onPageShow`, `onPageHide`; page-only hooks
   additionally include `onRouteDone`, `onSaveExitState`), M1014 (config key
   collides with a compiler-generated JSON field — `pages`/`subPackages`/
   `sitemapLocation` in app.mist, `component`, or `usingComponents` when the
   unit also imports a `.mist` component), M1015 (invalid `plugin://` import
   shape or `pluginComponents` entry — named/aliased plugin imports,
   empty/invalid plugin names, non-`plugin://` `pluginComponents` values, and a
   `pluginComponents` name colliding with an imported `.mist` component's tag),
   M1016 (`pages/<dir>/` contains `.mist` files that are not compiled — pages
   must sit directly in `pages/`, or in `packages/<pkg>/pages/` for
   subpackages, warn), M1017 (state write inside `onCreate` — `created` runs
   before `properties`/`data` exist on the instance, line:col + help; detected
   by intersecting the `onCreate` callback's span with `MutationCollector`
   edits), M1018 (element mapping to native `text` has a non-text/non-`text`
   child — box styling on that child silently no-ops, warn; recurses through
   `wx:if`/`wx:else` and list children at that position), M1019 (tag not in
   `NATIVE_TAGS`, the web-alias table, a registered `.mist`/plugin component,
   or manual `usingComponents`/`config.customTags` — WeChat renders unknown
   tags as nothing, warn; did-you-mean via inline Levenshtein when a
   candidate is within edit distance 2, deduped per distinct tag per unit),
   M1020 (custom tab bar file/config mismatch — `config.tabBar.custom: true`
   without `src/custom-tab-bar.mist` is an error, the file without the flag is
   a warning; detected via `frontmatter::config_tab_bar_custom`, an AST walk
   beside `config_top_level_keys`), M1021 (unknown `navigate()` route, or a
   non-literal route argument — always line:col; route-list validation is
   late, in `compile_project_dir` once every page/subpackage page is known,
   with a Levenshtein "did you mean" within edit distance 3;
   `navigate.switchTab` checks against `app.mist`'s `tabBar.list[].pagePath`
   instead when that's statically extractable via
   `frontmatter::config_tab_bar_page_paths`; flat/single-file builds compile
   `navigate()` calls with no route-list check at all — no route set exists).
   M1001–M1025 allocated (M1023 unknown native event / M1024 unknown native attribute — driven by `src/tag_meta.rs`, suppressed via `config.customAttrs`; M1025 `[param].mist` route page missing its `const <param> = state(...)`).
   Frontmatter TS is type-stripped (`strip_types`, whitespace-
   preserving blanking) before analysis, so annotations never reach emitted JS.
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

## Test suite map (~360 tests; counts drift — trust cargo)

| File | Covers |
|---|---|
| `compile.rs` (110) | mutation compilation, derived, lifecycles, template emission, TS stripping, M1008, M1018, M1019 |
| `runtime.rs` (20) | real Node execution: batching, keyed diff, stores notify/unbind |
| `stores.rs` (14) | store module compilation, page wiring, collisions, export rules |
| `tailwind_cli.rs` (10) + `tailwind_v4.rs` (16) + `tailwind.rs` (10) | WXSS post-processor fixtures + real-CLI e2e (skips gracefully offline) |
| `project.rs` (37) | directory builds, app shell, nested paths, config JSON, guards, M1021 route validation |
| `inline.rs` (9) | pure-render inlining, slots, multipleSlots |
| `components.rs` (25) | properties, callback events, usingComponents |
| `diagnostics.rs` (78) | M-code positions and messages, M1001/M1007/M1009/M1021, npm guard |
| `crates/mistc-lsp` (14 unit + 1 e2e) | LSP helpers incl. incremental sync + cross-file rename; full stdio protocol session via a Node driver |
| `bench.rs` (1) | performance regression guard over `benchmark/bench.js` |
| `todo.rs` (1) | single-file smoke test |
| `edge_cases.rs` (14) | error paths, unicode, deep paths, config edge cases |
| `cli.rs` (11) | --version/--help, unknown-command suggestion, init scaffold compiles, mist-routes.d.ts emission |
| `examples_food.rs` / `examples_portfolio.rs` / `examples_kanban.rs` (1 each) | example-app gate suites: warning-free builds, output invariants, Node boot assertions on compiled pages |

## Implemented vs SPEC (summary)

**Implemented:** §1–§3 (files, SFC, state/derived/props/lifecycles/config,
path-precise mutations, batching, keyed **field-level** diff, stores,
**dead-data elimination §8.5**), §4.1/4.3/4.4 (tags, control flow, events),
**§4.2 all tiers** (inline + page-scope hoisted `_h<i>` + per-item `_c<i>`
computed fields; nested loops excluded), **§4.5 `value:bind`** (model:value +
sync handler), §5 (components, callback props, slots), §6 (Tailwind v4;
warnings not errors), §7 (app shell; navigator + query-param routing; tab bar
via app.mist config), §8.1–8.3, §9, §11 (incl. M1001 aliased-mutation
analysis — scope-aware `BindingScope` tracking: innermost binding wins,
rebinds cancel, covers for-of and iterator-callback params).

**Design-only (do not assume these exist):** static *subtree*
hoisting (§8.4 — distinct from §4.2 expression hoisting, which ships); setData budget chunking;
`mist.config.ts` as a file; `<style global>`
(`<style scoped>` ships — per-unit `--<name>` suffixing in `src/scope.rs`);
`mist trace`; package-size budgets; npm interop;
snapshot testing; hoisting inside nested loops.

`navigate()` **is implemented** (plan 026): `import { navigate } from 'mist'`
compiles `navigate(route, params?)` / `.replace` / `.back` / `.switchTab` to
`wx.navigateTo`/`redirectTo`/`navigateBack`/`switchTab`; route arguments must
be string literals (M1021), validated against the compiled page list for
directory builds only (see M1021 above). `mistc build <dir>` also emits
`mist-routes.d.ts` next to an existing `mist.d.ts`, narrowing the `Route`
type — `types/mist.d.ts` (the static `init` scaffold) deliberately does
**not** declare `navigate`, because a second looser `declare module 'mist'`
augmentation of the same function silently wins over the generated file's
narrow `Route` union (confirmed with `tsc`) instead of erroring or
intersecting — so `navigate` exists only in the generated file today.
`[id].mist` route files ship: `pages/<dir>/[<param>].mist` compiles to
`pages/<dir>/<dir>` with a generated missing-param guard + query seeding
(`emit_js_route`) and a `RouteParams` entry in `mist-routes.d.ts` (M1025 when
the param state is missing; collision with `pages/<dir>.mist` is an error).
SPEC §7's `navigate('/pages/todo/[id]', …)` example predates this design and
does not work — the route argument is always the plain compiled path
(`/pages/todo/todo`); brackets never survive into routes.
