# 雾板 MistBoard — kanban board

A team kanban board. This app stresses keyed list reordering.

<img src="screenshot.png" width="320" alt="MistBoard board page" />

## What it shows

- Columns of cards with move buttons. A reorder ships two path-precise
  `order` writes in one `setData`.
- The M1009 pattern: per-card handlers live in a real `KanbanCard`
  component, because a component boundary resets template loop depth.
- A cross-store derived: the board page reads the `board`, `team` and
  `prefs` stores in one derived, including through a helper function.
- WIP-limit flags, a backlog pool, and a card detail page with the
  `missing`-guard pattern for `?id=` pages.
- Three persisted stores and a `theme.css` token file.

## Run it

```sh
mistc build examples/kanban/src -o examples/kanban/dist
```

Open `examples/kanban` in WeChat DevTools. The gate suite is
`tests/examples_kanban.rs`.
