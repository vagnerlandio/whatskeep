# Releasing

Releases are built by GitHub Actions and published as **draft** GitHub releases; nothing goes public until you click **Publish**.

## Workflows

| Workflow | When | What |
|---|---|---|
| `.github/workflows/ci.yml` | every push to `main`, pull requests, and before every release | clippy (`-D warnings`), Rust tests, type checks, security lint, export end-to-end test in Chromium and WebKit — on macOS |
| `.github/workflows/release.yml` | pushing a tag `v*` | runs CI, checks that the tag matches the app version, builds the macOS installers (Apple Silicon and Intel) and uploads them to a draft release with notes |

All third-party actions are pinned to commit SHAs (a tag can be moved to other code). To update one, resolve the new tag to its commit (`git ls-remote https://github.com/<owner>/<repo> 'refs/tags/<tag>^{}'`) and keep the version comment next to it.

## Making a release

```sh
scripts/release.sh 1.2.3          # sets the version, commits "Release v1.2.3", tags v1.2.3
git push origin main v1.2.3       # starts the release workflow
```

- `scripts/release.sh` requires a clean working tree; it updates `tauri.conf.json`, the app's `Cargo.toml` (and `Cargo.lock`) and the `package.json` files. Pre-releases like `1.3.0-beta.1` are marked as such on GitHub.
- When the workflow finishes, open **Releases** on GitHub, review the draft (assets: `WhatsKeep_<version>_aarch64.dmg`, `WhatsKeep_<version>_x64.dmg` and `.app.tar.gz` files; notes generated from the merged changes) and click **Publish release**.
- If the tag does not match the version, the workflow stops before building. Delete the tag (`git tag -d v1.2.3 && git push origin :refs/tags/v1.2.3`), fix and try again.

## Code signing and notarization (optional)

Without an Apple Developer account, builds are **ad-hoc signed**: users confirm the app once in *System Settings → Privacy & Security → Open Anyway* (the README explains it). With an account ($99/year), add these repository secrets (*Settings → Secrets and variables → Actions*) and the next release is signed with your Developer ID and notarized, so it opens without warnings:

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | the *Developer ID Application* certificate exported as `.p12`, base64-encoded (`base64 -i cert.p12 \| pbcopy`) |
| `APPLE_CERTIFICATE_PASSWORD` | the password chosen when exporting the `.p12` |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | the Apple ID e-mail (for notarization) |
| `APPLE_PASSWORD` | an app-specific password for that Apple ID |
| `APPLE_TEAM_ID` | the 10-character team id |

Signing needs the first three; notarization also needs the last three.

## Before the first release

1. Create the GitHub repository and push: `git remote add origin git@github.com:<you>/whatskeep.git && git push -u origin main`.
2. In *Settings → Actions → General*, keep "Read repository contents" as the default token permission (the release job asks for write access itself).
3. Wait for the CI workflow on `main` to pass, then follow *Making a release*.

Windows and Linux are not built yet: they compile in principle (Tauri), but have not been tested. Add them to the release matrix once they are.
