# Releasing Hydra

CLI (`hydra-cli`) and GUI (`hydra-gui`) are versioned **independently** from the library stack (`hydra-common`, `hydra-engine`, `hydra-sdk`). The library stack shares a single workspace version.

| Command | What it bumps | Tag created | Triggers |
|---|---|---|---|
| `just bump [patch\|minor\|major\|x.y.z]` | Workspace version (common + engine + sdk) + dep pins in cli/sdk | `v{version}` | GitHub draft release + crates.io publish of common/engine/sdk |
| `just bump-cli [patch\|minor\|major\|x.y.z]` | `crates/cli/Cargo.toml` only | `cli-v{version}` | CLI binary release + crates.io publish of hydra-cli |
| `just bump-gui [patch\|minor\|major\|x.y.z]` | `crates/gui/Cargo.toml` + `tauri.conf.json` | `gui-v{version}` | GUI installer release |

## Release patterns

### Pattern 1 — Library + CLI/GUI (library changed)

`hydra-cli` depends on `hydra-sdk`, which must be indexed on crates.io before the CLI publish can succeed. Push the library tag first and wait for the `publish-crates` workflow to complete before pushing CLI/GUI tags.

```sh
just bump minor
git push && git push --tags   # creates library draft release + starts crates.io publish

# 1. Wait for the publish-crates workflow to go green
# 2. Publish the library draft release from the GitHub releases page

just bump-cli minor
just bump-gui minor
git push && git push --tags   # cli and gui are safe to push together
```

### Pattern 2 — CLI and/or GUI only (no library change)

CLI and GUI are independent of each other and can be pushed together.

```sh
just bump-cli patch   # and/or just bump-gui patch
git push && git push --tags
```

## Important rules

- **Never push a `cli-v*` or `gui-v*` tag at the same time as a `v*` tag.** The CLI publish will race against the library publish and fail because `hydra-sdk` won't be on crates.io yet.
- **Never use these recipes just to set a version without intending a release.** They commit and tag, which triggers CI/CD. To reset or change a version without releasing, edit the relevant `Cargo.toml` and `tauri.conf.json` files directly, run `cargo update --workspace`, and commit — no tag.
