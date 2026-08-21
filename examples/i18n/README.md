# 雾语 Mist i18n — runtime language switching

The [docs/i18n.md](../../docs/i18n.md) recipe as a runnable app: a persisted
locale store, a plain `t` helper, and one page that flips between English
and Chinese live.

## What it shows

- `stores/locale.ts` — the whole i18n layer: a `locale` store
  (persisted, so the choice survives restarts), the dictionary as a plain
  module constant, and pure `t`/`setLocale` helpers.
- Template calls like `{t('greet.title')}` hoist to generated deriveds, so
  every visible string re-renders the moment the locale changes — no
  compiler i18n machinery involved.
- The page imports `locale` alongside `t`; that import is what subscribes
  the page. Drop it and the strings freeze at first render.
- Config strings can't be localized statically, so `onShow` retitles the
  nav bar through `wx.setNavigationBarTitle`.

## Run it

```sh
mistc build examples/i18n/src -o examples/i18n/dist
mistc test examples/i18n
```

The test flips the locale from Node and asserts the hoisted strings and the
nav-bar retitle both follow.
