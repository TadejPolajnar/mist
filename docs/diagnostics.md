# Diagnostics

Errors carry a code and a fix; M1004 includes file line:col and M1010 the line
(other codes report the file path only). Warnings (`M1002`, `M1006`) go to stderr and never fail the build.

## M1002 — unknown class (warning)

A template class produced no CSS. Usually a typo, a custom class you style in
`<style>` (fine — ignore), or a Tailwind utility that doesn't exist.

## M1003 — invalid list key

`wx:key` accepts only a **direct property** of the loop item, the item itself,
or `index`.

```jsx
{items.value.map(t => <li key={t.a + t.b}>…</li>)}   // ✗ computed
{items.value.map(t => <li key={t.meta.id}>…</li>)}   // ✗ deep path
{items.value.map(t => <li key={t.id}>…</li>)}        // ✓
{tags.value.map(t => <li key={t}>…</li>)}            // ✓ primitives → *this
{xs.value.map(x => <li key={index}>…</li>)}          // ✓ but disables keyed diffing
```

Fix: put a stable id on your items.

## M1004 — non-compilable array mutation

`pop / splice / shift / unshift / sort / reverse` on state can't compile to a
precise `setData` path.

```ts
items.value.splice(i, 1)                        // ✗ M1004
items.value = items.value.filter((_, j) => j !== i)   // ✓ one whole-key write
```

`push` and index assignment stay precise and are allowed.

## M1005 — name collision

State, derived, props, methods, store imports and store functions share one
namespace (they all become keys of the same object). Rename one of the two
reported declarations.

## M1006 — dropped selector (warning)

Tailwind produced a selector WXSS can't express (`hover:`, `:not()`, sibling
combinators like `space-x-*`, `@container`). The rule was removed. Fixes:
`space-x-* → gap-*`; interaction states → WeChat's `hover-class`; container
queries → `@media`.

## M1010 — template syntax error

Mismatched/unclosed tags and malformed attribute names — reported with the file
line. Common cause: a self-closing native tag written without `/>`.

## Un-coded errors worth knowing

- **"store modules require a project build"** — relative non-`.mist` imports
  only resolve in directory builds (`mistc build <dir>`).
- **"plain values cannot cross the page boundary"** — store modules export only
  `store()` values and functions; export a getter function instead of a const.
- **"app.mist cannot declare state / have a template"** — app shell is
  lifecycle + config + global styles only.
- **"config must be a static object literal"** — no function calls or variables
  inside `export const config`.
- **store output path collision** — two store files with the same stem; rename one.

## Silent hazard (until M1001 ships)

Aliasing state into a local and mutating through it —
`const t = todos.value[0]; t.done = true` — compiles to nothing reactive: the
write happens in memory but no `setData` is emitted. Always write through the
full path: `todos.value[0].done = true`. The `M1001` compile-time check for
this is on the roadmap.
