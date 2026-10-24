//! `document.json` migrations. Each step upgrades the JSON by exactly one
//! format version. Add a step (and a fixture test) for every format change.

use serde_json::Value;

use super::{FormatError, FORMAT_VERSION};

pub(crate) fn migrate(mut doc: Value, from_version: u32) -> Result<Value, FormatError> {
    if from_version == 0 || from_version > FORMAT_VERSION {
        return Err(FormatError::UnsupportedVersion { required: from_version, supported: FORMAT_VERSION });
    }
    // Steps run in order, each only for files older than its target.
    if from_version < 2 {
        doc = v1_to_v2(doc);
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let v2 = migrate(v1, 1).unwrap();
        assert_eq!(v2["tags"][0]["loop"], true);
        assert_eq!(v2["tags"][0]["name"], "Idle");
    }

    #[test]
    fn rejects_unknown_versions() {
        assert!(migrate(Value::Null, 0).is_err());
        assert!(migrate(Value::Null, FORMAT_VERSION + 1).is_err());
    }
}
