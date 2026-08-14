# Security policy

## Reporting a vulnerability

Please report vulnerabilities privately through
[GitHub's private vulnerability reporting](https://github.com/TadejPolajnar/mist/security/advisories/new)
— not in public issues.

Reports are acknowledged within a week. Please include a minimal reproduction
and the version (`mistc --version` or the npm package version).

## Scope

Anything in this repository: the compiler (`mistc`), the runtime
(`mist-rt.js`), the language server, the VS Code extension, the npm packages
(`mist-lang`, `@mist-lang/*`), and the release workflow.

Of particular interest:

- Compiler input leading to emitted code that executes attacker-controlled
  logic beyond what the `.mist` source expresses.
- Path traversal through crafted project layouts, import specifiers, or
  asset/store paths.
- Supply-chain issues in the npm distribution or the release pipeline.

## Supported versions

The latest released version only. Fixes ship as a new release; there are no
backport branches.
