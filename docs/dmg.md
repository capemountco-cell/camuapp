# Building the .dmg, and what goes wrong

`npm run dmg` produces `release/<Product Name>-<version>-<arch>.dmg` and then
checks that the image actually works. This page lists the failures it guards
against, so that when something new goes wrong you know where to look.

## What `npm run dmg` does, in order

1. **Refuses to run off a Mac**, and says so, instead of failing deep in a
   build. A `.app` and a `.dmg` can only be made by macOS tools.
2. **Checks for the Xcode command line tools.** Without them `codesign` and
   the linker are missing. Fix: `xcode-select --install`.
3. **Ejects any mounted volume named like the app.** An image left mounted by
   an earlier build, or by opening an old `.dmg`, makes the next build fail
   with *resource busy*, or produces a volume called `My App 1` whose window
   layout silently no longer applies.
4. **Deletes images from earlier builds**, including the writable `rw.*.dmg`
   a failed build leaves behind, which otherwise breaks the next run.
5. **Runs the type check and the tests first**, so a mistake costs seconds
   rather than a full release build.
6. **Builds the `.app` and `.dmg`.** Over SSH, or with `--plain`, it skips the
   Finder step that styles the window, because that step needs a logged-in
   desktop and hangs without one. GitHub Actions sets `CI=true`, which skips
   it automatically.
7. **Finds the image instead of assuming its path.** In a Cargo workspace it
   lands in the workspace `target/`, not `src-tauri/target/`, and a
   `--target` build adds another directory level.
8. **Verifies it** (`scripts/verify-dmg.sh`): the checksum holds, it mounts
   read-only, it contains the `.app` and an `Applications` link, the
   executable is there, and the signature is valid.

## Failures and fixes

| You see | Why | Fix |
| --- | --- | --- |
| `failed to bundle project: error running bundle_dmg.sh` | Usually the Finder styling step, with no desktop session or a mounted volume of the same name | `npm run dmg -- --plain`, and eject the volume |
| `hdiutil: create failed - Resource busy` | A volume with the app's name is still mounted | `npm run dmg` ejects it; by hand: `hdiutil detach "/Volumes/My App" -force` |
| The app is **"damaged and can't be opened"** after downloading | The app has no valid signature. macOS words a missing signature as damage | The template ad-hoc signs (`signingIdentity: "-"`). Check with `npm run verify-dmg`. For no warning at all, see [signing.md](signing.md) |
| **"cannot be opened because the developer cannot be verified"** | Ad-hoc signed, not notarized | Expected without a Developer ID. System Settings > Privacy & Security > Open Anyway, once |
| Icons missing, `icon.icns` not found | Icons were never generated | `npm run icons` (from `src-tauri/app-icon.png`, 1024×1024) |
| A universal build fails with `can't find crate for std` | One Rust target is missing | `npm run dmg:universal` installs both |
| CI job cancelled one second after starting, no logs | The workflow names a macOS runner image GitHub has retired | Use `macos-latest`, which this template already does |
| The window layout in the `.dmg` is wrong | The Finder step was skipped (`--plain`, SSH or CI) | Build on a Mac you are logged into to get the styled window. The image works either way |

## Things that must stay in sync

- `devUrl` port in `src-tauri/tauri.conf.json` and `server.port` in
  `vite.config.ts` (1420).
- `frontendDist` (`../dist`) and Vite's `build.outDir` (`dist`).
- The version: only ever set it with `npm run set-version -- 1.2.0`.
  `tauri.conf.json` reads it from `package.json`.
