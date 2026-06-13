# Installation

## Prerequisites

- **Rust** (stable, ≥ 1.95), install via [rustup](https://rustup.rs/)
- **[just](https://just.systems)**: cross-platform task runner (`cargo install just` or `brew install just`)
- **For the GUI only:** Node.js 22, [pnpm](https://pnpm.io) 10, [Tauri CLI](https://tauri.app/reference/cli/) (`cargo install tauri-cli`), and the [Tauri system prerequisites](https://tauri.app/start/prerequisites/) for your platform

## Build

```sh
just build          # debug
just release        # release (fat LTO)
```

## Test

```sh
just test
```
