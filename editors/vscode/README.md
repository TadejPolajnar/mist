# Mist for VS Code

Syntax highlighting and language server integration for `.mist` single-file
components: TypeScript frontmatter between `---` fences, JSX-ish template, and
embedded `<style>` CSS.

## Install (local dev)

Symlink into your extensions directory and reload VS Code:

```sh
ln -s "$(pwd)/editors/vscode" ~/.vscode/extensions/mist-lang
```

Or package a `.vsix`:

```sh
npx @vscode/vsce package
code --install-extension mist-lang-0.1.0.vsix
```

## What it does

- `.mist` files get a `mist` language mode.
- Frontmatter highlights as TypeScript, the template as TSX, `<style>` blocks
  as CSS — all via VS Code's built-in grammars, so themes and semantic colors
  work unchanged.
- Bracket/quote auto-closing and `//` / `/* */` comment toggling.
- With `mistc-lsp` on your PATH (or `mist.lspPath` set): M-code diagnostics as
  you type, completions for state/derived/method/store names in templates plus tags, attributes, events and component props inside markup,
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
