# 雾讯 MistFeed — setData budget lab

A long-feed stress app. It holds ~1.1 MB of posts in page memory and shows
what does — and deliberately does not — cross the `setData` bridge.

<img src="screenshot.png" width="320" alt="MistFeed feed page" />

## What it shows

- **Dead-data elimination at scale**: 1000 posts (~1.1 MB) live as unbound
  page state. The template never reads them, so they never enter `data`.
  Only the visible slice does — the first page costs ~58 KB.
- **Path-precise writes at scale**: a like on any of 1000 rows ships a
  ~48-byte patch.
- **The rejection path, on purpose**: the 压测台 tab has a "全量渲染" switch
  that forces all 1000 rows into one `setData`. The write exceeds WeChat's
  1 MB limit and is rejected; the runtime rolls the page back, resyncs store
  mirrors, and damps the retry. Restore paging and the app continues.
- Paged rendering through `onReachBottom` with a store-controlled page size.

## Run it

```sh
mistc build examples/feed/src -o examples/feed/dist
```

Open `examples/feed` in WeChat DevTools, then watch the Console while you
press 全量渲染. The gate suite is `tests/examples_feed.rs` — it simulates the
1 MB rejection in Node against the compiled page.

## Snapshots

This app also carries committed compiler goldens in `snapshots/`:

```sh
mistc test examples/feed --snapshots   # diff emitted output vs the goldens
mistc test examples/feed --update      # accept intentional codegen changes
```

Any compiler upgrade that changes the emitted WXML/JS/WXSS shows up here as
per-file drift before it reaches DevTools.
