# Keyboard Shortcuts

A complete reference for the Hydra desktop GUI. Press **?** at any time to open the in-app cheatsheet.

Throughout this page, the **primary modifier** is **⌘** (Command) on macOS and **Ctrl** on Windows/Linux; **⇧** is Shift. Single-key shortcuts (like the canvas tools) are ignored while you are typing in a text field.

---

## Global

| Action | macOS | Windows/Linux | Notes |
|---|---|---|---|
| Toggle command palette | ⌘K | Ctrl+K | Works everywhere, even while typing |
| Show keyboard-shortcut cheatsheet | ? | ? | Suppressed while typing |
| Dismiss (cheatsheet → issues panel → command palette) | Esc | Esc | Also closes any open modal |

## Project navigation

These require a project to be open.

| Action | macOS | Windows/Linux |
|---|---|---|
| Go to Overview | ⌘1 | Ctrl+1 |
| Go to Canvas | ⌘2 | Ctrl+2 |
| Go to Editor | ⌘3 | Ctrl+3 |
| Go to Results | ⌘4 | Ctrl+4 |
| Go to Report | ⌘5 | Ctrl+5 |
| Toggle the Issues panel | ⌘⇧M | Ctrl+Shift+M |
| Undo network edit | ⌘Z | Ctrl+Z |
| Redo network edit | ⌘⇧Z | Ctrl+Shift+Z |

On the **Projects** screen, ⌘F / Ctrl+F focuses the projects search box.

## Simulation

| Action | macOS | Windows/Linux | Notes |
|---|---|---|---|
| Open the Run dialog | ⌘R | Ctrl+R | Requires a project open |
| Confirm and run | ⌘Enter | Ctrl+Enter | From within the Run dialog |

⌘R opens a confirmation dialog where you select scenarios; the run itself starts on ⌘Enter (or the confirm button).

## Canvas and map

These work on the Canvas view. The zoom/layout shortcuts also switch you to the Canvas view first.

| Action | macOS | Windows/Linux | Notes |
|---|---|---|---|
| Zoom in | ⌘= (or ⌘+) | Ctrl+= (or Ctrl++) | |
| Zoom out | ⌘- (or ⌘_) | Ctrl+- (or Ctrl+_) | |
| Fit to network extent | ⌘0 | Ctrl+0 | |
| Toggle geographic ⇄ orthogonal layout | ⌘M | Ctrl+M | |

### Canvas tools (single keys)

Active on the Canvas view; the edit/add/measure tools apply in geographic (map) mode.

| Action | Key |
|---|---|
| Select tool | S |
| Edit (move) tool | E |
| Add-node tool | N |
| Add-link tool | L |
| Measure tool | D |
| Return to Select tool | Esc |
| Delete the selected node/link | Delete or Backspace |

### Canvas mouse

| Action | Gesture |
|---|---|
| Select an element | Click |
| Zoom in / out | Scroll |

### Timeline playback (single keys)

| Action | Key |
|---|---|
| Play / pause | Space |
| Step backward / forward | ← / → |
| Jump to start / end | Home / End |

The same keys work when the timeline scrubber has focus.

## Command palette

While the palette is open (⌘K / Ctrl+K):

| Action | Key |
|---|---|
| Move selection up / down | ↑ / ↓ |
| Run the selected command | Enter |
| Find a node or link by ID | type `#` then the ID |
| Close the palette | Esc |

## Editors and dialogs

| Action | Key | Where |
|---|---|---|
| Commit an edit | Enter | Network editor table cells, renames, pattern/curve editors |
| Cancel an edit | Esc | Same |
| Save settings | ⌘Enter / Ctrl+Enter | Simulation-settings dialog |
| Close a dialog | Esc | Any modal dialog |
