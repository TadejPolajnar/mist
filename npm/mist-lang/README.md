# mist-lang

Compiler for `.mist` single-file components. It targets native WeChat Mini Program code.

## Install

```sh
npm install -g mist-lang
```

The install puts the `mistc` command on your PATH. A Rust toolchain is not necessary.

## Use

```sh
mistc build src -o dist
```

## Supported platforms

| Platform | Architecture |
|---|---|
| macOS | arm64, x64 |
| Linux (glibc) | x64, arm64 |
| Linux (musl) | x64 |
| Windows | x64 |

The correct binary installs automatically for your platform.

## Documentation

See the [project README](../../README.md).

## License

MIT
