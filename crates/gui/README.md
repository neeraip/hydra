# hydra-gui

Tauri-based desktop GUI for Hydra.

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
