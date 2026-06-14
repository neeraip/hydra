# Troubleshooting

## GUI

### macOS — "Hydra is damaged and can't be opened"

macOS Gatekeeper blocks apps that are not code-signed with an Apple Developer certificate. This is a known limitation while Hydra's builds are not yet notarised.

To open Hydra after installing it to `/Applications`, run this once in Terminal:

```sh
xattr -cr /Applications/Hydra.app
```

Then open the app normally from Finder or Spotlight.

### macOS — App opens but immediately quits

This can happen if the app was extracted from a `.dmg` without being moved to `/Applications` first. Move `Hydra.app` to `/Applications`, run the `xattr -cr` command above, then try again.

### Windows — "Windows protected your PC" (SmartScreen)

Click **More info**, then **Run anyway**. SmartScreen warns on unsigned executables. This will be resolved once Hydra's Windows builds are code-signed.

### Linux — AppImage does not open

Make the AppImage executable before running it:

```sh
chmod +x Hydra-*.AppImage
./Hydra-*.AppImage
```

If you see a FUSE-related error, install the required library:

```sh
# Ubuntu / Debian
sudo apt install libfuse2

# Fedora
sudo dnf install fuse-libs
```

---

## CLI

### macOS — "hydra cannot be opened because the developer cannot be verified"

The pre-built binary is not code-signed. Remove the quarantine attribute after downloading:

```sh
xattr -d com.apple.quarantine hydra
```

Then move it to your `PATH` and run normally.

### `hydra: command not found`

The `hydra` binary is not on your `PATH`.

- If you installed with `cargo install hydra-cli`, ensure `~/.cargo/bin` is on your `PATH`:
  ```sh
  export PATH="$HOME/.cargo/bin:$PATH"
  ```
  Add this line to your shell profile (`.bashrc`, `.zshrc`, etc.) to make it permanent.

- If you downloaded a pre-built binary, move it to a directory that is already on your `PATH` (e.g. `/usr/local/bin` on macOS/Linux).

### Exit code 1 — Input error

Hydra could not read or parse the network file. Common causes:

- The file path is wrong or the file does not exist.
- The `.inp` file contains a syntax error. Check the report for the specific line.
- A URL was provided but the server returned 4xx. Verify the URL is accessible.

### Exit code 2 — Solver did not converge

The hydraulic solver could not find a balanced solution for one or more time steps. This usually means the network model itself has an issue:

- Check for isolated nodes or disconnected sub-networks.
- Verify pump curves and valve settings are physically reasonable.
- Try setting `UNBALANCED CONTINUE 10` in the `[OPTIONS]` section to let the simulation proceed past the failing step and produce a partial report for diagnosis.

### Exit code 3 — I/O error

Hydra could not write output. Check that the output directory exists and that you have write permission.

---

## Getting Help

If the steps above do not resolve your issue, open a GitHub issue with:

- The Hydra version (`hydra -v`)
- Your operating system and version
- A minimal `.inp` file that reproduces the problem (if applicable)
- The full error message or report output

→ [Open an issue](https://github.com/neeraip/hydra/issues)
