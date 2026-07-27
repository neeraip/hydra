# Bundled Protomaps basemap assets

Glyphs and sprites for the offline basemap styles (`src/canvas/offlineStyles.ts`),
copied from <https://github.com/protomaps/basemaps-assets> (same files that
`https://protomaps.github.io/basemaps-assets/` serves). Bundled here so the
offline styles need no network access.

## Fonts (`fonts/`)

Font stacks referenced by the generated Protomaps layers:

- `Noto Sans Regular`
- `Noto Sans Medium`
- `Noto Sans Italic`

Only the Latin-relevant glyph ranges are committed for each stack: `0-255.pbf`
and `256-511.pbf`. Labels using characters outside U+0000–U+01FF (Greek,
Cyrillic, CJK, Arabic, …) will NOT render on the offline basemaps until the
corresponding ranges are added from the assets repo.

## Sprites (`sprites/`)

`light` and `dark` sprite sheets (v4), each as `.json`/`.png` plus `@2x`
variants. The assets repo ships only these two sheets; the "white" flavor
(Offline Light) reuses the `light` sheet.
