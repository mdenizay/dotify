# The `.dotify` Project Format

Status: **format version 2** (implemented in `crates/dotify-core/src/format`).

A `.dotify` file is a ZIP archive holding one *project*: a library of
artboards (sprites), each with its own canvas size, layers, frames, cels and
tags, plus a palette shared by all of them and optional folders. See
[`LIBRARY.md`](LIBRARY.md) for the design. All structured data is JSON
(UTF-8); all pixel data is stored losslessly as 8-bit RGBA PNG, one image per
non-empty cel. The editable project is never reduced to flattened images.

## Archive layout

| Entry             | Compression | Required | Purpose |
|-------------------|-------------|----------|---------|
| `mimetype`        | stored      | yes      | Must be the first entry. Contents: `application/x-dotify` |
| `manifest.json`   | deflate     | yes      | Versioning and integrity |
| `document.json`   | deflate     | yes      | Project: palette, folders, artboards (canvas, layers, frames, cel index, tags, export settings) |
| `artboards/<artboard>/cels/<layer>-<frame>.png` | stored | per cel | Trimmed RGBA PNG of one cel |
| `thumbnail.png`   | stored      | no       | Composite of the first frame of the first artboard, for file browsers |

Readers locate cel PNGs through each cel's `file` field, never by building
the path themselves (files migrated from v1 keep their `cels/…` paths).

PNG entries are stored without ZIP compression because PNG is already
deflate-compressed.

## `manifest.json`

```json
{
  "format": "dotify",
  "format_version": 2,
  "min_reader_version": 2,
  "generator": "dotify 0.1.0",
  "entries": {
    "document.json": "sha256:…",
    "artboards/1/cels/1-2.png": "sha256:…"
  }
}
```

- `format_version` – version this file was written with.
- `min_reader_version` – oldest reader version that can safely open the file.
  A reader whose supported version is lower than this must refuse the file.
  Writers bump it only for incompatible changes; additive changes keep it.
- `entries` – SHA-256 of every entry except `mimetype` and `manifest.json`.
  Readers verify every listed entry and reject the file on mismatch or if a
  listed entry is missing (corruption detection on top of ZIP CRC-32).

## `document.json`

```json
{
  "name": "Quest",
  "palette": { "name": "Default", "colors": ["#000000ff", "#1d2b53ff"] },
  "folders": [ { "id": 2, "name": "Characters" } ],
  "artboards": [
    {
      "id": 1,
      "folder": 2,
      "name": "Hero",
      "width": 32,
      "height": 32,
      "layers": [
        { "id": 1, "name": "Background", "kind": "pixel",
          "visible": true, "locked": false, "opacity": 255, "blend_mode": "normal" }
      ],
      "frames": [ { "id": 2, "duration_ms": 100 } ],
      "cels": [
        { "layer": 1, "frame": 2, "x": 3, "y": 4, "width": 10, "height": 12,
          "file": "artboards/1/cels/1-2.png" }
      ],
      "tags": [
        { "name": "Idle", "from": 2, "to": 2, "direction": "forward", "color": "#5b8cffff" }
      ],
      "export": { "png_scale": 1 },
      "next_id": 3,
      "extensions": {}
    }
  ],
  "next_id": 3,
  "extensions": {}
}
```

Rules:

- A project has at least one artboard. Artboard order is library order.
- Artboard and folder IDs share the project's `next_id` counter and are
  unique within the project. `folder` is `null` for artboards outside any
  folder; folders are a flat list.
- The project `palette` is shared by every artboard; artboards do not store
  their own.
- Each artboard is a self-contained document: its layer and frame IDs come
  from its own `next_id` and are scoped to that artboard.
- Layer order is bottom → top; frame order is playback order.
- `from`/`to` in tags are **frame IDs**, so tags survive frame reordering
  and are independent of pixel data.
- Cel `x`/`y`/`width`/`height` describe where the trimmed PNG sits inside
  the canvas. Cels that would be fully transparent are omitted.
- Colors are `#rrggbbaa` (straight alpha).
- `extensions` (on the project and on each artboard) is an object reserved
  for forward-compatible additions. Readers must preserve unknown keys in it
  when re-saving.
- Unknown top-level fields are ignored by readers.

## Versioning and migrations

On load the reader:

1. Checks `mimetype`.
2. Parses `manifest.json`, rejects if `min_reader_version` > supported.
3. Verifies hashes.
4. Parses `document.json` as generic JSON and runs migrations
   `v(n) → v(n+1)` up to the current version, then deserializes into typed
   structures.

Migrations live in `crates/dotify-core/src/format/migrate.rs`. Every format
change must add a migration and a test fixture (`crates/dotify-core/tests/fixtures`).

| Step | Change |
|------|--------|
| 1 → 2 | The v1 single document becomes a project with one artboard (ID 1, no folder, project `next_id` 2). Its `palette` moves to the project; everything else, including cel `file` paths and `extensions`, moves into the artboard unchanged. The project is named after the document. |

`min_reader_version` is 2 for files written by this version: a v1 reader
must not open a project it would misread as a single document.

## Atomic writes

Writers serialize to `<target>.tmp-<pid>` in the destination directory, flush
and `fsync`, then rename over the target. A crash mid-write never leaves a
truncated project behind.
