# Changelog

## Unreleased

- Timeline playback with loop, once and ping-pong modes, honoring per-frame durations
- Onion skinning with configurable previous/next frame counts, opacity and tint
- Animation tags: create, rename, recolor, retime, set direction and loop, delete — all undoable
- Live preview panel that keeps playing while you draw
- Export dialog for PNG, spritesheet (grid or row per tag, padding, extrude) + JSON metadata, animated GIF and Godot 4
- Godot 4 export: spritesheet PNG plus a `SpriteFrames` `.tres` with one animation per tag
- CLI: `export-sheet`, `export-gif` and `export-godot`
- Project format version 2: tags gain a `loop` flag (older files open with every tag looping)

## 0.1.1

- Fix the canvas background and checkerboard keeping the old colors after switching between light and dark themes
- The window title bar and system dialogs now follow the app theme

## 0.1.0

First release: a working pixel editor.

- Pencil, eraser, bucket fill, line, rectangle, ellipse, eyedropper and hand tools, with horizontal/vertical symmetry
- Pixel-exact zoom and pan, pixel grid, dark and light themes
- HSV + alpha color picker and editable palette
- Layers (visibility, lock, opacity, reorder) and a frame strip with per-frame durations
- Undo/redo for every edit
- Native `.dotify` project format with autosave and crash recovery
- PNG export at 1×–16×
- `dotify` command-line tool
- Automatic updates
