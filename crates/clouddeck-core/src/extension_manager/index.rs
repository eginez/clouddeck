use std::collections::BTreeSet;

use serde::Deserialize;

use crate::extension_host::is_valid_id;
use crate::release::verify::verify_manifest;

pub const INDEX_SCHEMA: &str = "clouddeck-extension-index/1";
pub const INDEX_URL: &str =
    "https://raw.githubusercontent.com/eginez/clouddeck-extensions/main/index.json";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    pub id: String,
    pub repository: String,
    pub author_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Index {
    pub schema: String,
    pub extensions: Vec<IndexEntry>,
}

impl Index {
    pub fn find(&self, id: &str) -> Option<&IndexEntry> {
        self.extensions.iter().find(|entry| entry.id == id)
    }
}

/// `Unavailable` means the index could not be fetched; `Invalid` means it was fetched and refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexError {
    Unavailable(String),
    Invalid(String),
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(reason) => write!(f, "the extension index is unavailable: {reason}"),
            Self::Invalid(reason) => write!(f, "the extension index was refused: {reason}"),
        }
    }
}

/// Parses an index only after its signature checks out against the release key.
pub fn verify_index(bytes: &[u8], signature_b64: &str, release_key: &str) -> Result<Index, String> {
    verify_manifest(bytes, signature_b64, release_key)
        .map_err(|error| format!("index.json signature: {error}"))?;
    let index: Index =
        serde_json::from_slice(bytes).map_err(|error| format!("invalid index.json: {error}"))?;
    if index.schema != INDEX_SCHEMA {
        return Err(format!(
            "index.json uses schema {:?}; this CloudDeck reads {INDEX_SCHEMA}",
            index.schema
        ));
    }
    let mut seen = BTreeSet::new();
    for entry in &index.extensions {
        if !is_valid_id(&entry.id) {
            return Err(format!(
                "index.json lists an invalid extension id {:?}",
                entry.id
            ));
        }
        if !seen.insert(entry.id.as_str()) {
            return Err(format!("index.json lists extension {} twice", entry.id));
        }
    }
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::release::sign::{generate_keypair, sign_bytes};

    fn signed(json: &str) -> (Vec<u8>, String, String) {
        let (secret, public) = generate_keypair();
        let sig = sign_bytes(json.as_bytes(), &secret).unwrap();
        (json.as_bytes().to_vec(), sig, public)
    }

    const GOOD: &str = r#"{"schema":"clouddeck-extension-index/1","extensions":[{"id":"toy","repository":"https://github.com/eginez/clouddeck-toy","author_key":"AAAA"}]}"#;

    #[test]
    fn correctly_signed_index_parses() {
        let (bytes, sig, public) = signed(GOOD);
        let index = verify_index(&bytes, &sig, &public).unwrap();
        assert_eq!(index.find("toy").unwrap().author_key, "AAAA");
        assert!(index.find("other").is_none());
    }

    #[test]
    fn wrong_signature_is_refused() {
        let (bytes, _, public) = signed(GOOD);
        let (_, other_sig, _) = signed(GOOD);
        assert!(verify_index(&bytes, &other_sig, &public).is_err());
    }

    #[test]
    fn wrong_schema_is_refused() {
        let json = GOOD.replace("/1\"", "/2\"");
        let (bytes, sig, public) = signed(&json);
        assert!(
            verify_index(&bytes, &sig, &public)
                .unwrap_err()
                .contains("schema")
        );
    }

    #[test]
    fn duplicate_id_is_refused() {
        let entry = r#"{"id":"toy","repository":"https://github.com/a/b","author_key":"AAAA"}"#;
        let json =
            format!(r#"{{"schema":"clouddeck-extension-index/1","extensions":[{entry},{entry}]}}"#);
        let (bytes, sig, public) = signed(&json);
        assert!(
            verify_index(&bytes, &sig, &public)
                .unwrap_err()
                .contains("twice")
        );
    }

    #[test]
    fn unknown_fields_are_refused() {
        let json = r#"{"schema":"clouddeck-extension-index/1","extensions":[],"extra":1}"#;
        let (bytes, sig, public) = signed(json);
        assert!(verify_index(&bytes, &sig, &public).is_err());
    }
}
