# Releasing Hydra

Hydra has three release tracks, each with its own version, tag and changelog:

| Track | What it versions | Tag | Changelog |
|---|---|---|---|
| Library | The workspace version shared by `hydra-common`, the engine crates (`hydra-engine-wds`, `hydra-engine-uds`), the dialect crates (`hydra-interop-swmm`, `hydra-interop-epanet`), `hydra-engines`, `hydra-report` and `hydra-sdk` | `v{version}` | `CHANGELOG.md` |
| CLI | `hydra-cli` | `cli-v{version}` | `crates/cli/CHANGELOG.md` |
| GUI | `hydra-gui` (`Cargo.toml`, `tauri.conf.json`, the frontend `package.json`) | `gui-v{version}` | `crates/gui/CHANGELOG.md` |

Releases are cut by [release-please](https://github.com/googleapis/release-please) (`.github/workflows/release.yml`, configured in `release-please-config.json`). Nobody bumps a version or pushes a tag by hand.

## How a release happens

1. **Merge to main as usual.** release-please keeps one release PR open per track that has unreleased changes. Each shows the next version and the changelog, worked out from the Conventional Commit titles of what touched that track since its last tag: `feat` is a minor, `fix` and `perf` a patch, `!` or a `BREAKING CHANGE` footer a major. A commit is assigned to a track by the files it touches.
2. **The release PR completes itself.** After release-please updates a release PR, the workflow pushes one more commit onto it (`scripts/release_sync.py`, also `just release-sync`): every intra-workspace dependency pin and the documented `hydra-sdk = "N"` pin move to the new version, `Cargo.lock` is refreshed, and the third-party licence notices are regenerated. That push runs CI on the PR.
3. **Merge the release PR when CI is green.** release-please tags the merge commit and opens a **draft** GitHub release with the changelog as its notes. The workflow then builds every asset onto the draft: CLI binaries for a `cli-v*` release, GUI installers for `gui-v*`, the single-file browser demo for `v*`. A GUI release refuses to build if the notices are stale.
4. **Review the notes and publish the draft.** Edit the notes into consumer-facing language first (see *Release notes* below): the GUI shows them in its What's New panel. Publishing is what releases. It triggers:
   - `publish-crates`: the library crates for `v*`, `hydra-cli` for `cli-v*`;
   - `updater-manifest`: installed GUIs see a published `gui-v*` release;
   - `site`: the hosted demo at /try rebuilds from the latest `v*` release.

Draft releases are invisible to the updater and the in-app What's New feed, so nothing reaches users before step 4.

## Releasing the library and the CLI together

`hydra-cli` depends on `hydra-sdk` by version, so a CLI release that follows a library change needs that library version on crates.io. The order is:

1. Merge and publish the library release. Wait for `publish-crates` to go green. Its release PR moved the CLI's `hydra-sdk` pin on main.
2. Merge and publish the CLI release PR. One exists if the CLI itself has a `feat` or `fix` since its last release. The pin move alone does not open one, because the library release merges as a `chore`. To ship the CLI only to pick up library changes, land a commit such as `fix(cli): build on hydra-sdk 17.1.0` touching `crates/cli`, or put a `Release-As: x.y.z` footer in one.

If the CLI is published first anyway, its `publish-crates` job waits up to half an hour for the pinned `hydra-sdk` to appear, then fails and says which release to publish first. Re-run the job afterwards.

CLI and GUI releases are independent of each other.

## Important rules

- **Release PRs are release-please's.** Don't edit one by hand: it rewrites its branch whenever main moves. To change a version, change the commits (or use a `Release-As: x.y.z` footer in a commit body on main). Change the notes on the draft release instead.
- **Keep breaking changes in one track.** A commit marked `!` bumps the major of every track whose files it touches. Split an engine API break and its GUI follow-up into separate commits so the GUI does not take a major it has no public API to justify.
- **The release PR needs `RELEASE_BOT_TOKEN`.** A fine-grained PAT with `contents: write` on this repository, stored as an Actions secret. Without it the completion step fails and the PR gets no CI. Pushes made with the workflow's own token trigger no workflow, which is why a PAT is needed.
- **Licence notices are regenerated per release, not per change.** `main` may carry stale notices between releases. That is expected: every release PR regenerates them, and the GUI build checks them. `just licenses` regenerates them locally if you want them current sooner.
- **Never delete a tag or a published release.** This repository has immutable releases; a deleted release permanently retires its tag name. Supersede a bad release with the next version instead.

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
