# Hydra

[![Cargo CI](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml/badge.svg)](https://github.com/neeraip/hydra/actions/workflows/cargo-ci.yml)
[![GitHub release](https://img.shields.io/github/v/release/neeraip/hydra)](https://github.com/neeraip/hydra/releases/latest)
[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

Hydra is a water distribution network simulator written in Rust: extended-period hydraulics and water quality for pressurised pipe networks, built without the historical constraints that shaped EPANET.

**[→ Full documentation](https://neeraip.github.io/hydra/)**

## Install

```sh
cargo install hydra-cli
```

Or download a pre-built binary from the [releases page](https://github.com/neeraip/hydra/releases/latest).

## GUI

Download the installer for your platform from the [releases page](https://github.com/neeraip/hydra/releases/latest).

> **macOS — "Hydra is damaged and can't be opened"**
>
> The app is not yet code-signed. macOS Gatekeeper blocks unsigned apps downloaded from the internet. To open it, run the following in Terminal after installing:
> ```sh
> xattr -cr /Applications/Hydra.app
> ```
> Then try opening the app again.

## Usage

```sh
# Run a simulation — report goes to stdout
hydra network.inp

# Write report and binary output to files
hydra network.inp report.rpt output.out

# Named flags (equivalent)
hydra --input network.inp --report report.rpt --output output.out

# Accept an HTTP URL as input
hydra https://example.com/network.inp

# JSON report
hydra network.inp --report report.json

# Suppress progress output
hydra -q network.inp

# Print version
hydra -v
```

## Documentation

| | |
|---|---|
| [Getting Started](https://neeraip.github.io/hydra/getting-started/installation.html) | Installation, build, CLI, GUI |
| [SDK](https://neeraip.github.io/hydra/sdk/overview.html) | Library usage and examples |
| [Architecture](https://neeraip.github.io/hydra/architecture/crates.html) | Crate layout and specifications |
| [Reference](https://neeraip.github.io/hydra/reference/inp-format.html) | INP format, performance, EPANET migration |

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](.github/CONTRIBUTING.md) before opening a pull request, in particular the **Spec First** workflow, which requires spec changes to land before implementation changes for any solver, model, or analytics work.

## License

[AGPL v3](LICENSE): free to use and modify. Commercial products built on Hydra must either release their source under AGPL v3 or obtain a separate commercial license. See [COMMERCIAL_LICENSE.md](.github/COMMERCIAL_LICENSE.md) for details.
