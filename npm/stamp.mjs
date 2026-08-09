import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+/.test(version ?? "")) {
  console.error("usage: node npm/stamp.mjs <version>");
  process.exit(1);
}

const root = dirname(fileURLToPath(import.meta.url));
const platforms = [
  "darwin-arm64",
  "darwin-x64",
  "linux-x64-gnu",
  "linux-arm64-gnu",
  "linux-x64-musl",
  "win32-x64",
];

for (const dir of platforms) {
  const path = join(root, dir, "package.json");
  const pkg = JSON.parse(readFileSync(path, "utf8"));
  pkg.version = version;
  writeFileSync(path, `${JSON.stringify(pkg, null, 2)}\n`);
}

const metaPath = join(root, "mist-lang", "package.json");
const meta = JSON.parse(readFileSync(metaPath, "utf8"));
meta.version = version;
for (const name of Object.keys(meta.optionalDependencies)) {
  meta.optionalDependencies[name] = version;
}
writeFileSync(metaPath, `${JSON.stringify(meta, null, 2)}\n`);

console.log(`stamped ${version} into ${platforms.length + 1} packages`);
