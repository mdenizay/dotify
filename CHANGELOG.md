# Changelog

## Unreleased

- Project library: one `.dotify` project now holds many artboards (sprites), each with its own size, layers, frames and tags, sharing one palette
- Library sidebar with thumbnails, folders, search, drag and drop and a context menu (rename, duplicate, move, delete)
- Artboard overview: all artboards side by side with pan and zoom; double-click one to edit it
- Export all artboards as PNG in one go, with one sub-folder per library folder (also `dotify export-all`)
- `.dotify` format version 2; version 1 files open as a project with one artboard

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
