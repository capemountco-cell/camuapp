<!-- LOVABLE:BEGIN -->
> [!IMPORTANT]
> This project is connected to [Lovable](https://lovable.dev). Avoid rewriting
> published git history — force pushing, or rebasing/amending/squashing commits
> that are already pushed — as it rewrites history on Lovable's side and the
> user will likely lose their project history.
>
> Commits you push to the connected branch sync back to Lovable and show up in
> the editor, so keep the branch in a working state.

## What Lovable manages

Rules outside this block are the project's own. Change the pieces below in
Lovable, not by hand. With the [Lovable MCP](https://mcp.lovable.dev)
connected, `send_message` asks Lovable to do it; otherwise ask the user to
use the editor.

- **Database.** Do not add or edit `supabase/migrations/` or `drizzle/`.
  Ask Lovable to change the schema.
- **Generated files.** Do not edit a file whose header says it is automatically
  generated.
- **AI gateway.** Keep model calls on Lovable's gateway, and keep
  `LOVABLE_API_KEY` out of browser code.
- **Connectors.** Lovable provides native connectors. `list_connectors` shows
  which are enabled; `add_connector` only returns the dashboard link. When a
  connector is enabled, calls go through the connector gateway and need
  `LOVABLE_API_KEY`. When Lovable does not provide one, users can bring their
  own key and store it in Secrets.
- **Secrets.** Lovable manages them. Do not put secret values in the repo.
<!-- LOVABLE:END -->

# Working on this app

This repository started from the Mac App Template. These rules keep it able
to produce a working `.dmg` at all times.

## Rules

**Logic goes in the core crate** (`crates/*-core`), never in Tauri command
handlers or React components. A command handler deserialises, calls one core
function, and returns. The core crate must not depend on Tauri, so its tests
run on Linux.

**Every UI call to Rust goes through `src/lib/native.ts`.** Components never
call `invoke` directly.

**Do not change these without reading docs/dmg.md:**
- `bundle.macOS.signingIdentity: "-"`. Without a signature a downloaded app
  is reported as "damaged".
- `"version": "../package.json"` in `tauri.conf.json`. Set versions only with
  `npm run set-version`.
- The dev port (1420) in both `tauri.conf.json` and `vite.config.ts`.
- `runs-on: macos-latest` in the workflows. Pinned images get retired, and a
  job naming one is cancelled with no logs.

**Permissions start at nothing.** Add Tauri capabilities in
`src-tauri/capabilities/default.json` and entitlements in
`src-tauri/entitlements.plist` one at a time, as features need them, with a
comment saying why.

**Do not add dependencies casually.** Prefer fifty lines of code to a library
for a trivial problem.

## Before pushing

```sh
npm run check
```

On a Mac, before anything that touches `src-tauri/`, packaging or icons:

```sh
npm run dmg
```

CI builds and verifies a `.dmg` on every push, so a packaging break shows up
on the pull request that caused it.
