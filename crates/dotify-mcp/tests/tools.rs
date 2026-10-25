//! Tool behaviour through `Dotify::call`, without a transport.

use dotify_core::format;
use dotify_mcp::{Dotify, Options, Sandbox, ToolError, ToolOutput};
use serde_json::{json, Value};

struct Fixture {
    root: tempfile::TempDir,
    dotify: Dotify,
}

fn fixture(read_only: bool) -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let sandbox = Sandbox::new([root.path().to_path_buf()]).unwrap();
    Fixture { dotify: Dotify::new(Options { sandbox, read_only }), root }
}

impl Fixture {
    fn call(&self, name: &str, args: Value) -> Result<Value, ToolError> {
        let Value::Object(args) = args else { panic!("arguments must be an object") };
        match self.dotify.call(name, args)? {
            ToolOutput::Json(v) => Ok(v),
            ToolOutput::Image { info, .. } => Ok(info),
        }
    }

    fn ok(&self, name: &str, args: Value) -> Value {
        self.call(name, args).unwrap_or_else(|e| panic!("{name} failed: {e}"))
    }

    fn err(&self, name: &str, args: Value) -> ToolError {
        match self.call(name, args) {
            Ok(v) => panic!("{name} unexpectedly succeeded: {v}"),
            Err(e) => e,
        }
    }

    /// A 4×4 project with its first layer and frame IDs.
    fn project(&self) -> (String, u64, u64) {
        let v = self.ok("project_create", json!({ "width": 4, "height": 4, "path": "art.dotify" }));
        let id = v["project"].as_str().unwrap().to_string();
        let layer = v["summary"]["layers"][0]["id"].as_u64().unwrap();
        let frame = v["summary"]["frames"][0]["id"].as_u64().unwrap();
        (id, layer, frame)
    }
}

#[test]
fn edits_are_undoable_transactions() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    let r = f.ok(
        "draw_rectangle",
        json!({ "project": p, "layer": l, "frame": fr, "x": 0, "y": 0, "width": 4, "height": 4, "color": "#ff0000", "filled": true }),
    );
    assert_eq!(r["revision"], 1);
    assert_eq!(r["changed"], true);
    assert_eq!(r["unsaved_changes"], true);
    let canvas = f.ok("get_canvas", json!({ "project": p, "include_pixels": true }));
    assert_eq!(canvas["pixels"]["rows"], json!(["0000", "0000", "0000", "0000"]));
    assert_eq!(canvas["pixels"]["legend"]["0"], "#ff0000ff");
    f.ok("undo", json!({ "project": p }));
    let canvas = f.ok("get_canvas", json!({ "project": p, "include_pixels": true }));
    assert_eq!(canvas["pixels"]["rows"], json!(["....", "....", "....", "...."]));
    let r = f.ok("redo", json!({ "project": p }));
    assert_eq!(r["revision"], 3);
}

#[test]
fn get_canvas_returns_a_png() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    f.ok(
        "set_pixels",
        json!({ "project": p, "layer": l, "frame": fr, "pixels": [{ "x": 1, "y": 2, "color": "#00ff00" }] }),
    );
    let Value::Object(args) = json!({ "project": p, "scale": 2 }) else { unreachable!() };
    let ToolOutput::Image { png, info } = f.dotify.call("get_canvas", args).unwrap() else {
        panic!("expected an image")
    };
    let img = dotify_core::png_io::decode_rgba(&png, 4096).unwrap();
    assert_eq!((img.width(), img.height()), (8, 8));
    assert_eq!(img.get(2, 4), dotify_core::Rgba::opaque(0, 255, 0));
    assert_eq!(info["scale"], 2);
    // Auto scale makes small canvases big enough to look at.
    let auto = f.ok("get_canvas", json!({ "project": p }));
    assert_eq!(auto["scale"], 16);
}

#[test]
fn rejects_invalid_arguments() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    let base = |extra: Value| {
        let mut v = json!({ "project": p, "layer": l, "frame": fr, "x": 0, "y": 0, "color": "#fff" });
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    };
    assert_eq!(f.err("fill_region", base(json!({ "colour": "#fff" }))).code, "invalid_arguments");
    assert_eq!(f.err("fill_region", base(json!({ "color": "red" }))).code, "invalid_arguments");
    assert_eq!(f.err("fill_region", base(json!({ "x": "1" }))).code, "invalid_arguments");
    assert_eq!(f.err("fill_region", json!({ "layer": l, "frame": fr })).code, "invalid_arguments");
    assert_eq!(f.err("fill_region", base(json!({ "project": 5 }))).code, "invalid_arguments");
    assert_eq!(f.err("fill_region", base(json!({ "confirm": true }))).code, "invalid_arguments");
    assert_eq!(f.err("fill_region", base(json!({ "project": "p99" }))).code, "project_not_found");
    // Engine errors keep CoreError::code().
    assert_eq!(f.err("fill_region", base(json!({ "layer": 999 }))).code, "layer_not_found");
    assert_eq!(
        f.err("set_frame_duration", json!({ "project": p, "frame": fr, "duration_ms": 0 })).code,
        "invalid_argument"
    );
    assert_eq!(f.err("project_create", json!({ "width": 0, "height": 4 })).code, "invalid_canvas_size");
    assert_eq!(f.err("batch", json!({ "project": p, "ops": [] })).code, "invalid_arguments");
    assert_eq!(f.err("batch", json!({ "project": p, "ops": [{ "op": "explode" }] })).code, "invalid_arguments");
    assert_eq!(f.err("no_such_tool", json!({})).code, "unknown_tool");
    f.ok("layer_update", json!({ "project": p, "layer": l, "locked": true }));
    let e = f.err("fill_region", base(json!({})));
    assert_eq!(e.code, "layer_locked");
}

#[test]
fn expected_revision_guards_edits() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    let line = |rev: u64| json!({ "project": p, "layer": l, "frame": fr, "from": { "x": 0, "y": 0 }, "to": { "x": 3, "y": 0 }, "color": "#000", "expected_revision": rev });
    f.ok("draw_line", line(0));
    let e = f.err("draw_line", line(0));
    assert_eq!(e.code, "revision_mismatch");
    assert_eq!(e.details, Some(json!({ "expected": 0, "actual": 1 })));
    f.ok("draw_line", line(1));
}

#[test]
fn batch_is_atomic_and_one_undo_step() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    let failing = json!({ "project": p, "ops": [
        { "op": "set_pixels", "layer": l, "frame": fr, "pixels": [{ "x": 0, "y": 0, "color": "#f00" }] },
        { "op": "draw_line", "layer": 999, "frame": fr, "from": { "x": 0, "y": 0 }, "to": { "x": 1, "y": 1 }, "color": "#f00" },
    ]});
    assert_eq!(f.err("batch", failing).code, "layer_not_found");
    assert_eq!(f.ok("project_get", json!({ "project": p }))["summary"]["revision"], 0);

    let r = f.ok(
        "batch",
        json!({ "project": p, "label": "Hero", "ops": [
            { "op": "layer_create", "name": "Outline" },
            { "op": "add_frame", "duration_ms": 120 },
            { "op": "fill_region", "layer": l, "frame": fr, "x": 0, "y": 0, "color": "#123456" },
            { "op": "tag_create", "name": "idle", "from": fr, "to": fr, "direction": "ping_pong", "loop": false },
        ]}),
    );
    assert_eq!(r["revision"], 1);
    assert_eq!(r["label"], "Hero");
    assert_eq!(r["created"].as_array().unwrap().len(), 2);
    let s = f.ok("project_get", json!({ "project": p }))["summary"].clone();
    assert_eq!(s["layers"].as_array().unwrap().len(), 2);
    assert_eq!(s["tags"][0]["direction"], "ping_pong");
    assert_eq!(s["undo_label"], "Hero");
    f.ok("undo", json!({ "project": p }));
    let s = f.ok("project_get", json!({ "project": p }))["summary"].clone();
    assert_eq!(s["layers"].as_array().unwrap().len(), 1);
    assert_eq!(s["frames"].as_array().unwrap().len(), 1);
}

#[test]
fn destructive_tools_need_confirm() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    let created = f.ok("layer_create", json!({ "project": p }))["created"][0]["layer"].as_u64().unwrap();
    f.ok("add_frame", json!({ "project": p }));
    assert_eq!(f.err("layer_delete", json!({ "project": p, "layer": created })).code, "confirmation_required");
    assert_eq!(
        f.err("layer_delete", json!({ "project": p, "layer": created, "confirm": false })).code,
        "confirmation_required"
    );
    f.ok("layer_delete", json!({ "project": p, "layer": created, "confirm": true }));
    assert_eq!(f.err("delete_frame", json!({ "project": p, "frame": fr })).code, "confirmation_required");
    f.ok("delete_frame", json!({ "project": p, "frame": fr, "confirm": true }));
    let ops = json!([{ "op": "layer_delete", "layer": l }]);
    assert_eq!(f.err("batch", json!({ "project": p, "ops": ops })).code, "confirmation_required");
    // The last layer can never be deleted.
    assert_eq!(f.err("batch", json!({ "project": p, "ops": ops, "confirm": true })).code, "last_layer");

    // Closing with unsaved changes.
    assert_eq!(f.err("project_close", json!({ "project": p })).code, "confirmation_required");
    f.ok("project_close", json!({ "project": p, "confirm": true }));
    assert_eq!(f.err("project_get", json!({ "project": p })).code, "project_not_found");
}

#[test]
fn saving_never_silently_overwrites_other_files() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    std::fs::write(f.root.path().join("other.dotify"), b"keep me").unwrap();
    f.ok(
        "set_pixels",
        json!({ "project": p, "layer": l, "frame": fr, "pixels": [{ "x": 0, "y": 0, "color": "#abc" }] }),
    );
    // Its own file: no confirmation.
    let r = f.ok("project_save", json!({ "project": p }));
    assert_eq!(r["revision"], 1);
    assert_eq!(f.ok("project_get", json!({ "project": p }))["summary"]["dirty"], false);
    let doc = format::load(&f.root.path().join("art.dotify")).unwrap();
    assert_eq!(doc.cels().count(), 1);

    let e = f.err("project_save", json!({ "project": p, "path": "other.dotify" }));
    assert_eq!(e.code, "confirmation_required");
    assert_eq!(std::fs::read(f.root.path().join("other.dotify")).unwrap(), b"keep me");
    f.ok("project_save", json!({ "project": p, "path": "other.dotify", "confirm": true }));
    assert!(format::load(&f.root.path().join("other.dotify")).is_ok());

    assert_eq!(
        f.err("project_create", json!({ "width": 2, "height": 2, "path": "art.dotify" })).code,
        "confirmation_required"
    );
    // In-memory projects need a path to save.
    let mem = f.ok("project_create", json!({ "width": 2, "height": 2 }))["project"].as_str().unwrap().to_string();
    assert_eq!(f.err("project_save", json!({ "project": mem })).code, "no_path");
    // Writing over a file another open project uses is refused even when confirmed.
    let e = f.err("project_save", json!({ "project": mem, "path": "other.dotify", "confirm": true }));
    assert_eq!(e.code, "project_open");
}

#[test]
fn file_access_is_limited_to_roots() {
    let f = fixture(false);
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.dotify");
    let doc = dotify_core::Document::new("s", 2, 2).unwrap();
    format::save(&doc, &secret, Default::default()).unwrap();
    let s = secret.to_str().unwrap();
    assert_eq!(f.err("project_open", json!({ "path": s })).code, "path_outside_roots");
    let name = outside.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(f.err("project_open", json!({ "path": format!("../{name}/secret.dotify") })).code, "path_outside_roots");
    let (p, _, _) = f.project();
    assert_eq!(f.err("project_save", json!({ "project": p, "path": s, "confirm": true })).code, "path_outside_roots");
    let out_png = outside.path().join("x.png");
    let e = f.err("export_png", json!({ "project": p, "path": out_png.to_str().unwrap(), "confirm": true }));
    assert_eq!(e.code, "path_outside_roots");
    let e = f.err("export_godot", json!({ "project": p, "dir": outside.path().to_str().unwrap(), "confirm": true }));
    assert_eq!(e.code, "path_outside_roots");
    assert_eq!(f.err("export_png", json!({ "project": p, "path": "x.jpg" })).code, "invalid_path");
    assert!(!out_png.exists());
    let list = f.ok("project_list", json!({}));
    assert_eq!(list["files"].as_array().unwrap().len(), 1);
    assert_eq!(list["open"][0]["project"], p);
}

#[test]
fn exports_write_inside_roots_and_ask_before_replacing() {
    let f = fixture(false);
    let (p, l, fr) = f.project();
    f.ok(
        "set_pixels",
        json!({ "project": p, "layer": l, "frame": fr, "pixels": [{ "x": 0, "y": 0, "color": "#f00" }] }),
    );
    f.ok("add_frame", json!({ "project": p }));
    f.ok("tag_create", json!({ "project": p, "name": "walk", "from": fr, "to": fr }));
    let r =
        f.ok("export_godot", json!({ "project": p, "dir": "godot/hero", "name": "hero", "sheet": { "padding": 1 } }));
    assert_eq!(r["files"].as_array().unwrap().len(), 3);
    let tres = std::fs::read_to_string(f.root.path().join("godot/hero/hero.tres")).unwrap();
    assert!(tres.contains("\"walk\""), "{tres}");
    let e = f.err("export_godot", json!({ "project": p, "dir": "godot/hero", "name": "hero" }));
    assert_eq!(e.code, "confirmation_required");
    assert_eq!(e.details.unwrap()["existing"].as_array().unwrap().len(), 3);
    f.ok("export_godot", json!({ "project": p, "dir": "godot/hero", "name": "hero", "confirm": true }));

    f.ok("export_png", json!({ "project": p, "path": "out/a.png", "scale": 2 }));
    f.ok("export_gif", json!({ "project": p, "path": "out/a.gif", "tag": "walk" }));
    let r = f.ok(
        "export_spritesheet",
        json!({ "project": p, "path": "out/sheet.png", "sheet": { "layout": "rows_per_tag" } }),
    );
    assert!(f.root.path().join("out/sheet.json").exists(), "{r}");
    assert_eq!(f.err("export_png", json!({ "project": p, "path": "out/a.png" })).code, "confirmation_required");
}

#[test]
fn read_only_mode_offers_only_reading_tools() {
    let f = fixture(true);
    let names: Vec<&str> = f.dotify.tools().map(|t| t.name).collect();
    assert!(names.contains(&"project_open") && names.contains(&"get_canvas"));
    assert!(!names.contains(&"draw_line") && !names.contains(&"project_save") && !names.contains(&"export_png"));
    let doc = dotify_core::Document::new("r", 2, 2).unwrap();
    format::save(&doc, &f.root.path().join("r.dotify"), Default::default()).unwrap();
    let p = f.ok("project_open", json!({ "path": "r.dotify" }))["project"].as_str().unwrap().to_string();
    let again = f.ok("project_open", json!({ "path": "r.dotify" }));
    assert_eq!(again["project"], p);
    assert_eq!(again["already_open"], true);
    f.ok("layer_list", json!({ "project": p }));
    let e = f.err("draw_line", json!({ "project": p }));
    assert_eq!(e.code, "read_only");
    assert_eq!(f.err("project_create", json!({ "width": 2, "height": 2 })).code, "read_only");
}
