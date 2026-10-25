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
│   ├── dotify-core/            Document model, commands, history, compositing,
│   │                           PNG codec, .dotify format (read/write/migrate)
│   ├── dotify-export/          PNG, spritesheet + JSON metadata, GIF, Godot 4
│   ├── dotify-cli/             Headless project manipulation and export
│   └── dotify-mcp/             MCP server (stdio + Streamable HTTP)
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

## Layering

```
 ┌───────────────────────────────┐   ┌──────────────┐  ┌──────────────┐
 │ React panels (Zustand stores) │   │  dotify-cli  │  │  dotify-mcp  │
 ├───────────────────────────────┤   │              │  │              │
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

### Commands (`command.rs`)

`Command` is a serde-tagged enum (`{"type": "draw_line", ...}`) that is the
public editing vocabulary shared by every client:

- Drawing: `set_pixels`, `draw_line`, `draw_rectangle`, `draw_ellipse`,
  `fill_region`, `replace_color`, `clear_cel`
- Layers: `create_layer`, `duplicate_layer`, `delete_layer`, `update_layer`,
  `move_layer`
- Frames: `add_frame`, `duplicate_frame`, `delete_frame`, `move_frame`,
  `set_frame_duration`
- Tags: `create_tag`, `update_tag`, `delete_tag` (addressed by unique name)
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
| `SetPalette`, `SetName`, `SetTags` | old value     |

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

## dotify-export

All exporters composite with `composite.rs` and scale with integer
nearest-neighbour; nothing is resampled or quantized.

- `sheet.rs` – packs frames into a spritesheet: `grid` (each frame once,
  near-square or fixed columns) or `rows_per_tag` (one row per tag, then
  untagged frames). `padding` separates cells, `extrude` repeats each cell's
  edge pixels outwards. Also defines `playback_order`, the one definition of
  forward/reverse/ping-pong order (mirrored in the desktop `playback.ts`).
- `meta.rs` – engine-neutral JSON: frame rects, durations, tags with their
  playback sequence, pivot.
- `gif.rs` – animated GIF with exact colors (a global palette when ≤ 255
  colors overall, else per-frame palettes; more than 255 in one frame is an
  error). Partial alpha is rounded at 128 and reported.
- `godot.rs` – Godot 4 `SpriteFrames` `.tres` (one `AtlasTexture` per used
  cell, one animation per tag, `speed = 1000 / gcd(durations)`), written
  together with the sheet PNG and JSON.

## dotify-mcp

A Model Context Protocol server built on the official Rust SDK (`rmcp`). See
[`MCP.md`](MCP.md) for setup and the tool list.

- `tools.rs` – `Dotify`, the transport-independent tool set: open projects
  (`Workspace`, handles `p1`, `p2`, …, each an `Editor` plus its file path),
  argument parsing, confirmation and overwrite checks, exports. Edit tools
  compile to one `Command` and run through `Editor::execute_expecting`, so
  each call (or `batch`) is one undoable transaction with an optional
  revision check.
- `args.rs` – the tools' argument types with their JSON Schemas (schemars).
  They mirror `Command` rather than adding schema derives to the core;
  `EditOp::into_command` is the only mapping. `batch` takes a list of
  `EditOp`s named like the single edit tools.
- `sandbox.rs` – every path goes through `Sandbox`: canonicalized, required
  to stay inside a `--root`, extension-checked.
- `server.rs` – rmcp `ServerHandler`: lists tools (minus writing tools in
  read-only mode), runs calls on the blocking pool and turns `ToolError`s
  into `isError` results with a structured `{ error: { code, message } }`.
- `http.rs` – Streamable HTTP under `/mcp` with bearer-token auth (optionally
  `/mcp/<token>` for clients without custom headers) and rmcp's `Host`
  allow-list.

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

- Holds one `Editor` in a `Mutex` as Tauri managed state, plus the current
  file path.
- IPC commands: `doc_new`, `doc_open`, `doc_save`, `doc_save_as`,
  `doc_summary`, `execute`, `undo`, `redo`, `read_cel`, `read_frame` (an
  engine-composited frame), `export_png`, `export_sheet`, `export_gif`,
  `export_godot`, and recovery commands. `read_cel` returns raw RGBA bytes through
  `tauri::ipc::Response` (no JSON/base64 overhead).
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
- `canvas/FrameStore.ts` – engine composites of whole frames (`read_frame`)
  for playback, onion skins and the preview. Loaded lazily, refetched after
  edits that touch them, old pixels shown until the new ones arrive.
- `canvas/playback.ts` – `Player`, a `requestAnimationFrame` clock that
  walks a frame range by per-frame durations (loop/once, forward/reverse/
  ping-pong). The timeline player drives the canvas, which shows composites
  while playing; any edit pauses it on the shown frame. A second player
  loops in the preview panel (`canvas/PreviewView.ts`) while you draw.
- `canvas/onion.ts` – which neighbouring frames to draw under the active
  one, and how faded. `CanvasView` draws them (optionally tinted) below the
  layers.
- `canvas/tools.ts` – tool behaviours. Pencil/eraser paint into the local
  cache for zero-latency feedback, then commit one `set_pixels` command per
  stroke; shape tools draw into a preview layer and commit a shape command;
  the bucket calls the engine and the eyedropper samples the cache.
  After every commit the dirty region is re-read from the engine, so the
  cache always converges to the authoritative state. All engine calls go
  through one promise queue in `app/controller.ts`, keeping commands and
  read-backs strictly ordered.
- `state/` – Zustand stores for tool settings, colors, document summary,
  active layer/frame, and UI preferences. Components subscribe with narrow
  selectors to avoid re-renders.

## Testing strategy

- `dotify-core`: unit tests for raster algorithms, flood fill, compositing,
  every command, undo/redo round trips, batch atomicity, revision checking and
  format round trips/corruption detection.
- `dotify-export`: PNG export pixel equality and scaling; spritesheet
  layouts, padding and extrusion; GIF round trips (colors, delays, loop,
  per-frame palettes); a snapshot of the Godot `.tres`
  (`tests/snapshots/hero.tres`, refresh with `DOTIFY_UPDATE_SNAPSHOTS=1`).
- `dotify-cli`: end-to-end run against a temp directory.
- `dotify-mcp`: path sandboxing (traversal, symlinks), argument validation,
  confirmations, read-only mode, every tool against a temp root, HTTP token
  and host checks, and an end-to-end JSON-RPC session with the binary over
  stdio.
- Frontend: Vitest tests for rasterisers and view math.

## Roadmap

| Phase | Scope |
|-------|-------|
| 1 | Tauri app, workspace, canvas, pencil/eraser/fill/line/rect/ellipse/eyedropper, zoom/pan, palette, layers, undo/redo, save/load, PNG export, autosave/recovery |
| 2 | Timeline playback, onion skinning, tags, live preview, GIF/spritesheet + JSON export, Godot 4 export |
| 3 (current) | `dotify-mcp` server (stdio + Streamable HTTP) over the shared command engine; root sandboxing, token auth, revision checks, confirmations, read-only mode |
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
