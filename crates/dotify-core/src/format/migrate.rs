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
        doc = v1_to_v2(doc)?;
    }
    Ok(doc)
}

/// v1 stored one document; v2 stores a project of artboards. The document
/// becomes artboard 1 (no folder) and its palette becomes the project
/// palette. Cel `file` paths are kept as written, so the v1 entries still
/// resolve.
fn v1_to_v2(doc: Value) -> Result<Value, FormatError> {
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
    fn rejects_unknown_versions() {
        assert!(migrate(Value::Null, 0).is_err());
        assert!(migrate(Value::Null, FORMAT_VERSION + 1).is_err());
    }

    #[test]
    fn v1_document_becomes_a_one_artboard_project() {
        let migrated = migrate(fixture("v1-document.json"), 1).unwrap();
        assert_eq!(migrated, fixture("v2-from-v1-document.json"));
    }

    #[test]
    fn v1_to_v2_rejects_non_objects() {
        assert!(matches!(migrate(json!([1, 2]), 1), Err(FormatError::Corrupt(_))));
    }
}
