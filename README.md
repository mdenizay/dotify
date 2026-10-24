# Dotify

A lightweight, open-source pixel art editor and sprite animation studio for
indie game developers — built so that AI assistants can eventually drive it
through the same command engine you use by hand.

> **Status: Phase 1 (functional pixel editor).** macOS first; Windows and
> Linux are planned. See the [roadmap](docs/ARCHITECTURE.md#roadmap).

## What works today

- Tauri 2 desktop app with a native macOS menu and dark/light themes
- Pixel-exact canvas: integer zoom (100%–12800%), pan (Space/middle-drag/
  trackpad), HiDPI aware, optional pixel grid
- Tools: pencil and eraser (1–32 px square brush), bucket fill (contiguous
  or global), line, rectangle, ellipse (outline/filled, Shift for square),
  eyedropper (Alt while drawing), hand
- Horizontal/vertical symmetry for every drawing tool
- Primary/secondary colors (left/right click), HSV + alpha picker, editable
  palette (PICO-8 default)
- Layers: create, duplicate, delete, rename, reorder, visibility, lock,
  opacity
- Frames: add, duplicate, delete, reorder, per-frame duration (playback
  arrives in Phase 2)
- Project library: many artboards (sprites) per project, each with its own
  size, layers and frames, sharing one palette; folders, search, drag and
  drop, and an overview of all artboards side by side
- Undo/redo of every edit, with labels
- Native `.dotify` project format (versioned ZIP, lossless, checksummed),
  atomic saves, autosave with crash recovery
- PNG export at 1×–16× with nearest-neighbour scaling, for one artboard or
  all of them at once
- `dotify` CLI for headless editing and export

## Install

With [Homebrew](https://brew.sh) (macOS 13+, Apple Silicon):

```bash
brew install --cask mdenizay/tap/dotify
```

The command-line tool is a separate formula:

```bash
brew install mdenizay/tap/dotify-cli
```

Builds are signed with a Developer ID and notarized by Apple. Dotify checks
for updates on launch and every few hours, and from **Dotify → Check for
Updates…**. Updates are signed and verified before they install, and Dotify
offers to save your work before restarting. Homebrew leaves updating to the
app (`auto_updates true`), so `brew upgrade` won't fight it.

## Building from source

Requirements: Rust (stable, ≥ 1.85), Node.js ≥ 20, pnpm 10, and Xcode
command line tools on macOS.

```bash
pnpm install
```

```bash
pnpm dev
```

`pnpm dev` starts Vite and the Tauri app with hot reload. Other scripts:

| Command            | What it does                                        |
|--------------------|-----------------------------------------------------|
| `pnpm build`       | Release build and macOS bundle (`target/release/bundle`) |
| `tools/release.sh` | Sign, notarize, publish a GitHub release and update the Homebrew tap (maintainers) |
| `pnpm test`        | Rust workspace tests + frontend unit tests          |
| `pnpm lint`        | `cargo clippy -D warnings` + ESLint                 |
| `pnpm format`      | `cargo fmt` + Prettier                              |
| `pnpm typecheck`   | Strict TypeScript check                             |

### CLI

```bash
cargo run -p dotify-cli -- new hero.dotify --width 32 --height 32
```

```bash
cargo run -p dotify-cli -- info hero.dotify
```

```bash
echo '[{"type":"draw_rectangle","layer":1,"frame":2,"x":4,"y":4,"width":8,"height":8,"color":"#ff004d","filled":true}]' | cargo run -p dotify-cli -- apply hero.dotify -
```

```bash
cargo run -p dotify-cli -- export-png hero.dotify hero.png --scale 8
```

```bash
cargo run -p dotify-cli -- export-all hero.dotify out/ --scale 4
```

`apply` takes the same JSON commands as the desktop app: project commands
(`create_artboard`, `create_folder`, `edit`, … in
`crates/dotify-core/src/project/command.rs`) and document commands (in
`crates/dotify-core/src/command.rs`), which apply to `--artboard` (default:
the first artboard). An array is applied atomically.

## Keyboard shortcuts

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| B | Pencil | E | Eraser |
| G | Bucket fill | L | Line |
| U | Rectangle | O / ⇧U | Ellipse |
| I | Eyedropper | H | Hand |
| X | Swap colors | [ / ] | Brush size |
| , / . | Previous / next frame | ⇧N | New layer |
| Space | Pan while held | Alt | Eyedropper while held |
| ⌘Z / ⇧⌘Z | Undo / redo | ⌘= / ⌘− | Zoom |
| ⌘0 | Fit | ⌘1 | Actual size |
| ⌘' | Pixel grid | ⌘E | Export PNG |

## Project layout

```
crates/dotify-core     document model, commands, undo/redo, compositing, .dotify format
crates/dotify-export   PNG export (GIF, spritesheets and metadata next)
crates/dotify-cli      headless `dotify` binary
apps/desktop           Tauri + React app (src/ = UI, src-tauri/ = native shell)
docs/                  ARCHITECTURE.md, FORMAT.md
```

Read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) before contributing and
[docs/FORMAT.md](docs/FORMAT.md) for the file format.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE) at your option — the standard choice in the
Rust ecosystem, permissive for game developers, and Apache-2.0 adds an
explicit patent grant. Contributions are accepted under the same terms.
