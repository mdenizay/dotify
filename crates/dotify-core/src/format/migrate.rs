//! `document.json` migrations. Each step upgrades the JSON by exactly one
//! format version. Add a step (and a fixture test) for every format change.

use serde_json::{json, Value};

use super::{FormatError, FORMAT_VERSION};

pub(crate) fn migrate(mut doc: Value, from_version: u32) -> Result<Value, FormatError> {
    if from_version == 0 || from_version > FORMAT_VERSION {
        return Err(FormatError::UnsupportedVersion { required: from_version, supported: FORMAT_VERSION });
    }
    // Steps run in order, each only for files older than its target.
    if from_version < 2 {
        doc = v1_to_v2(doc);
    }
    if from_version < 3 {
        doc = v2_to_v3(doc)?;
    }
    Ok(doc)
}

/// v2 adds `loop` to animation tags. Version 1 had no way to express a
/// one-shot animation, so every existing tag loops.
fn v1_to_v2(mut doc: Value) -> Value {
    if let Some(tags) = doc.get_mut("tags").and_then(Value::as_array_mut) {
        for tag in tags.iter_mut().filter_map(Value::as_object_mut) {
            tag.entry("loop").or_insert(Value::Bool(true));
        }
    }
    doc
}

/// v2 stored one document; v3 stores a project of artboards. The document
/// becomes artboard 1 (no folder) and its palette becomes the project
/// palette. Everything else, tags included, moves into the artboard as is.
/// Cel `file` paths are kept as written, so the old entries still resolve.
fn v2_to_v3(doc: Value) -> Result<Value, FormatError> {
    let Value::Object(mut artboard) = doc else {
        return Err(FormatError::Corrupt("document.json: expected an object".into()));
    };
    let palette = artboard.remove("palette").unwrap_or(Value::Null);
    let name = artboard.get("name").cloned().unwrap_or(Value::Null);
    artboard.insert("id".into(), json!(1));
    artboard.insert("folder".into(), Value::Null);
    Ok(json!({
        "name": name,
        "palette": palette,
        "folders": [],
        "artboards": [artboard],
        "next_id": 2,
        "extensions": {},
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn current_version_is_identity() {
        let v = serde_json::json!({ "name": "x" });
        assert_eq!(migrate(v.clone(), FORMAT_VERSION).unwrap(), v);
    }

    #[test]
    fn v1_tags_gain_loop() {
        let v1 = serde_json::json!({ "tags": [
            { "name": "Idle", "from": 1, "to": 2, "direction": "forward", "color": "#ffffffff" }
        ] });
        let v2 = v1_to_v2(v1);
        assert_eq!(v2["tags"][0]["loop"], true);
        assert_eq!(v2["tags"][0]["name"], "Idle");
    }

    #[test]
    fn rejects_unknown_versions() {
        assert!(migrate(Value::Null, 0).is_err());
        assert!(migrate(Value::Null, FORMAT_VERSION + 1).is_err());
    }

    /// A v1 file passes through both steps: tags gain `loop`, then the
    /// document becomes a one-artboard project.
    #[test]
    fn v1_document_becomes_a_one_artboard_project_with_looping_tags() {
        let migrated = migrate(fixture("v1-document.json"), 1).unwrap();
        assert_eq!(migrated, fixture("v3-from-v1-document.json"));
        assert_eq!(migrated["artboards"][0]["tags"][0]["loop"], true);
    }

    /// A v2 file only gets wrapped; its `loop` values are kept as written.
    #[test]
    fn v2_document_becomes_a_one_artboard_project() {
        let migrated = migrate(fixture("v2-document.json"), 2).unwrap();
        assert_eq!(migrated, fixture("v3-from-v2-document.json"));
        assert_eq!(migrated["artboards"][0]["tags"][1]["loop"], false);
    }

    #[test]
    fn v2_to_v3_rejects_non_objects() {
        assert!(matches!(migrate(json!([1, 2]), 2), Err(FormatError::Corrupt(_))));
    }
}
