# 雾茶 MistTea — food ordering

A tea-shop ordering app. This is the flagship mist example.

<img src="screenshot.png" width="320" alt="MistTea home page" />

## What it shows

- Six pages: home, menu, item detail, cart, checkout, orders.
- Two persisted stores (`cart`, `orders`). The orders store uses a `version`
  bump with `migrate`.
- A checkout subpackage (`src/packages/order/`) with a `preloadRule`.
- Tab-bar icons and a search `sitemap.json` through the asset pipeline.
- App lifecycle hooks: `onError`, `onPageNotFound`, `onUnhandledRejection`,
  `onThemeChange`.
- A `theme.css` design-token file. The templates use no arbitrary Tailwind
  values.
- Cross-tab navigation intent through a small `ui` store, because
  `wx.switchTab` cannot carry query parameters.

## Run it

```sh
mistc build examples/food/src -o examples/food/dist
```

Open `examples/food` in WeChat DevTools. The gate suite for this app is
`tests/examples_food.rs`.
