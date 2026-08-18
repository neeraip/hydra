# Installation

## Try without installing

**[The browser demo](../../try/index.html)** runs the real simulation engines, compiled to WebAssembly. Drop an EPANET or SWMM `.inp` file, or pick a bundled example, and read the same report the CLI prints, produced by the same engine code. Models are read, solved and reported in your own tab; nothing is uploaded anywhere.

Each library release attaches a single-file version, `hydra-try-<version>.html`, pinned to that release's engines: one HTML file that runs the same engines offline, straight from a Downloads folder, with no server and no install.

The demo runs models; it does not edit them, draw them, or read large result files. For those, install the desktop app below.

## GUI: Desktop Application

Download the installer for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest):

| Platform | Installer type |
|---|---|
| macOS (Apple Silicon) | `.dmg` disk image, or portable `.app.tar.gz` |
| Windows | `.msi` or `.exe` installer |
| Linux | `.AppImage`, `.deb`, or `.rpm` package |

After installing, see [Troubleshooting](troubleshooting.md) if macOS blocks the app from opening.

## CLI: Command Line

For most users, **Cargo install is the recommended path**.

**Option 1: Install with Cargo (recommended)**

```sh
cargo install hydra-cli
```

Requires Rust ≥ 1.95 (install via [rustup.rs](https://rustup.rs)).

After installing, verify with:

```sh
hydra -V
```

**Option 2: Pre-built binary** (no Rust required)

Download the `hydra` binary for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest) and place it somewhere on your `PATH`.

> **macOS:** Pre-built CLI binaries are currently not notarised. If Gatekeeper blocks the binary, remove the quarantine flag:
> ```sh
> xattr -d com.apple.quarantine hydra
> ```

## Building from Source

If you want to build Hydra yourself (e.g. to contribute or run the test suite):

**Prerequisites**

- Rust stable ≥ 1.95, from [rustup.rs](https://rustup.rs)
- [just](https://just.systems): `cargo install just` or `brew install just`
- **GUI only:** Node.js 24, [pnpm](https://pnpm.io) 11, [Tauri CLI](https://tauri.app/reference/cli/) (`cargo install tauri-cli`), and the [Tauri system prerequisites](https://tauri.app/start/prerequisites/) for your platform

```sh
git clone https://github.com/neeraip/hydra
cd hydra
just setup          # optional: install Cargo deps, frontend deps, and CLI tools (needs pnpm)
just build          # debug build
just release        # optimised release build (fat LTO, embeds the GUI frontend — needs Node/pnpm)
just test           # run the full test suite
```
