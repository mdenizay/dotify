# Dotify MCP server

`dotify-mcp` lets AI assistants open, draw in, animate and export Dotify
projects through the [Model Context Protocol](https://modelcontextprotocol.io).
It drives `dotify_core::Editor`, the same command engine as the desktop app and
the `dotify` CLI, so every tool call is one undoable transaction with the same
validation and the same results as editing by hand.

It speaks two transports:

| Transport | Command | For |
|-----------|---------|-----|
| stdio | `dotify-mcp --root DIR` | Claude Code, Claude Desktop and other local clients |
| Streamable HTTP | `dotify-mcp --root DIR http` | ChatGPT developer mode (through an HTTPS tunnel) and other remote clients |

## Install

```bash
brew install mdenizay/tap/dotify-cli
```

The `dotify-cli` formula installs both `dotify` and `dotify-mcp`. From source:

```bash
cargo install --path crates/dotify-mcp
```

## Security model

- **Root directories.** The server only reads and writes inside the
  directories given with `--root` (repeatable, or `DOTIFY_MCP_ROOTS` as a
  `:`-separated list). It refuses to start without one. Paths are resolved
  with symlinks followed (`canonicalize`); anything that ends up outside every
  root, including through `..` or a symlink, fails with `path_outside_roots`.
  Relative paths start at the first root. Projects must end in `.dotify`,
  exports in `.png`/`.gif`.
- **Confirmation.** Tools that delete artboards, layers or frames
  (`artboard_delete`, `layer_delete`, `delete_frame`, or a `batch` containing
  them), replace an existing file
  (saving over another file, any export onto existing files) or close a project
  with unsaved changes fail with `confirmation_required` unless called with
  `confirm: true`. Saving a project to its own file needs no confirmation.
- **Read-only mode.** `--read-only` (or `DOTIFY_MCP_READ_ONLY=true`) offers
  only the reading tools: `project_list`, `project_open`, `project_get`,
  `project_close`, `artboard_list`, `layer_list` and `get_canvas`.
- **HTTP.** The HTTP transport listens on `127.0.0.1:8765` by default and
  requires a bearer token of at least 24 characters (`dotify-mcp token` prints
  a random one). Requests with an unexpected `Host` header are rejected
  (DNS-rebinding protection); add tunnel hostnames with `--allowed-host`.
  `--allow-token-in-url` additionally accepts the token as the last path
  segment, `/mcp/<token>`, for clients that cannot send headers. Treat that
  URL like a password.

Edits stay in memory until `project_save`. Exports render the in-memory state,
so you can export without saving.

## Claude Code

```bash
claude mcp add dotify --scope user -- dotify-mcp --root ~/Art
```

Several roots: `-- dotify-mcp --root ~/Art --root ~/game/sprites`. Add
`--read-only` after `dotify-mcp` for a look-only server. Check it with
`claude mcp list`, or `/mcp` inside a session.

To use a running HTTP server instead:

```bash
claude mcp add dotify --scope user --transport http http://127.0.0.1:8765/mcp --header "Authorization: Bearer $DOTIFY_MCP_TOKEN"
```

## Claude Desktop

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`
(Settings → Developer → Edit Config) and restart Claude Desktop. Claude
Desktop does not use your shell's `PATH`, so give the full path
(`which dotify-mcp`; Homebrew on Apple silicon installs to `/opt/homebrew/bin`):

```json
{
  "mcpServers": {
    "dotify": {
      "command": "/opt/homebrew/bin/dotify-mcp",
      "args": ["--root", "/Users/you/Art"]
    }
  }
}
```

## ChatGPT developer mode

ChatGPT connects only to remote MCP servers over HTTPS, so run the HTTP
transport locally and expose it through a tunnel. Developer mode is a beta for
paid plans, and which plans may use write tools has changed over time; check
OpenAI's current documentation if tools show up but edits are refused.

ChatGPT connectors offer OAuth or no authentication, not a custom bearer
header, so this setup uses the secret URL form (`--allow-token-in-url`).

1. Create a token and keep it somewhere safe:

   ```bash
   export DOTIFY_MCP_TOKEN="$(dotify-mcp token)"
   ```

2. Start the server (it stays on loopback; only the tunnel reaches it):

   ```bash
   dotify-mcp --root ~/Art http --allow-token-in-url
   ```

3. Start a tunnel in another terminal. Either rewrite the `Host` header to
   the local address, as below, or pass the tunnel hostname to the server with
   `--allowed-host <name>`.

   Cloudflare Tunnel (`brew install cloudflared`; a quick tunnel needs no
   account and prints a `https://….trycloudflare.com` URL that changes each
   run):

   ```bash
   cloudflared tunnel --url http://127.0.0.1:8765 --http-host-header 127.0.0.1:8765
   ```

   ngrok (`brew install ngrok`, then `ngrok config add-authtoken …`):

   ```bash
   ngrok http 8765 --host-header=rewrite
   ```

4. In ChatGPT, open Settings → Apps & Connectors → Advanced settings and turn
   on Developer mode. Then create a connector (Settings → Apps & Connectors →
   Create):
   - **MCP Server URL:** `https://<tunnel-host>/mcp/<your token>`
   - **Authentication:** No authentication
   - Confirm that you trust the server.

5. In a chat, choose Developer mode and enable the Dotify connector. ChatGPT
   asks before running tools that write.

Stop the tunnel when you are done; anyone with the URL can use the server while
it runs. To revoke access, restart the server with a new token.

## Tools

All tools take JSON arguments described by their JSON Schema (`tools/list`).
Unknown fields are rejected. Artboards, layers and frames are addressed by
their stable IDs from `project_get`, never by position.

A project is a library of artboards: separate sprites, each with its own
size, layers, frames and tags, sharing one palette. Every tool that reads or
edits pixels, layers, frames or tags, and every export, takes an optional
`artboard`. Leave it out when the project has a single artboard. With several
artboards, leaving it out fails with `artboard_required`, and the error's
`details.artboards` lists the artboards to choose from.

| Tool | Does |
|------|------|
| `project_list` | Open projects and `.dotify` files under the roots |
| `project_create` | New project (optionally saved to a path); returns its handle (`p1`, …) |
| `project_open` | Open a `.dotify` file; opening the same file again returns the same handle |
| `project_get` | Artboards (size, layers, frames, tags), folders, shared palette, revision, unsaved changes, undo state |
| `project_save` | Save to its file or a new path |
| `project_close` | Close a project |
| `artboard_list`, `artboard_create`, `artboard_rename`, `artboard_duplicate`, `artboard_delete` | Artboards (sprites) in the project library |
| `get_canvas` | A frame (or one layer) as a PNG image; `include_pixels` adds the exact pixels as text rows |
| `set_pixels`, `draw_line`, `draw_rectangle`, `draw_ellipse`, `fill_region`, `replace_color` | Drawing |
| `layer_list`, `layer_create`, `layer_update`, `layer_delete` | Layers |
| `add_frame`, `duplicate_frame`, `delete_frame`, `move_frame`, `set_frame_duration`, `copy_frame`, `copy_silhouette` | Frames |
| `tag_create`, `tag_update`, `tag_delete` | Animation tags |
| `set_palette` | Palette shared by all artboards |
| `batch` | Several edits as one atomic undo step |
| `undo`, `redo` | History |
| `export_png`, `export_gif`, `export_spritesheet`, `export_godot` | Exports, same options as the CLI |

Edit tools return the new `revision`, whether anything `changed`, the IDs of
`created` layers/frames and the `changed_regions`. Pass `expected_revision` to
refuse the edit if the project changed since you last looked
(`revision_mismatch`).

`batch` takes `ops`, each an object with `op` set to an edit tool name and that
tool's arguments without `project`. The batch's `artboard` is the default for
ops that don't name one:

```json
{
  "project": "p1",
  "artboard": 1,
  "label": "Draw hero",
  "ops": [
    { "op": "layer_create", "name": "Outline" },
    { "op": "draw_rectangle", "layer": 1, "frame": 2, "x": 4, "y": 4, "width": 8, "height": 8, "color": "#ff004d", "filled": true },
    { "op": "tag_create", "name": "idle", "from": 2, "to": 2 }
  ]
}
```

If any op fails, nothing is applied.

### Errors

Failures are returned as tool results with `isError: true` and a structured
body the model can act on:

```json
{ "error": { "code": "layer_locked", "message": "layer 3 is locked" } }
```

Engine errors keep `CoreError::code()` (`artboard_not_found`, `last_artboard`,
`layer_not_found`, `frame_not_found`, `layer_locked`, `tag_not_found`,
`last_layer`, `last_frame`, `invalid_canvas_size`, `invalid_argument`,
`revision_mismatch`), and export and file errors keep theirs
(`too_many_colors`, `invalid_scale`, `not_dotify`, `corrupt_file`, …). The
server adds `invalid_arguments` (arguments don't match the schema),
`artboard_required`, `project_not_found`, `path_outside_roots`,
`invalid_path`, `not_found`, `confirmation_required`, `read_only`, `no_path`,
`project_open`, `too_many_projects`, `too_large` and `io_error`.

## Limits

- At most 32 open projects and 10,000 ops per batch.
- `get_canvas` previews are at most 4096 pixels on a side; `include_pixels`
  works up to 128×128 pixels and 62 colors.
- The server keeps open projects in memory for its lifetime. Over HTTP every
  session shares them, so two clients editing the same project should use
  `expected_revision`.
- Projects opened in the desktop app and over MCP at the same time are separate
  copies; whichever saves last wins.
