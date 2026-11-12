#!/usr/bin/env node
// Set the app's version in one place.
//
//   npm run set-version -- 1.2.0
//
// tauri.conf.json reads its version from package.json, so this updates
// package.json, the Cargo workspace, and the lockfiles that record both, so
// that `npm ci` and `cargo build --locked` keep working.

import { existsSync, readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!version || !/^\d+\.\d+\.\d+$/.test(version)) {
  console.error("Usage: npm run set-version -- 1.2.0   (major.minor.patch)");
  process.exit(1);
}

const pkg = JSON.parse(readFileSync("package.json", "utf8"));
pkg.version = version;
writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");

const cargo = readFileSync("Cargo.toml", "utf8");
const next = cargo.replace(
  /(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/,
  `$1${version}$2`,
);
if (next === cargo && !cargo.includes(`version = "${version}"`)) {
  console.error("Could not find version under [workspace.package] in Cargo.toml.");
  process.exit(1);
}
writeFileSync("Cargo.toml", next);

if (existsSync("package-lock.json")) {
  const lock = JSON.parse(readFileSync("package-lock.json", "utf8"));
  lock.version = version;
  if (lock.packages?.[""]) lock.packages[""].version = version;
  writeFileSync("package-lock.json", JSON.stringify(lock, null, 2) + "\n");
}

// The workspace's own crates are the [[package]] entries with no `source`.
if (existsSync("Cargo.lock")) {
  const lock = readFileSync("Cargo.lock", "utf8");
  const updated = lock
    .split("\n[[package]]\n")
    .map((block) =>
      block.startsWith("name = ") && !/\nsource = /.test(block)
        ? block.replace(/\nversion = "[^"]+"/, `\nversion = "${version}"`)
        : block,
    )
    .join("\n[[package]]\n");
  writeFileSync("Cargo.lock", updated);
}

console.log(`Version is now ${version}. Commit, then tag it: git tag v${version} && git push --tags`);
