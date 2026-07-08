# Releasing Hydra

CLI (`hydra-cli`) and GUI (`hydra-gui`) are versioned **independently** from the library stack (`hydra-engine-wds`, `hydra-sdk`). The library stack shares a single workspace version.

| Command | What it bumps | Tag created | Triggers |
|---|---|---|---|
| `just bump [patch\|minor\|major] [--push\|--no-push]` | Workspace version (common + engine + sdk) + dep pins in cli/sdk | `v{version}` | GitHub draft release (crates.io publish triggers when release is published) |
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

# 1. Review and publish the library draft release from the GitHub releases page
# 2. Publishing triggers the publish-crates workflow — wait for it to go green
#    (hydra-sdk must be on crates.io before the CLI publish can succeed)

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

- **Never push a `cli-v*` or `gui-v*` tag at the same time as a `v*` tag.** The CLI publish will race against the library publish and fail because `hydra-sdk` won't be on crates.io yet.
- **Never use these recipes just to set a version without intending a release.** They commit and tag, which triggers CI/CD. To reset or change a version without releasing, edit the relevant `Cargo.toml`, `tauri.conf.json`, and `crates/gui/frontend/package.json` files directly, run `cargo update --workspace`, and commit — no tag.
