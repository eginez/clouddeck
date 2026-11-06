use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn dependency_key(line: &str) -> Option<&str> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
        return None;
    }
    let (key, _) = line.split_once('=')?;
    Some(key.trim().trim_matches('"'))
}

#[test]
fn core_and_app_manifests_have_no_aws_sdk_deps() {
    let root = workspace_root();
    for manifest in ["crates/clouddeck-core/Cargo.toml", "crates/clouddeck-app/Cargo.toml"] {
        let path = root.join(manifest);
        let text = std::fs::read_to_string(&path).expect("read manifest");
        for line in text.lines() {
            let Some(key) = dependency_key(line) else {
                continue;
            };
            assert!(
                !key.starts_with("aws-") && !key.starts_with("aws_") && !key.contains("smithy"),
                "{} must not depend on the AWS SDK; offending line: {}",
                path.display(),
                line.trim()
            );
        }
    }
}
