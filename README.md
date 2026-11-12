# Mac App Template

A starting point for native macOS apps that build a working `.dmg` from day
one. Tauri 2, Rust and React, with every known way the `.dmg` step fails
handled by the scripts rather than by you.

## Start a new app

Create an empty GitHub repository for the app, open a Claude chat with it
selected, and paste the prompt in [docs/new-app.md](docs/new-app.md). The
chat copies this template in, names it, and checks that the `.dmg` builds.

### Or do it yourself

1. On GitHub, click **Use this template** > **Create a new repository**.
2. Clone it, then name it:

   ```sh
   npm install
   npm run init -- --name "My App" --id com.yourname.myapp
   ```

3. Check this Mac has what it needs, and fix anything it lists:

   ```sh
   npm run doctor
   ```

4. Run it:

   ```sh
   npm run dev
   ```

5. Package it:

   ```sh
   npm run dmg
   ```

   The image lands at `release/My App-0.1.0-arm64.dmg`, already checked:
   it mounts, holds the app and an Applications link, and the signature is
   valid.

## Everyday commands

| Command | Does |
| --- | --- |
| `npm run dev` | Runs the app with live reload |
| `npm run check` | Type check, tests, Rust format, lint and tests. Run before every push |
| `npm run dmg` | Builds and verifies a `.dmg` for this Mac's chip |
| `npm run dmg:universal` | One `.dmg` for Apple silicon and Intel |
| `npm run dmg -- --plain` | Same, without the Finder window styling (over SSH, or if that step hangs) |
| `npm run verify-dmg -- path.dmg` | Checks any `.dmg` |
| `npm run icons` | Regenerates every icon size from `src-tauri/app-icon.png` (1024×1024) |
| `npm run set-version -- 1.2.0` | Sets the version everywhere it lives |
| `npm run doctor` | Lists anything missing on this machine, with the fix |

## Releasing

```sh
npm run set-version -- 1.0.0
git commit -am "Release 1.0.0"
git tag v1.0.0
git push && git push --tags
```

GitHub Actions builds a universal `.dmg` and attaches it to a GitHub release.
It is signed and notarized if you have added Apple credentials as secrets
([docs/signing.md](docs/signing.md)), and ad-hoc signed otherwise.

Every push also runs CI, which builds a `.dmg` on a real Mac and attaches it
to the run, so a change that breaks packaging is caught on the day it is made.

## Layout

```
crates/starter-core/   The app's logic. No Tauri, so it tests on any OS.
src-tauri/             The native shell: window, commands, bundling config.
  tauri.conf.json      Name, id, window, .dmg layout, signing.
  entitlements.plist   What the hardened runtime allows.
  app-icon.png         The one icon you edit; npm run icons makes the rest.
src/                   The React interface.
  lib/native.ts        Every call from the UI to Rust, in one place.
scripts/               doctor, init, build-dmg, verify-dmg, set-version.
docs/                  dmg.md (failures and fixes), signing.md.
```

`init` renames `starter` and `starter-core` to your app's name.

## Requirements

macOS 12 or newer to build, with the Xcode command line tools, Node 20+ and
Rust. `npm run doctor` checks all of it. The core crate and the frontend also
build and test on Linux, and that is what CI does first.
