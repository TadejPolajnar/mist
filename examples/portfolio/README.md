# 雾投 MistFolio — portfolio dashboard

A stock-portfolio dashboard. This app stresses the derived graph.

<img src="screenshot.png" width="320" alt="MistFolio dashboard" />

## What it shows

- A 13-node derived graph on one page: positions → per-position P&L →
  sector rollups → weighted totals → sparkline buckets → alert flags. It has
  diamond joins and skip-links.
- Keyed field-level diffing on the allocation and movers lists.
- A deterministic price-tick engine with integer-cent math.
- Weighted-average cost basis on buys, so a trade does not change P&L.
- Loop index (`wx:for-index`) for the mover ranks.
- Two persisted stores and a `theme.css` token file.

## Run it

```sh
mistc build examples/portfolio/src -o examples/portfolio/dist
```

Open `examples/portfolio` in WeChat DevTools. The gate suite is
`tests/examples_portfolio.rs`.
