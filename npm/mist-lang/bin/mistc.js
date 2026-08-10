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
  console.error("mistc: install with cargo instead: cargo install --git https://github.com/TadejPolajnar/mist");
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
