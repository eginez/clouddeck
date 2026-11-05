use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn is_reference_name(name: &str) -> bool {
    name.starts_with("reference-")
}

fn collect_reference_paths(root: &Path, dir: &Path, found: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    for entry in entries {
        let entry = entry.expect("directory entry");
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name == "target" || name == ".git" {
            continue;
        }
        let path = entry.path();
        if is_reference_name(&name) {
            found.push(path.strip_prefix(root).unwrap_or(&path).to_path_buf());
        }
        let file_type = entry.file_type().expect("file type");
        if file_type.is_dir() {
            collect_reference_paths(root, &path, found);
        }
    }
}

#[test]
fn no_reference_files_in_product_tree() {
    let root = workspace_root();
    let mut found = Vec::new();
    collect_reference_paths(&root, &root, &mut found);
    let listing: Vec<String> = found.iter().map(|p| p.display().to_string()).collect();
    assert!(
        found.is_empty(),
        "third-party reference screenshots found in the product tree:\n  {}\n\
         Third-party screenshots belong on the ops side, not in the product tree.",
        listing.join("\n  ")
    );
}

#[test]
fn design_readmes_do_not_link_reference_images() {
    let root = workspace_root();
    for readme in ["design/README.md", "design/images/README.md"] {
        let text = fs::read_to_string(root.join(readme))
            .unwrap_or_else(|e| panic!("read {readme}: {e}"));
        assert!(
            !text.contains("reference-"),
            "{readme} mentions a reference- image; remove the link, third-party screenshots belong on the ops side"
        );
    }
}
