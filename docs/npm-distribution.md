# Handoff: Distribute `mistc` on npm

Status: implemented but not yet published — the npm/ tree, shim and release workflow exist; packages appear on npm with the first tagged release. Until then, install with cargo (see the README quickstart).

## Goal

Users install the compiler with `npm install -g mist-lang`. The install puts the `mistc` command on their PATH. Users do not need a Rust toolchain.

## Why npm

The target users are WeChat mini-program developers. They work in the Node ecosystem and most do not have Rust installed. The current install path (`cargo install --path .`) blocks adoption.

## Decisions already made

These decisions are final. Do not reopen them without cause.

- **Distribution pattern**: prebuilt binaries in per-platform npm packages, selected with `optionalDependencies`. This is the esbuild and Biome pattern.
- **No postinstall download**: do not fetch binaries from GitHub Releases at install time. Many users are behind Chinese registry mirrors, and GitHub downloads fail there. Packages on the registry flow through npmmirror without problems.
- **Names**: `mist-lang` for the meta package, `@mist-lang/<platform>` for the binary packages. Both were free on the npm registry on 2026-08-08. The names `mist`, `mistc`, and `mist-cli` are taken.
- **Command name**: the installed command is `mistc`, independent of the package name.
- **No workspace tool**: the `npm/` directory holds plain folders. Do not add pnpm or npm workspaces. The packages have no shared dependencies and no JS build step.
- **Versioning**: all seven packages share one version, pinned exactly. They publish together in lockstep.

## Prerequisites (manual, needs the npm account owner)

1. Create the `mist-lang` organization on npmjs.com to claim the `@mist-lang` scope.
2. Create an npm automation token. Add it to the GitHub repo as the `NPM_TOKEN` secret.

## Package layout

Create this directory tree in the repo root:

```
npm/
├── mist-lang/                  # meta package, the one users install
│   ├── package.json
│   ├── bin/mistc.js            # shim: resolve platform package, spawn binary
│   └── README.md               # short: install line + link to main README
├── darwin-arm64/
│   ├── package.json
│   └── bin/                    # CI drops the binary here; keep with .gitkeep
├── darwin-x64/
├── linux-x64-gnu/
├── linux-arm64-gnu/
├── linux-x64-musl/
└── win32-x64/                  # binary name is mistc.exe
```

Binaries are never committed. Add `npm/*/bin/mistc*` to `.gitignore`.

### Meta package `npm/mist-lang/package.json`

```json
{
  "name": "mist-lang",
  "version": "0.1.0",
  "description": "Mist compiler for WeChat mini-programs",
  "bin": { "mistc": "bin/mistc.js" },
  "license": "MIT",
  "repository": { "type": "git", "url": "<repo url>" },
  "optionalDependencies": {
    "@mist-lang/darwin-arm64": "0.1.0",
    "@mist-lang/darwin-x64": "0.1.0",
    "@mist-lang/linux-x64-gnu": "0.1.0",
    "@mist-lang/linux-arm64-gnu": "0.1.0",
    "@mist-lang/linux-x64-musl": "0.1.0",
    "@mist-lang/win32-x64": "0.1.0"
  }
}
```

### Platform package, example `npm/darwin-arm64/package.json`

```json
{
  "name": "@mist-lang/darwin-arm64",
  "version": "0.1.0",
  "os": ["darwin"],
  "cpu": ["arm64"],
  "license": "MIT"
}
```

The `os` and `cpu` fields make npm skip the package on other platforms. On the two Linux x64 packages, add `"libc": ["glibc"]` and `"libc": ["musl"]` to separate them. Newer npm and pnpm versions read this field. The shim is the fallback for older versions.

### Shim `npm/mist-lang/bin/mistc.js`

```js
#!/usr/bin/env node
const { execFileSync } = require("node:child_process");

function isMusl() {
  if (process.platform !== "linux") return false;
  const report = process.report.getReport();
  return !report.header.glibcVersionRuntime;
}

const key = `${process.platform}-${process.arch}${isMusl() ? "-musl" : ""}`;
const pkg = {
  "darwin-arm64": "@mist-lang/darwin-arm64",
  "darwin-x64": "@mist-lang/darwin-x64",
  "linux-x64": "@mist-lang/linux-x64-gnu",
  "linux-x64-musl": "@mist-lang/linux-x64-musl",
  "linux-arm64": "@mist-lang/linux-arm64-gnu",
  "win32-x64": "@mist-lang/win32-x64",
}[key];

if (!pkg) {
  console.error(`mistc: unsupported platform ${key}`);
  console.error("mistc: install with cargo instead: cargo install --git <repo url>");
  process.exit(1);
}

let bin;
try {
  bin = require.resolve(`${pkg}/bin/mistc${process.platform === "win32" ? ".exe" : ""}`);
} catch {
  console.error(`mistc: ${pkg} is not installed.`);
  console.error("mistc: reinstall without --no-optional, and make sure that the npm version is 7 or newer.");
  process.exit(1);
}

try {
  execFileSync(bin, process.argv.slice(2), { stdio: "inherit" });
} catch (e) {
  process.exit(e.status ?? 1);
}
```

Note: the shim uses CommonJS on purpose. It must run on old Node versions without ESM friction. Do not convert it to ESM.

## Release workflow

Create `.github/workflows/npm-release.yml`. Trigger: push of a tag that matches `v*`.

### Job 1: build matrix

One job per target:

| npm directory | Rust target | Runner |
|---|---|---|
| darwin-arm64 | aarch64-apple-darwin | macos-14 |
| darwin-x64 | x86_64-apple-darwin | macos-13 |
| linux-x64-gnu | x86_64-unknown-linux-gnu | ubuntu-22.04 |
| linux-arm64-gnu | aarch64-unknown-linux-gnu | ubuntu-22.04-arm |
| linux-x64-musl | x86_64-unknown-linux-musl | ubuntu-22.04 |
| win32-x64 | x86_64-pc-windows-msvc | windows-2022 |

Each matrix job:

1. Run `rustup target add <target>`.
2. Run `cargo build --release --target <target> --bin mistc`.
3. Copy `target/<target>/release/mistc` (or `mistc.exe`) into `npm/<dir>/bin/`.
4. Upload `npm/<dir>` as an artifact.

Known problem areas:

- The musl build usually needs `musl-tools` (`apt-get install musl-tools`). If the crate links C code, use the `cross` tool instead.
- Build linux-arm64 on an arm runner or with `cross`. Plain cross-compilation on x64 fails at the linker step.

### Job 2: publish

Runs after all matrix jobs succeed.

1. Download all artifacts into `npm/`.
2. Read the version from the tag (`v0.1.0` → `0.1.0`).
3. Stamp this version into all seven `package.json` files. Update the version and every `optionalDependencies` entry.
4. Publish the six platform packages: `npm publish --access public --provenance` in each directory.
5. Publish `npm/mist-lang` last. This order makes sure that the meta package never points at missing binaries.

`--access public` is required. Scoped packages default to private. `--provenance` needs `id-token: write` permission on the job.

## Verification

After the first release:

1. On a clean machine or container, run `npm install -g mist-lang`.
2. Run `mistc --version`. Make sure that the output matches the tag.
3. Run `mistc build examples/project/src -o dist`. Make sure that the build succeeds.
4. Repeat on at least one Linux container (`node:20-alpine` covers the musl path) and one Windows machine.
5. Run `npm install -g mist-lang --registry=https://registry.npmmirror.com` to test the mirror path. The mirror can lag some hours behind.

## Documentation updates (after the release works)

- `README.md` and `README.zh-CN.md`: add `npm install -g mist-lang` as the first install option. Keep `cargo install` as the contributor path.
- `AGENTS.md`: record the `npm/` directory and the release workflow.

## Out of scope

- A JS API wrapper or Vite plugin. If one appears later, that is the point to add a pnpm workspace.
- Homebrew, cargo-binstall, or other channels.
- Windows arm64 and other minor targets. Add them when users ask.
