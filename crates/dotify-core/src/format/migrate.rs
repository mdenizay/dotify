//! `document.json` migrations. Each step upgrades the JSON by exactly one
//! format version. Add a step (and a fixture test) for every format change.

use serde_json::Value;

use super::{FormatError, FORMAT_VERSION};

pub(crate) fn migrate(doc: Value, from_version: u32) -> Result<Value, FormatError> {
    if from_version == 0 || from_version > FORMAT_VERSION {
        return Err(FormatError::UnsupportedVersion { required: from_version, supported: FORMAT_VERSION });
    }
    // Steps run in order, each only for files older than its target, e.g.:
    // if from_version < 2 { doc = v1_to_v2(doc)?; }
    Ok(doc)
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
    fn rejects_unknown_versions() {
        assert!(migrate(Value::Null, 0).is_err());
        assert!(migrate(Value::Null, FORMAT_VERSION + 1).is_err());
    }
}
