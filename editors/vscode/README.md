# Mist for VS Code

Syntax highlighting and language server integration for `.mist` single-file
components: TypeScript frontmatter between `---` fences, JSX-ish template, and
embedded `<style>` CSS.

## Install

Search for **Mist** (`tadejpolajnar.mist-lang`) in the VS Code Marketplace, or:

```sh
code --install-extension tadejpolajnar.mist-lang
```

For the full language server (diagnostics, completions, rename), also install
the compiler so `mistc-lsp` is on your PATH:

```sh
npm install -g mist-lang        # ships mistc and mistc-lsp (v0.3.0+)
```

(On older releases without a bundled `mistc-lsp`, build it from a repo clone:
`cargo install --path crates/mistc-lsp`.) Without it the extension degrades
gracefully to highlighting only.

## Install (local dev)

Symlink into your extensions directory and reload VS Code:

```sh
ln -s "$(pwd)/editors/vscode" ~/.vscode/extensions/mist-lang
```

Or package a `.vsix` yourself:

```sh
npm ci && npx @vscode/vsce package
code --install-extension mist-lang-0.3.0.vsix
```

Releases: pushing a `vscode-v<version>` tag (matching `package.json`) runs
`.github/workflows/vscode-release.yml` — packages the `.vsix`, publishes to
the VS Code Marketplace (`VSCE_PAT` secret) and OpenVSX (`OVSX_PAT`,
best-effort), and attaches the `.vsix` to a GitHub release.

## What it does

- `.mist` files get a `mist` language mode.
- Frontmatter highlights as TypeScript, the template as TSX, `<style>` blocks
  as CSS — all via VS Code's built-in grammars, so themes and semantic colors
  work unchanged.
- Bracket/quote auto-closing and `//` / `/* */` comment toggling.
- With `mistc-lsp` on your PATH (or `mist.lspPath` set): M-code diagnostics as
  you type (re-checking importing pages workspace-wide when a store or component file changes — a deleted component flags its importers), completions for state/derived/method/store names in templates plus tags, attributes, events and component props inside markup,
  hover cards (state init, derived source, method signatures), go-to-definition
  (including into store modules), signature help, and rename for local
  state/derived/method/prop names.

## Language server

```sh
cargo build -p mistc-lsp
```

The client spawns `mistc-lsp` over stdio. It looks up the binary on PATH; point
`mist.lspPath` at `target/debug/mistc-lsp` (or an installed copy) otherwise. If
the binary is missing the extension quietly falls back to highlighting only.

Packaging note: `npm install` first — `vscode-languageclient` must be inside
the `.vsix`.
