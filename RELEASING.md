# Releasing Hydra

CLI (`hydra-cli`) and GUI (`hydra-gui`) are versioned **independently** from the library stack (`hydra-common`, the engine crates `hydra-engine-wds`/`hydra-engine-uds`, the dialect crates `hydra-interop-swmm`/`hydra-interop-epanet`, `hydra-engines`, `hydra-report`, `hydra-sdk`). The library stack shares a single workspace version.

| Command | What it bumps | Tag created | Triggers |
|---|---|---|---|
| `just bump [patch\|minor\|major] [--push\|--no-push]` | Workspace version (common + engines + report + sdk) + dep pins in cli/sdk | `v{version}` | GitHub draft release (crates.io publish triggers when release is published) |
| `just bump-cli [patch\|minor\|major] [--push\|--no-push]` | `crates/cli/Cargo.toml` only | `cli-v{version}` | CLI binary release + crates.io publish of hydra-cli |
| `just bump-gui [patch\|minor\|major] [--push\|--no-push]` | `crates/gui/Cargo.toml` + `tauri.conf.json` + `crates/gui/frontend/package.json` | `gui-v{version}` | GUI installer release |

By default, each bump command asks: `Push commit and tags now? [y/N]`.

- Pass `--push` to skip the prompt and push immediately.
- Pass `--no-push` to skip the prompt and avoid pushing.

## Release patterns

### Pattern 1 — Library + CLI/GUI (library changed)

`hydra-cli` depends on `hydra-sdk`, which must be indexed on crates.io before the CLI publish can succeed. Push the library tag first and wait for the `publish-crates` workflow to complete before pushing CLI/GUI tags.

```sh
just bump minor
# respond y to the push prompt (or run: just bump minor --push)

# 1. The draft-release workflow attaches hydra-try-<version>.html — the
#    browser demo as one portable file, the engines at exactly this tag.
#    Wait for its job before publishing so the asset is on the draft.
# 2. Review and publish the library draft release from the GitHub releases page
# 3. Publishing triggers the publish-crates workflow — wait for it to go green
#    (hydra-sdk must be on crates.io before the CLI publish can succeed)
# 4. Publishing the library release also redeploys the Pages site: /try
#    is built from the latest v* tag (page, theme, and wasm together), so
#    the hosted demo updates to this release on its own. No manual
#    dispatch needed.

just bump-cli minor
just bump-gui minor
# respond y to each push prompt (or run each with --push)
```

### Pattern 2 — CLI and/or GUI only (no library change)

CLI and GUI are independent of each other and can be pushed together.

```sh
just bump-cli patch   # and/or just bump-gui patch
# respond y to the push prompt (or pass --push)
```

## Important rules

- **The bump refuses a stale checkout.** It fetches `origin/main` first and stops if this branch is behind or has diverged. A release tag here is immutable, so one cut from a checkout missing someone else's commits cannot be moved afterwards — only superseded by another version.
- **Never push a `cli-v*` or `gui-v*` tag at the same time as a `v*` tag.** The CLI publish will race against the library publish and fail because `hydra-sdk` won't be on crates.io yet.
- **Regenerate the third-party notices whenever dependencies moved** — `just licenses`, committed. The app shows them under Settings → About, and the licences of the packages Hydra ships ask that their copyright notices travel with the binary. The `Licence Notices` workflow fails a pull request whose dependencies no longer match the committed file, so this lands with the dependency change rather than at release time. It did not always: the check lived only in `just ci`, which nobody runs on an auto-merged dependency bump, and the file drifted for a release at a time before anyone noticed.
- **Never use these recipes just to set a version without intending a release.** They commit and tag, which triggers CI/CD. To reset or change a version without releasing, edit the relevant `Cargo.toml`, `tauri.conf.json`, and `crates/gui/frontend/package.json` files directly, run `cargo update --workspace`, and commit — no tag.

## GUI self-updater

Installed GUI apps check for updates via `tauri-plugin-updater`. The moving parts:

- **Signing.** The GUI release build signs its updater artifacts (macOS `.app.tar.gz`, Windows `.exe`, Linux `.AppImage` + detached `.sig` files) with a Tauri-specific minisign keypair — **separate from Apple code signing**. CI needs the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` secrets; the matching public key is committed in `crates/gui/tauri.conf.json` (`plugins.updater.pubkey`).
- **Losing the private key strands every installed copy** — apps verify updates against the embedded public key and can never accept an update signed with a replacement key. Keep an offline backup of the key and its password (password manager); GitHub secrets are write-only and are not a backup.
- **Manifest.** Publishing a `gui-v*` release (draft → published) triggers the `updater-manifest` workflow, which composes `latest.json` from the release's signed assets and commits it to the dedicated `updater-manifest` branch. Installed apps poll `https://raw.githubusercontent.com/neeraip/hydra/updater-manifest/latest.json`. **Do not delete or rewrite the `updater-manifest` branch** — if the manifest is ever wrong or missing, re-run the workflow via its manual trigger (Actions → Updater Manifest → Run workflow, with the `gui-v*` tag). The workflow never moves the manifest to an older version.
- **Why a branch, not a release asset:** this repo has **immutable releases** enabled — published assets are frozen, and a deleted immutable release permanently tombstones its tag name, so no fixed release tag can host a replaceable manifest. The `updater` tag was burned learning this; shipped gui-v2.2.0 binaries point at that dead endpoint and cannot self-update (its users download the next release manually, once).
- **Draft releases are invisible to the updater** — nothing reaches users until you publish the release, same as the in-app What's-new feed.
- **Linux:** only AppImage installs can self-update; the app hides updater UI on deb/rpm installs (they update via the package manager).
- **Local bundles need the key too.** With `createUpdaterArtifacts` enabled, `just bundle` fails at the signing step unless `TAURI_SIGNING_PRIVATE_KEY` (and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`) are set in the environment, e.g. `TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/hydra-updater.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=… just bundle`.

## Release notes

GitHub release notes should keep the categorized sections from `.github/release.yml` (`What's New`, `Bug Fixes`, `Performance`, `Internal`) — this distinction genuinely helps readers tell new capabilities apart from fixes and security-relevant patches, and is worth preserving even when writing notes by hand instead of using `gh release create --generate-notes`.

The GUI's in-app "What's New" panel (`crates/gui/frontend/src/hooks/useReleaseNotes.ts`) renders the release body as markdown, after stripping plumbing: HTML comments, "Full Changelog" lines, and the trailing "Installation Note" boilerplate section. Categorized section headers survive and render in the in-app modal.

### Writing style

Release notes are consumer-facing — most readers are end users, not engineers. Each bullet should be understandable to someone who has never looked at the code:

- Describe the *user-visible effect* ("Pump curves can now be edited directly"), not the implementation ("refactored CurveEditor to stage points via DraftContext").
- Avoid internal jargon — component/file/function names, crate names, data structures, PR/issue numbers — unless there is genuinely no other way to describe the change (e.g. a specific `.inp` keyword or unit-system term users already know from EPANET).
- Prefer plain verbs over technical ones: "fixed", "added", "now supports" instead of "refactored", "migrated", "unified".
- Keep each bullet to one sentence. If a change needs a technical caveat for advanced users, put it in the PR description or commit body instead — not the release notes.
