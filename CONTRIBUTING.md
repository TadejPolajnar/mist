# Contributing

Thanks for your interest in Mist.

**Start with [AGENTS.md](AGENTS.md).** It is the canonical guide to the
codebase: the architecture map, the compile pipeline, the contracts you must
not break, the diagnostics registry, and the test suite map. Everything below
is logistics.

## Working locally

```sh
cargo build
cargo test              # full suite; needs Node.js + npm on PATH
cargo test --test compile   # pure-Rust subset, no Node needed
```

The first build downloads the Tailwind v4 CLI (~20 MB, one time, into
`~/.cache/mistc`). Offline builds still work — CSS generation degrades with a
warning.

## Pull requests

- Keep changes small and focused; one concern per PR.
- Every behavior change needs a test. Bug fixes need a regression test that
  fails without the fix.
- New diagnostics take the next free `M` code (see the registry in AGENTS.md)
  and a section in `docs/diagnostics.md`.
- User-facing changes update `docs/` — and ideally the `zh-CN` variants; if
  you can't write Chinese, say so in the PR and leave the English change only.
- CI must pass (`cargo build` + `cargo test` on Linux and macOS).
- Conventional commit subjects (`feat:`, `fix:`, `docs:`, `test:`, `ci:`).

## Reporting bugs

Open an issue with a minimal `.mist` file that reproduces the problem and the
exact `mistc` output. Compiler bugs with reproductions get fixed fast — the
example apps in `examples/` have caught fifteen of them.

## Security

See [SECURITY.md](SECURITY.md) — please do not report vulnerabilities in
public issues.
