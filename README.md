# Camu

Keeps every app on your Mac up to date. Dark, one line per app,
no accounts, no tracking.

Camu checks each app against every source that knows it (its Sparkle
appcast, its Electron update feed, the Mac App Store, GitHub releases and
Homebrew's cask catalogue) and believes the most trusted one that answers.
See [docs/product.md](docs/product.md) for what it must do better than the
alternatives, and the order of work.

Built with Tauri 2, Rust and React, from the Mac App Template.

**Privacy.** Camu talks to the update sources it checks. Besides those, it
contacts camu.cc for two things, and neither carries anything that
identifies you or your Mac: once, the first time it opens, to count the
install (only Camu's version is sent), and when you choose *Version looks
wrong? Report it* (the app's name, its version and what Camu found).

## Download

Get the latest `.dmg` from <https://camu.cc>; <https://camu.cc/download>
always downloads the newest release. (The site fetches it from this
repository's latest release, which from 0.2.1 on also carries `Camu.dmg`.) It is one universal build for Apple
silicon and Intel, and needs macOS 12 or newer.

Once installed, Camu keeps itself up to date: it checks for a new release
once a day and, with automatic installs on (the default), installs it and
relaunches.

Builds are not yet signed with an Apple Developer ID, so the first time you
open Camu macOS says it "cannot be verified". Approve it once in System
Settings > Privacy & Security > Open Anyway.

## Where things live

| Path | What |
|---|---|
| `crates/camu-core/src/scan.rs`, `app.rs` | Finding installed apps and reading their bundles |
| `crates/camu-core/src/sources/` | One module per update source |
| `crates/camu-core/src/check.rs` | Asking the sources and deciding what to believe |
| `crates/camu-core/src/version.rs` | Comparing version strings |
| `src-tauri/src/lib.rs` | Thin IPC commands |
| `src/` | The UI. Every call to Rust goes through `src/lib/native.ts` |

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

Set the version and merge it to `main`:

```sh
npm run set-version -- 1.0.0
git commit -am "Release 1.0.0"
```

Then start the release in either of two ways:

- **Run workflow** (what Camu's releases use): on GitHub, open Actions >
  Release > Run workflow, choose `main` and enter the version (`1.0.0`, the
  same as `package.json`). GitHub creates the `v1.0.0` tag and the release
  itself.
- **Push a tag**, where tag pushes are allowed:
  `git tag v1.0.0 && git push --tags`.

Either way, GitHub Actions builds a universal `.dmg` and attaches it to a GitHub release.
It is signed and notarized if you have added Apple credentials as secrets
([docs/signing.md](docs/signing.md)), and ad-hoc signed otherwise.

Every push also runs CI, which builds a `.dmg` on a real Mac and attaches it
to the run, so a change that breaks packaging is caught on the day it is made.

## Layout

```
crates/camu-core/      The app's logic. No Tauri, so it tests on any OS.
src-tauri/             The native shell: window, menu bar icon, commands, bundling config.
  tauri.conf.json      Name, id, window, .dmg layout, signing.
  entitlements.plist   What the hardened runtime allows.
  app-icon.png         The one icon you edit; npm run icons makes the rest.
src/                   The React interface (main window and menu bar panel).
  lib/native.ts        Every call from the UI to Rust, in one place.
scripts/               doctor, build-dmg, verify-dmg, set-version.
docs/                  product.md (what Camu must do better), dmg.md (failures
                       and fixes), signing.md.
.github/workflows/     ci.yml (every push), release.yml (releases).
```

## Requirements

macOS 12 or newer to build, with the Xcode command line tools, Node 20+ and
Rust. `npm run doctor` checks all of it. The core crate and the frontend also
build and test on Linux, and that is what CI does first.
