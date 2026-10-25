use std::process::Command;

fn dotify(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_dotify")).args(args).output().unwrap();
    assert!(out.status.success(), "dotify {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn new_apply_info_export() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("hero.dotify");
    let p = project.to_str().unwrap();
    dotify(&["new", p, "--width", "8", "--height", "6"]);

    let info: serde_json::Value = serde_json::from_str(&dotify(&["info", p])).unwrap();
    assert_eq!(info["artboards"][0]["width"], 8);
    let layer = info["artboards"][0]["layers"][0]["id"].as_u64().unwrap();
    let frame = info["artboards"][0]["frames"][0]["id"].as_u64().unwrap();

    let cmds = dir.path().join("cmds.json");
    std::fs::write(
        &cmds,
        serde_json::json!([
            { "type": "draw_rectangle", "layer": layer, "frame": frame, "x": 0, "y": 0,
              "width": 8, "height": 6, "color": "#ff0000", "filled": true },
            { "type": "create_layer", "name": "Ink" }
        ])
        .to_string(),
    )
    .unwrap();
    let outcome: serde_json::Value = serde_json::from_str(&dotify(&["apply", p, cmds.to_str().unwrap()])).unwrap();
    assert_eq!(outcome["changed"], true);

    let info: serde_json::Value = serde_json::from_str(&dotify(&["info", p])).unwrap();
    assert_eq!(info["artboards"][0]["layers"][1]["name"], "Ink");

    let png = dir.path().join("out.png");
    dotify(&["export-png", p, png.to_str().unwrap(), "--scale", "2"]);
    let img = dotify_core::png_io::decode_rgba(&std::fs::read(&png).unwrap(), 64).unwrap();
    assert_eq!((img.width(), img.height()), (16, 12));
    assert_eq!(img.get(15, 11), dotify_core::Rgba::opaque(255, 0, 0));
}

#[test]
fn failed_apply_leaves_file_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("a.dotify");
    let p = project.to_str().unwrap();
    dotify(&["new", p]);
    let before = std::fs::read(&project).unwrap();
    let cmds = dir.path().join("bad.json");
    std::fs::write(&cmds, r#"[{"type":"create_layer"},{"type":"delete_layer","layer":999}]"#).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_dotify")).args(["apply", p, cmds.to_str().unwrap()]).output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("layer_not_found"));
    assert_eq!(std::fs::read(&project).unwrap(), before);
}

#[test]
fn library_commands_and_export_all() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("game.dotify");
    let p = project.to_str().unwrap();
    dotify(&["new", p, "--width", "4", "--height", "4", "--name", "Hero"]);
    let cmds = dir.path().join("library.json");
    std::fs::write(
        &cmds,
        serde_json::json!([
            { "type": "create_folder", "name": "Tiles" },
            { "type": "create_artboard", "name": "Grass", "width": 2, "height": 2, "folder": 2 },
            { "type": "edit", "artboard": 3, "command":
              { "type": "fill_region", "layer": 1, "frame": 2, "x": 0, "y": 0, "color": "#00ff00" } }
        ])
        .to_string(),
    )
    .unwrap();
    dotify(&["apply", p, cmds.to_str().unwrap()]);
    let info: serde_json::Value = serde_json::from_str(&dotify(&["info", p])).unwrap();
    assert_eq!(info["folders"][0]["name"], "Tiles");
    assert_eq!(info["artboards"][1]["folder"], 2);

    let out = dir.path().join("out");
    let listed = dotify(&["export-all", p, out.to_str().unwrap()]);
    assert_eq!(listed.lines().count(), 2);
    let grass = dotify_core::png_io::decode_rgba(&std::fs::read(out.join("Tiles/Grass.png")).unwrap(), 64).unwrap();
    assert_eq!(grass.get(1, 1), dotify_core::Rgba::opaque(0, 255, 0));
    assert!(out.join("Hero.png").exists());

    let godot = dir.path().join("godot");
    let listed = dotify(&["export-all", p, godot.to_str().unwrap(), "--godot"]);
    assert_eq!(listed.lines().count(), 6);
    assert!(godot.join("Tiles/Grass.tres").exists());
}

#[test]
fn tags_and_godot_gif_sheet_exports() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("knight.dotify");
    let p = project.to_str().unwrap();
    dotify(&["new", p, "--width", "4", "--height", "4"]);
    let info: serde_json::Value = serde_json::from_str(&dotify(&["info", p])).unwrap();
    let board = &info["artboards"][0];
    let (layer, f1) = (board["layers"][0]["id"].as_u64().unwrap(), board["frames"][0]["id"].as_u64().unwrap());

    let cmds = dir.path().join("anim.json");
    std::fs::write(
        &cmds,
        serde_json::json!([
            { "type": "set_pixels", "layer": layer, "frame": f1, "pixels": [{ "x": 0, "y": 0, "color": "#ff0000" }] },
            { "type": "add_frame", "duration_ms": 200 },
            { "type": "create_tag", "name": "idle", "from": f1, "to": f1 },
        ])
        .to_string(),
    )
    .unwrap();
    dotify(&["apply", p, cmds.to_str().unwrap()]);
    let info: serde_json::Value = serde_json::from_str(&dotify(&["info", p])).unwrap();
    let f2 = info["artboards"][0]["frames"][1]["id"].as_u64().unwrap();
    std::fs::write(
        &cmds,
        serde_json::json!({ "type": "update_tag", "tag": "idle", "to": f2, "loop": false }).to_string(),
    )
    .unwrap();
    dotify(&["apply", p, cmds.to_str().unwrap()]);
    let info: serde_json::Value = serde_json::from_str(&dotify(&["info", p])).unwrap();
    assert_eq!(info["artboards"][0]["tags"][0]["loop"], false);

    let out = dir.path().join("godot");
    let printed = dotify(&["export-godot", p, out.to_str().unwrap(), "--padding", "1", "--scale", "2"]);
    assert_eq!(printed.lines().count(), 3);
    let tres = std::fs::read_to_string(out.join("knight.tres")).unwrap();
    assert!(tres.contains("path=\"knight.png\""));
    assert!(tres.contains("\"loop\": false,\n\"name\": &\"idle\",\n\"speed\": 10.0"));
    let png = dotify_core::png_io::decode_rgba(&std::fs::read(out.join("knight.png")).unwrap(), 64).unwrap();
    // 2×1 grid of 8×8 frames with 1 px padding.
    assert_eq!((png.width(), png.height()), (19, 10));
    assert_eq!(png.get(1, 1), dotify_core::Rgba::opaque(255, 0, 0));

    let gif = dir.path().join("idle.gif");
    dotify(&["export-gif", p, gif.to_str().unwrap(), "--tag", "idle"]);
    assert!(std::fs::read(&gif).unwrap().starts_with(b"GIF89a"));

    let sheet = dir.path().join("sheet.png");
    dotify(&["export-sheet", p, sheet.to_str().unwrap(), "--layout", "rows", "--pivot", "2,4"]);
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("sheet.json")).unwrap()).unwrap();
    assert_eq!(json["pivot"], serde_json::json!({ "x": 2.0, "y": 4.0 }));
    assert_eq!(json["tags"][0]["sequence"], serde_json::json!([0, 1]));
}
