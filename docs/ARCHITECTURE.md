# Dotify Architecture

Dotify is a pixel art editor and sprite animation studio built as a Tauri 2
desktop application with a Rust document engine and a React/TypeScript UI.
This document describes how the pieces fit together and the rules each layer
must follow. It is the reference for contributors; keep it current when the
architecture changes.

## Goals that shape the design

1. **One command engine.** Manual editing, the CLI, the MCP server and the
   built-in AI assistant all mutate documents through the same
   `dotify_core::Command` type and the same `Editor::execute` entry point.
   There is no second code path that edits pixels.
2. **Deterministic, undoable edits.** Every command is a pure function of the
   document and its arguments. Commands compile into reversible low-level
   operations, so undo/redo is exact and an AI batch can be undone as one step.
3. **Lossless, pixel-exact data.** RGBA8 straight alpha everywhere, integer
   compositing, nearest-neighbour scaling only, no smoothing anywhere.
4. **The core knows nothing about UI or transport.** `dotify-core` has no
   Tauri, no async runtime, no JSON-RPC. It is a plain library.
5. **High-frequency input stays out of React.** Pointer events, the view
   transform and canvas drawing live in plain TypeScript classes; React only
   renders panels and reacts to coarse state changes.

## Repository layout

```
dotify/
├── Cargo.toml                  Rust workspace
├── crates/
│   ├── dotify-core/            Document and project model, commands, history,
│   │                           compositing, PNG codec, .dotify format
│   ├── dotify-export/          PNG export, batch export of all artboards
│   └── dotify-cli/             Headless project manipulation and export
├── apps/desktop/               Tauri + React application
│   ├── src/                    React/TypeScript UI
│   │   ├── engine/             Typed bridge to the Rust engine (Tauri IPC)
│   │   ├── canvas/             Canvas view, rasterisers, tools (no React)
│   │   ├── state/              Zustand stores
│   │   └── components/         Panels and UI primitives
│   └── src-tauri/              `dotify-desktop` crate: IPC commands, menus,
│                               dialogs, autosave/recovery
├── docs/                       Architecture and format specs
└── .github/workflows/          CI
```

Planned crates (not yet created, see roadmap): `dotify-mcp` (Phase 3).

## Layering

```
 ┌───────────────────────────────┐   ┌──────────────┐  ┌──────────────┐
 │ React panels (Zustand stores) │   │  dotify-cli  │  │  dotify-mcp  │
 ├───────────────────────────────┤   │              │  │  (Phase 3)   │
 │ CanvasView / tools (plain TS) │   │              │  │              │
 ├───────────────────────────────┤   │              │  │              │
 │ engine bridge (invoke)        │   │              │  │              │
 ╞═══════════════ Tauri IPC ═════╡   │              │  │              │
 │ dotify-desktop (commands)     │   │              │  │              │
 └───────────────┬───────────────┘   └──────┬───────┘  └──────┬───────┘
                 │                          │                 │
         ┌───────▼──────────────────────────▼─────────────────▼──────┐
         │ dotify-core: Document · Command · Editor/History · format │
         └───────────────────────────┬───────────────────────────────┘
                                     │
                             ┌───────▼───────┐
                             │ dotify-export │
                             └───────────────┘
```

The Rust side owns the authoritative document. The UI holds a *render cache*
of cel pixels that it refreshes from the engine after every change.

## dotify-core

### Document model (`model.rs`)

| Type          | Meaning                                                        |
|---------------|----------------------------------------------------------------|
| `Document`    | Canvas size, ordered layers, ordered frames, cels, palette, tags, export settings |
| `Layer`       | `LayerId`, name, visible, locked, opacity (0–255), blend mode   |
| `Frame`       | `FrameId`, duration in ms                                       |
| `Cel`         | Pixel content of one layer on one frame (`Image`)              |
| `Image`       | `width × height` RGBA8, straight alpha, row-major              |
| `Palette`     | Named ordered list of `Rgba` colors                            |
| `AnimationTag`| Named frame range + playback direction, independent of pixel data |

- Layers are stored bottom → top. Frames are stored in playback order.
- IDs are stable `u32` values allocated from a per-document counter; indices
  are positions and change on reorder. Commands always address layers and
  frames by ID so batches stay valid while they reorder things.
- Cels are kept in a `BTreeMap<(LayerId, FrameId), Image>` for deterministic
  iteration. A missing cel means "fully transparent". In memory every cel is
  canvas-sized; on disk cels are trimmed to their non-transparent bounds.
- Colors serialize as `#rrggbbaa` hex strings.

### Projects and artboards (`project/`)

A `Project` is the unit a `.dotify` file stores: an ordered library of
`Artboard`s (each wraps a `Document` plus an `ArtboardId` and an optional
`FolderId`), a flat list of `Folder`s and one palette shared by every
artboard. `ProjectCommand` adds library commands (`create_artboard`,
`rename_artboard`, `duplicate_artboard`, `delete_artboard`, `move_artboard`,
folder commands, `set_palette`, `rename_project`) and `edit`, which runs any
document `Command` on one artboard. `ProjectEditor` is the project-level
`Editor`: one history for the whole project, atomic batches across
artboards, revision checks. Document `Op`s are reused inside
`ProjectOp::Document`, so drawing undo stays bit-exact. See
[`LIBRARY.md`](LIBRARY.md).

### Commands (`command.rs`)

`Command` is a serde-tagged enum (`{"type": "draw_line", ...}`) that is the
public editing vocabulary shared by every client:

- Drawing: `set_pixels`, `draw_line`, `draw_rectangle`, `draw_ellipse`,
  `fill_region`, `replace_color`, `clear_cel`
- Layers: `create_layer`, `duplicate_layer`, `delete_layer`, `update_layer`,
  `move_layer`
- Frames: `add_frame`, `duplicate_frame`, `delete_frame`, `move_frame`,
  `set_frame_duration`
- Palette / document: `set_palette`, `rename_document`
- `batch`: a list of commands applied atomically

Commands are validated against the document (unknown IDs, locked layers,
out-of-range values, deleting the last layer, …) and return a structured
`CoreError` with a stable machine-readable `code`.

### Operations and history (`ops.rs`, `editor.rs`)

Commands do not mutate the document directly. Each command is *compiled*
against the current document into one or more `Op`s:

| Op                 | Inverse                       |
|--------------------|-------------------------------|
| `PutRegion`        | `PutRegion` with the old pixels |
| `InsertLayer`      | `RemoveLayer`                 |
| `RemoveLayer`      | `InsertLayer` (with saved cels) |
| `SetLayer`         | `SetLayer` with old props     |
| `MoveLayer`        | `MoveLayer` back              |
| `InsertFrame` / `RemoveFrame` / `MoveFrame` / `SetFrame` | symmetric |
| `SetPalette`, `SetName` | old value                |

Applying an `Op` returns its inverse. A transaction records the inverses of
everything it applied; undo applies them in reverse order and collects *their*
inverses for redo. Because pixel ops carry exact before/after regions, undo
and redo are bit-exact and never re-run drawing algorithms.

`Editor` wraps a `Document` with `History`, a monotonically increasing
`revision`, and a saved-revision marker for dirty tracking. Key methods:

- `execute(cmd)` / `execute_expecting(cmd, expected_revision)` – runs a
  command (or batch) as one transaction. On error, already applied ops of the
  batch are rolled back and the document is unchanged. Revision checking lets
  remote clients (MCP, AI) refuse to apply edits made against a stale view.
- `undo()` / `redo()`
- Each call returns an `Outcome` with the new revision, dirty cel regions and
  whether the structure (layers/frames/palette) changed, which the desktop UI
  uses to refresh only what changed.

### Compositing (`composite.rs`)

Integer source-over with straight alpha and per-layer opacity. Only visible
layers participate. The algorithm is exact and tested; exports and thumbnails
use it. The desktop canvas uses the browser's compositor for live display,
which can differ by ±1 in partially transparent channels, but never affects
saved or exported data.

### Raster algorithms (`raster.rs`)

Bresenham lines, axis-aligned rectangles, midpoint ellipses inside a bounding
box, and scanline 4-connected flood fill with an explicit stack (no
recursion). The TypeScript previews in `apps/desktop/src/canvas/raster.ts`
mirror these algorithms so previews match committed results; the engine is
always authoritative.

## The `.dotify` format

See [`FORMAT.md`](FORMAT.md) for the full specification. In short: a ZIP
container with a `mimetype` entry, a `manifest.json` (format version, minimum
reader version, SHA-256 of every entry), `document.json` (all structured
data) and one lossless PNG per non-empty cel. Readers verify hashes, run
migrations for older versions and refuse files that need a newer reader.
Writes go to a temporary file in the same directory and are atomically
renamed into place.

## Desktop application

### Rust side (`apps/desktop/src-tauri`)

- Holds one `ProjectEditor` in a `Mutex` as Tauri managed state, plus the
  current file path and the active artboard.
- IPC commands: `doc_new`, `doc_open`, `doc_save`, `doc_save_as`,
  `doc_view`, `execute` (a `ProjectCommand`), `undo`, `redo`,
  `artboard_activate`, `read_cel`, `artboard_composite`, `export_png`,
  `export_all_png`, and recovery commands. `read_cel` and
  `artboard_composite` return raw RGBA bytes through `tauri::ipc::Response`
  (no JSON/base64 overhead).
- File dialogs are opened **from Rust**, so the webview never supplies
  arbitrary filesystem paths to the backend. The webview has no filesystem
  permissions.
- Native menu with macOS conventions; menu actions are forwarded to the UI as
  `menu` events so the UI decides what to do (e.g. confirm before discarding).
- Autosave: a background thread writes a recovery copy to the app data
  directory every 30 s when the document has unsaved changes. On launch the
  UI offers to restore it. Recovery files are removed after a successful save
  or a clean discard.

### TypeScript side (`apps/desktop/src`)

- `engine/` – typed wrappers over `invoke`; TypeScript mirrors of the Rust
  command and summary types (`types.ts`). Keep them in sync with
  `command.rs` / `summary.rs`.
- `canvas/CanvasView.ts` – owns the `<canvas>` element, the view transform
  (integer zoom, device-pixel-aligned offsets for HiDPI), pointer handling and
  redraw scheduling via `requestAnimationFrame`. Each layer of the current
  frame has an offscreen canvas that is updated with `putImageData` only for
  dirty regions; the screen pass is a handful of nearest-neighbour
  `drawImage` calls plus checkerboard and grid.
- `canvas/tools.ts` – tool behaviours. Pencil/eraser paint into the local
  cache for zero-latency feedback, then commit one `set_pixels` command per
  stroke; shape tools draw into a preview layer and commit a shape command;
  the bucket calls the engine and the eyedropper samples the cache.
  After every commit the dirty region is re-read from the engine, so the
  cache always converges to the authoritative state. All engine calls go
  through one promise queue in `app/controller.ts`, keeping commands and
  read-backs strictly ordered.
- `canvas/OverviewView.ts` – the artboard overview: every artboard side by
  side (flow layout per folder from `overviewLayout.ts`), pan/zoom, click to
  select, double-click to edit. Artboard composites come from
  `ArtboardImages`, a debounced cache shared with the library thumbnails.
- `components/LibraryPanel.tsx` – library sidebar: search, folders, drag and
  drop, native context menus.
- `state/` – Zustand stores for tool settings, colors, document summary,
  active layer/frame, mode (editor/overview) and UI preferences. Components
  subscribe with narrow selectors to avoid re-renders.

## Testing strategy

- `dotify-core`: unit tests for raster algorithms, flood fill, compositing,
  every command, undo/redo round trips, batch atomicity, revision checking and
  format round trips/corruption detection.
- `dotify-core` projects: every library command with undo/redo, palette
  sharing, cross-artboard batches; v1 → v2 migration against JSON and real
  `.dotify` fixtures.
- `dotify-export`: PNG export pixel equality and scaling, batch export layout.
- `dotify-cli`: end-to-end run against a temp directory.
- Frontend: Vitest tests for rasterisers, view math, overview layout and
  library drag-and-drop targets.

## Roadmap

| Phase | Scope |
|-------|-------|
| 1 (current) | Tauri app, workspace, canvas, pencil/eraser/fill/line/rect/ellipse/eyedropper, zoom/pan, palette, layers, undo/redo, save/load, PNG export, autosave/recovery |
| 2 | Timeline playback, onion skinning, tags, live preview, GIF/spritesheet + JSON export |
| 3 | `dotify-mcp` stdio server over the shared command engine; permissions, revision checks, confirmations |
| 4 | Built-in AI assistant, provider abstraction (pixel-command vs raster generation), keychain-stored keys, preview/approve/undo |
| 5 | Selections and transforms, tilemaps, engine export presets, Windows/Linux |

### Extension points kept open on purpose

- **Rendering backend**: `CanvasView` isolates all drawing; a WebGL/WebGPU
  renderer can replace the 2D context without touching tools or panels.
- **Selections**: commands take explicit target regions; a future
  `selection` mask on the editor will restrict `PutRegion` writes.
- **Tilemaps**: `Layer` has a `kind` field (currently only `pixel`) so
  `tilemap` layers can be added without a format break; tileset data will
  live in new archive entries referenced from `document.json`.
- **AI**: AI providers will produce `Command` batches (pixel-command
  generation) or RGBA images that are imported through `set_pixels`/region
  commands (raster generation). Both land in history as a single undoable
  transaction.
