# Design note: project library and artboards

Status: implemented in format version 3.

## Problem

A game needs dozens of sprites: characters, tiles, UI. In format v1 a
`.dotify` file holds exactly one canvas, so the art for one game is spread
over many files with many palettes. The goal is one project that holds all
the art for a game, browsed like a library and laid out like artboards.

## Model

A new top-level `Project` (`crates/dotify-core/src/project/`) contains
today's `Document`s instead of replacing them:

```
Project
├── name
├── palette            shared by every artboard
├── folders: [Folder]  flat, ordered ("Characters", "Tiles", "UI")
├── artboards: [Artboard]  ordered (library order)
│   ├── id: ArtboardId
│   ├── folder: Option<FolderId>   None = ungrouped
│   └── document: Document         canvas size, layers, frames, cels, tags
├── next_id            allocator for artboard and folder IDs
└── extensions
```

- **Why contain `Document` instead of rewriting it.** Every drawing,
  layer, frame and tag feature (and everything being added in parallel for
  animation and Godot export) keeps working unchanged on a `Document`. The
  project adds one level of addressing on top.
- **IDs.** `ArtboardId` and `FolderId` come from the project counter and are
  never reused within a project. Layer and frame IDs stay scoped to their
  artboard, so duplicating an artboard keeps the copy's layer/frame IDs and
  existing commands addressed by `(artboard, layer, frame)` remain valid.
- **Name.** An artboard's name *is* its document's name, so
  `rename_document` inside an artboard and `rename_artboard` are the same
  edit.
- **Palette.** The project palette is authoritative. Each artboard's
  `Document::palette` is kept equal to it (an invariant maintained by the
  project commands), so code that only sees a `Document` still sees the
  right swatches. `set_palette` sent to an artboard is routed to the project.
- **Folders** are a flat list. Nesting can be added later through a
  `parent` field without a format break. Deleting a folder moves its
  artboards to the root; it never deletes art.
- **Invariant:** a project always has at least one artboard (like a document
  always has one layer and one frame).

## Commands and history

`ProjectCommand` is the project-level editing vocabulary (serde-tagged like
`Command`):

| Command | Effect |
|---------|--------|
| `create_artboard` | New artboard with a canvas size, optional folder and index |
| `rename_artboard` | Rename |
| `duplicate_artboard` | Deep copy placed after the source, same folder |
| `delete_artboard` | Remove (not the last one) |
| `move_artboard` | Reorder and/or change folder in one step |
| `create_folder` / `rename_folder` / `delete_folder` | Folder management |
| `set_palette` | Shared project palette |
| `rename_project` | Project name |
| `edit` | `{ artboard, command }`: any document `Command` on one artboard |
| `batch` | Atomic list of project commands |

`ProjectEditor` mirrors `Editor` (revision checks, atomic batches with
rollback, bounded history, dirty tracking) over `ProjectOp`s. Document edits
compile with the existing `Command::compile` into `Op`s and are wrapped as
`ProjectOp::Document { artboard, op }`, so their undo stays bit-exact. One
undo history covers the whole project: undoing a library change and a stroke
are steps in the same timeline, and an AI batch spanning several artboards is
still one undo step. Outcomes report which artboards changed so the UI can
refresh only those thumbnails.

`Editor` (single document) is unchanged and still used by tests and
document-level code.

## Format v3

See `FORMAT.md`. `document.json` now describes the project; each artboard
entry carries what a v2 document carried (minus the palette), and cel PNGs
live under `artboards/<id>/cels/`. `min_reader_version` becomes 3 because an
older reader would misread the file. The v2 → v3 migration wraps the
document as a project with one artboard (ID 1, no folder) whose palette
becomes the project palette; tags (with `loop`) and cel file paths are kept
as written. v1 files first get v2's `loop` step.

## Desktop UI

- **Library sidebar** (left): search, folders as collapsible groups,
  artboard rows with live thumbnails, rename in place, context menu
  (duplicate, delete, move to folder) and drag and drop to reorder or move
  between folders.
- **Overview** (`OverviewView`, plain TS like `CanvasView`): every artboard
  drawn side by side, grouped by folder, with pan/zoom. Click selects,
  double-click opens it in the editor. Layout is computed (flow layout per
  folder), not stored; free placement can be added later as an additive
  field.
- **Editor**: unchanged, edits the active artboard. The active artboard is
  part of the backend session so the window title, export and future MCP
  clients agree on it.

## Export

`dotify_export::project` exports every artboard in one go through an
`ArtboardExporter` trait, into `<dir>/<folder>/` with sanitized,
de-duplicated file names. The PNG exporter writes each artboard's first
frame as `<artboard>.png`; the Godot exporter writes its spritesheet,
`SpriteFrames` `.tres` and `.json`. New formats only implement the trait.

## Out of scope for now

Free artboard positions, nested folders, per-artboard palettes, cross-artboard
copy/paste, symbol/instance artboards.
