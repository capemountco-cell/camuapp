# Signing and notarization

There are three levels. The template works at the first with no setup.

| Level | What a downloader sees | What you need |
| --- | --- | --- |
| **Ad-hoc** (default) | A one-time "cannot be verified" prompt; they approve it in System Settings > Privacy & Security > Open Anyway | Nothing |
| **Developer ID signed** | The same prompt, naming you as the developer | An Apple Developer Program membership ($99/year) and a Developer ID Application certificate |
| **Signed and notarized** | Nothing: it opens like any app | The above, plus notarization credentials |

Ad-hoc signing matters even though it proves nothing about you: an app with
no signature at all is reported as **damaged** once downloaded, which reads as
corruption. `bundle.macOS.signingIdentity` is `"-"` for this reason. Do not
remove it.

## Signing and notarizing a release in CI

`release.yml` signs and notarizes automatically when these repository secrets
exist (Settings > Secrets and variables > Actions):

| Secret | What it is |
| --- | --- |
| `APPLE_CERTIFICATE` | Your Developer ID Application certificate exported as `.p12`, base64-encoded: `base64 -i cert.p12 \| pbcopy` |
| `APPLE_CERTIFICATE_PASSWORD` | The password you set when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | The certificate's name, e.g. `Developer ID Application: Your Name (TEAMID)`. Find it with `security find-identity -v -p codesigning` |
| `APPLE_ID` | The Apple ID email of your developer account |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID, made at account.apple.com |
| `APPLE_TEAM_ID` | Your 10-character team ID, shown at developer.apple.com/account |

With none of them set, the release is ad-hoc signed and still works.

## Signing locally

Export the same variables in your shell and run `npm run dmg`. Tauri's
bundler reads them; the identity overrides `"-"` in `tauri.conf.json`.

```sh
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com" APPLE_PASSWORD="xxxx-xxxx-xxxx-xxxx" APPLE_TEAM_ID="TEAMID"
npm run dmg
```

Check the result with `spctl -a -vv -t install "release/My App-1.0.0-universal.dmg"`.
