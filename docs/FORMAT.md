# The `.dotify` Project Format

Status: **format version 2** (implemented in `crates/dotify-core/src/format`).

A `.dotify` file is a ZIP archive. All structured data is JSON (UTF-8); all
pixel data is stored losslessly as 8-bit RGBA PNG, one image per non-empty
cel. The editable document is never reduced to a flattened image.

## Archive layout

| Entry             | Compression | Required | Purpose |
|-------------------|-------------|----------|---------|
| `mimetype`        | stored      | yes      | Must be the first entry. Contents: `application/x-dotify` |
| `manifest.json`   | deflate     | yes      | Versioning and integrity |
| `document.json`   | deflate     | yes      | Canvas, layers, frames, cel index, palettes, tags, export settings |
| `cels/<layer>-<frame>.png` | stored | per cel | Trimmed RGBA PNG of one cel |
| `thumbnail.png`   | stored      | no       | Composite of the first frame, for file browsers |

PNG entries are stored without ZIP compression because PNG is already
deflate-compressed.

## `manifest.json`

```json
{
  "format": "dotify",
  "format_version": 2,
  "min_reader_version": 1,
  "generator": "dotify 0.1.0",
  "entries": {
    "document.json": "sha256:…",
    "cels/1-1.png": "sha256:…"
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
  "name": "Hero",
  "width": 32,
  "height": 32,
  "layers": [
    { "id": 1, "name": "Background", "kind": "pixel",
      "visible": true, "locked": false, "opacity": 255, "blend_mode": "normal" }
  ],
  "frames": [ { "id": 1, "duration_ms": 100 } ],
  "cels": [
    { "layer": 1, "frame": 1, "x": 3, "y": 4, "width": 10, "height": 12,
      "file": "cels/1-1.png" }
  ],
  "palette": { "name": "Default", "colors": ["#000000ff", "#1d2b53ff"] },
  "tags": [
    { "name": "Idle", "from": 1, "to": 1, "direction": "forward", "loop": true,
      "color": "#5b8cffff" }
  ],
  "export": { "png_scale": 1 },
  "next_id": 3,
  "extensions": {}
}
```

Rules:

- Layer order is bottom → top; frame order is playback order.
- `from`/`to` in tags are **frame IDs**, so tags survive frame reordering
  and are independent of pixel data. `from` never comes after `to` in
  timeline order (moving frames swaps the endpoints when needed). Tag names
  are unique. `direction` is `forward`, `reverse` or `ping_pong`; `loop`
  says whether the range repeats.
- Cel `x`/`y`/`width`/`height` describe where the trimmed PNG sits inside
  the canvas. Cels that would be fully transparent are omitted.
- Colors are `#rrggbbaa` (straight alpha).
- `extensions` is an object reserved for forward-compatible additions. Readers
  must preserve unknown keys in it when re-saving.
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
change must add a migration and a test fixture.

| Version | Change | Migration |
|---------|--------|-----------|
| 1 | Initial format | – |
| 2 | Tags gain `loop` | Every v1 tag gets `"loop": true`. `min_reader_version` stays 1: older readers ignore the field. |

## Atomic writes

Writers serialize to `<target>.tmp-<pid>` in the destination directory, flush
and `fsync`, then rename over the target. A crash mid-write never leaves a
truncated project behind.
