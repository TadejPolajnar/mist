# 雾账 MistLedger — expense tracker

A dark-themed spending tracker: budget ring on the home tab, an add-expense
form, per-category stats, and a detail page per transaction.

## What it shows

- One persisted store (`stores/ledger.ts`, versioned envelope) driving four
  pages — every tab renders the same transactions through its own deriveds.
- A derived chain from raw transactions to budget percentage, capped at 100,
  plus category rollups with percentage bars on the stats tab.
- Per-item call hoisting: `fmtDate(t.ts)` on the transaction list compiles
  to a `_c` computed field on a generated derived; the scalar
  `fmtMoney(...)` calls hoist to page-scope `_h` deriveds.
- Form state with a `valid` derived gating the submit button.
- A native tab bar configured entirely from `app.mist`.

## Run it

```sh
mistc build examples/ledger/src -o examples/ledger/dist
mistc test examples/ledger
```

Open `examples/ledger` in WeChat DevTools. `tests/ledger.test.js` boots the
compiled pages in Node, adds and removes transactions through the store, and
asserts on `setData` payloads.
