//! End-to-end MCP session against the real binary over stdio.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use base64::Engine;
use serde_json::{json, Value};

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl Session {
    fn start(root: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dotify-mcp"))
            .arg("--root")
            .arg(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self { child, stdin, stdout, next_id: 1 }
    }

    fn send(&mut self, msg: Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(self.stdout.read_line(&mut line).unwrap() > 0, "server closed stdout");
            let msg: Value = serde_json::from_str(&line).unwrap();
            if msg["id"] == id {
                return msg;
            }
        }
    }

    fn call(&mut self, name: &str, arguments: Value) -> Value {
        let r = self.request("tools/call", json!({ "name": name, "arguments": arguments }));
        r["result"].clone()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn stdio_session() {
    let root = tempfile::tempdir().unwrap();
    let mut s = Session::start(root.path());

    let init = s.request(
        "initialize",
        json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": "test", "version": "1" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "dotify-mcp");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    s.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));

    let tools = s.request("tools/list", json!({}));
    let tools = tools["result"]["tools"].as_array().unwrap();
    for t in tools {
        assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
    }
    let delete = tools.iter().find(|t| t["name"] == "layer_delete").unwrap();
    assert_eq!(delete["annotations"]["destructiveHint"], true);
    assert!(delete["inputSchema"]["required"].as_array().unwrap().contains(&json!("confirm")));

    let created = s.call("project_create", json!({ "width": 8, "height": 8, "name": "Hero", "path": "hero.dotify" }));
    assert_eq!(created["isError"], false);
    let summary = &created["structuredContent"]["summary"];
    let p = created["structuredContent"]["project"].as_str().unwrap().to_string();
    let (layer, frame) = (summary["layers"][0]["id"].clone(), summary["frames"][0]["id"].clone());

    let drawn = s.call(
        "draw_rectangle",
        json!({ "project": p, "layer": layer, "frame": frame, "x": 2, "y": 2, "width": 4, "height": 4,
                "color": "#3366ffff", "filled": true, "expected_revision": 0 }),
    );
    assert_eq!(drawn["structuredContent"]["revision"], 1, "{drawn}");

    let stale = s.call(
        "draw_line",
        json!({ "project": p, "layer": layer, "frame": frame, "from": { "x": 0, "y": 0 }, "to": { "x": 7, "y": 0 },
                "color": "#000", "expected_revision": 0 }),
    );
    assert_eq!(stale["isError"], true);
    assert_eq!(stale["structuredContent"]["error"]["code"], "revision_mismatch");

    let canvas = s.call("get_canvas", json!({ "project": p, "scale": 1 }));
    let image = canvas["content"].as_array().unwrap().iter().find(|c| c["type"] == "image").unwrap();
    assert_eq!(image["mimeType"], "image/png");
    let png = base64::engine::general_purpose::STANDARD.decode(image["data"].as_str().unwrap()).unwrap();
    let img = dotify_core::png_io::decode_rgba(&png, 64).unwrap();
    assert_eq!(img.get(3, 3), dotify_core::Rgba::opaque(0x33, 0x66, 0xff));
    assert_eq!(img.get(0, 0).a, 0);

    let saved = s.call("project_save", json!({ "project": p }));
    assert_eq!(saved["isError"], false, "{saved}");
    let doc = dotify_core::format::load(&root.path().join("hero.dotify")).unwrap();
    assert_eq!(doc.name(), "Hero");
    assert_eq!(doc.cels().count(), 1);

    let outside = s.call("project_open", json!({ "path": "/etc/hosts" }));
    assert_eq!(outside["structuredContent"]["error"]["code"], "path_outside_roots");

    let unknown = s.request("tools/call", json!({ "name": "rm_rf", "arguments": {} }));
    assert!(unknown["error"].is_object(), "{unknown}");
}
