# hydra-gui

Tauri-based desktop GUI for Hydra, the water infrastructure simulation platform.

The GUI is engine-aware: it resolves a project's engine through `hydra-common`'s
registry rather than assuming water distribution.

<!-- PLANNED-ENGINE: och — drop the planned-engine clause when the open channel engine ships. -->
Engines the GUI cannot yet edit are presented but not selectable: the urban
drainage engine ships CLI-first (its card reads "CLI only" until editor
support lands), and the planned open channel engine appears in the registry
with no implementation behind it.

## Developer diagnostics

Performance tracing is automatic in development builds and disabled in production builds.

- Enabled: `pnpm dev`, `cargo tauri dev`
- Disabled: packaged release builds
- User toggle: none (intentional)

Trace events are emitted to the developer console and include key spans such as network load retries, network-list derive time, and first canvas frame timing.

## Regenerating icons

The source logo is `icons/logo.png` (1024×1024, rounded corners, transparent background).

To regenerate all platform icon sizes from the source:

```bash
cargo tauri icon crates/gui/icons/logo.png --output crates/gui/icons
```

> Requires `tauri-cli`: `cargo install tauri-cli`

### Adjusting the source logo

If you need to resize or repad the source before regenerating:

```bash
magick icons/logo.png -resize 920x920 -gravity center -background none -extent 1024x1024 icons/logo.png
cargo tauri icon icons/logo.png --output icons
```

> Requires ImageMagick: `brew install imagemagick`
