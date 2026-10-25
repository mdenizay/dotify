# Changelog

## 0.1.3

- Copy a frame onto the next frame, or carry it over as a gray silhouette layer to trace the next pose over (Frames toolbar)
- Fix the Frames panel not showing its frames again after being collapsed
- Timeline playback with loop, once and ping-pong modes, honoring per-frame durations
- Onion skinning with configurable previous/next frame counts, opacity and tint
- Animation tags: create, rename, recolor, retime, set direction and loop, delete — all undoable
- Live preview panel that keeps playing while you draw
- Export dialog for PNG, spritesheet (grid or row per tag, padding, extrude) + JSON metadata, animated GIF and Godot 4
- Godot 4 export: spritesheet PNG plus a `SpriteFrames` `.tres` with one animation per tag
- CLI: `export-sheet`, `export-gif` and `export-godot`
- Project format version 2: tags gain a `loop` flag (older files open with every tag looping)
- Project library: one `.dotify` project now holds many artboards (sprites), each with its own size, layers, frames and tags, sharing one palette
- Library sidebar with thumbnails, folders, search, drag and drop and a context menu (rename, duplicate, move, delete)
- Artboard overview: all artboards side by side with pan and zoom; double-click one to edit it
- Export all artboards in one go, as PNG or Godot 4 SpriteFrames, with one sub-folder per library folder (⌥⌘E, or `dotify export-all [--godot]`)
- Project format version 3: a project of artboards; version 1 and 2 files open as a project with one artboard

## 0.1.2

- Rulers along the top and left edges with cell numbers, the canvas size highlighted and the column/row under the cursor marked (View → Show Rulers, ⇧⌘R)
- The status bar shows the cell under the cursor as X/Y, counting from 1
- Symmetry shows its mirror axes and a see-through preview of the brush at the mirrored cell
- New mirror guide mode: see where the mirror would land without painting there

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
