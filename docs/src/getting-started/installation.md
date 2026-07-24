# Installation

## GUI — Desktop Application

Download the installer for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest):

| Platform | Installer type |
|---|---|
| macOS (Apple Silicon / Intel) | `.dmg` disk image |
| Windows | `.msi` installer |
| Linux | `.AppImage` or `.deb` package |

After installing, see [Troubleshooting](troubleshooting.md) if macOS blocks the app from opening.

## CLI — Command Line

For most users, **Cargo install is the recommended path**.

**Option 1 — Pre-built binary** (no Rust required)

Download the `hydra` binary for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest) and place it somewhere on your `PATH`.

> **macOS** — Pre-built CLI binaries are currently not notarised. If Gatekeeper blocks the binary, remove the quarantine flag:
> ```sh
> xattr -d com.apple.quarantine hydra
> ```

**Option 2 — Install with Cargo (recommended)**

```sh
cargo install hydra-cli
```

Requires Rust ≥ 1.95 (install via [rustup.rs](https://rustup.rs)).

After installing, verify with:

```sh
hydra -v
```

## Building from Source

If you want to build Hydra yourself (e.g. to contribute or run the test suite):

**Prerequisites**

- Rust stable ≥ 1.95 — [rustup.rs](https://rustup.rs)
- [just](https://just.systems) — `cargo install just` or `brew install just`
- **GUI only:** Node.js 24, [pnpm](https://pnpm.io) 11, [Tauri CLI](https://tauri.app/reference/cli/) (`cargo install tauri-cli`), and the [Tauri system prerequisites](https://tauri.app/start/prerequisites/) for your platform

```sh
git clone https://github.com/neeraip/hydra
cd hydra
just setup          # optional: install Cargo deps, frontend deps, and CLI tools (needs pnpm)
just build          # debug build
just release        # optimised release build (fat LTO, embeds the GUI frontend — needs Node/pnpm)
just test           # run the full test suite
```
