// BasemapStyle values correspond to the three OpenFreeMap styles, three
// offline styles rendered from locally downloaded Protomaps tiles, plus a
// tile-free "none" option. "streets" = Liberty, "light" = Positron,
// "dark" = Dark; the "offline-*" entries are their local counterparts.
export type BasemapStyle =
  | "streets"
  | "light"
  | "dark"
  | "offline-streets"
  | "offline-light"
  | "offline-dark"
  | "none";
