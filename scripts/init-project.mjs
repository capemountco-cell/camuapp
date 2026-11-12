#!/usr/bin/env node
// Turn this template into your app. Run once, right after cloning.
//
//   npm run init -- --name "My App" --id com.yourname.myapp
//
// Optional: --slug my-app  (crate and package names; derived from --name)
//
// It rewrites a fixed list of files and nothing else, then prints what it
// changed. Run `git diff` afterwards to see every edit.

import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";

const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce((pairs, arg, i, all) => (arg.startsWith("--") ? [...pairs, [arg.slice(2), all[i + 1]]] : pairs), []),
);

function usage(message) {
  if (message) console.error(`error: ${message}\n`);
  console.error('Usage: npm run init -- --name "My App" --id com.yourname.myapp [--slug my-app]');
  process.exit(1);
}

const name = args.name?.trim();
const id = args.id?.trim();
if (!name || !id) usage();

// A name becomes the .app bundle, the .dmg volume and the menu bar title.
if (/[/:\\"]/.test(name)) usage('the name cannot contain / : \\ or ".');
if (name.length > 40) usage("keep the name under 40 characters; it is the volume name of the .dmg.");

// Apple requires reverse-DNS: letters, digits, hyphens and dots, two parts or more.
if (!/^[A-Za-z][A-Za-z0-9-]*(\.[A-Za-z0-9-]+)+$/.test(id)) {
  usage("the id must look like com.yourname.myapp (letters, digits, hyphens, dots).");
}
if (id.toLowerCase().endsWith(".app")) usage('the id should not end in ".app"; macOS treats that specially.');
if (id === "com.example.starter") usage("pick your own id, not the template's.");

const slug = (args.slug ?? name)
  .toLowerCase()
  .replace(/[^a-z0-9]+/g, "-")
  .replace(/^-+|-+$/g, "");
if (!/^[a-z][a-z0-9-]*$/.test(slug)) usage(`could not make a crate name from "${name}"; pass --slug.`);
const snake = slug.replace(/-/g, "_");

const conf = readFileSync("src-tauri/tauri.conf.json", "utf8");
if (!conf.includes('"identifier": "com.example.starter"')) {
  console.error("This project has already been initialised; nothing to do.");
  process.exit(0);
}

// Token replacements, longest and most specific first.
const tokens = [
  ["com.example.starter", id],
  ["starter_core", `${snake}_core`],
  ["starter_lib", `${snake}_lib`],
  ["starter-core", `${slug}-core`],
];

const edits = [
  ["package.json", [['"name": "starter"', `"name": "${slug}"`]]],
  ["package-lock.json", [['"name": "starter"', `"name": "${slug}"`]]],
  ["index.html", [["<title>Starter</title>", `<title>${escapeHtml(name)}</title>`]]],
  ["src/App.tsx", [["<h1>Starter</h1>", `<h1>${escapeHtml(name)}</h1>`]]],
  ["Cargo.toml", []],
  ["Cargo.lock", [['name = "starter"\n', `name = "${slug}"\n`]]],
  ["crates/starter-core/Cargo.toml", []],
  ["src-tauri/Cargo.toml", [['name = "starter"\n', `name = "${slug}"\n`]]],
  ["src-tauri/src/main.rs", []],
  ["src-tauri/src/lib.rs", []],
  [
    "src-tauri/tauri.conf.json",
    [
      ['"productName": "Starter"', `"productName": ${JSON.stringify(name)}`],
      ['"title": "Starter"', `"title": ${JSON.stringify(name)}`],
    ],
  ],
  [".github/workflows/ci.yml", []],
  [".github/workflows/release.yml", []],
];

const changed = [];
for (const [file, specific] of edits) {
  if (!existsSync(file)) continue;
  let text = readFileSync(file, "utf8");
  const before = text;
  for (const [from, to] of specific) text = text.split(from).join(to);
  for (const [from, to] of tokens) text = text.split(from).join(to);
  if (text !== before) {
    writeFileSync(file, text);
    changed.push(file);
  }
}

if (existsSync("crates/starter-core")) {
  renameSync("crates/starter-core", `crates/${slug}-core`);
  changed.push(`crates/starter-core -> crates/${slug}-core`);
}

console.log(`Initialised "${name}" (${id}).\n`);
for (const file of changed) console.log(`  updated ${file}`);
console.log(`
Next:
  1. Replace src-tauri/app-icon.png with your 1024x1024 icon, then: npm run icons
  2. npm run doctor
  3. npm run dev
  4. npm run dmg
  5. git add -A && git commit -m "Initialise ${name}"`);

function escapeHtml(value) {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
