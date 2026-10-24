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
    assert_eq!(info["width"], 8);
    let layer = info["layers"][0]["id"].as_u64().unwrap();
    let frame = info["frames"][0]["id"].as_u64().unwrap();

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
    assert_eq!(info["layers"][1]["name"], "Ink");

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
