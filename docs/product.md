# Camu: what it must do better

Camu keeps every app on a Mac up to date. The alternatives (Latest, Updatest,
Pear Cleaner, Floodtide, MacUpdater, which was retired) each fail people in
known ways. These are the complaints, and Camu's answer to each one.

| Complaint | Camu's answer |
|---|---|
| Misses updates; only covers Sparkle and the App Store ("half my apps") | Check **every** source an app has (Sparkle appcast, Mac App Store, Homebrew cask, Electron update feed, GitHub releases) and report the newest version any of them knows |
| "Often miss App Store updates" | Look up the App Store in the user's own storefront first, then the US one |
| Homebrew's API lags the cask repository | Homebrew is a fallback. When an app's own feed is newer, the feed wins |
| Different tools disagree | Every source result is kept, so the details view can say where a version came from |
| Homebrew updates are a separate, "heavy" flow with per-app approval | One list, one **Update All** button. Homebrew casks update in the same batch |
| Updates that need an admin password are left to the user | The batch asks for the password **once**, only if an app needs it |
| Fear of an update breaking something | The old app is kept as a **backup**, and **Roll back** puts it back |
| Wanting to read changelogs first | Release notes inline, one click away, as plain text |
| Cluttered, verbose UI with information stacked on itself | One row per app: name, installed → available, one action. Details only when opened |
| Needs exclusions and beta channels | Ignore per app; opt into beta channels per app |
| Menu bar or dock, auto-update if wanted | Both modes; scheduled checks; auto-update is off by default |
| Subscriptions, tracking, accounts | None. No analytics, no registration, no ads |

## Order of work

Done: 1, 2 and 3. Next: 4.

1. **Coverage.** Scanning and every checker, all tested against fixtures in the
   core crate.
2. **The list.** Black only, in Linear's design language: a sidebar of views,
   a list grouped by how each update installs, a detail panel, a ⌘K command
   menu, and single-key shortcuts (j/k, Enter, U, I, G then a view).
3. **Updating.** Homebrew batch, App Store hand-off, download → verify the
   signature matches the installed app's team → back up → replace, with a
   single admin prompt for the batch. Roll back.
4. **Living with it.** Menu bar mode, scheduled checks, notifications, Camu
   updating itself.

## Safety rules for installing

- A downloaded app replaces an installed one only if both are signed by the
  same Apple team (or the installed one is unsigned and the download is
  signed and notarized).
- The bundle identifier must match.
- The old app is moved to a backup folder, never deleted, before the new one is
  copied in.
- A running app is asked to quit, never killed.
