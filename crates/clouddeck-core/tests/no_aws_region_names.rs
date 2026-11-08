use std::fs;
use std::path::{Path, PathBuf};

const AWS_REGIONS: &[&str] = &[
    "af-south-1",
    "ap-east-1",
    "ap-northeast-1",
    "ap-northeast-2",
    "ap-northeast-3",
    "ap-south-1",
    "ap-southeast-1",
    "ap-southeast-2",
    "ca-central-1",
    "eu-central-1",
    "eu-north-1",
    "eu-south-1",
    "eu-west-1",
    "eu-west-2",
    "eu-west-3",
    "me-south-1",
    "sa-east-1",
    "us-east-1",
    "us-east-2",
    "us-west-1",
    "us-west-2",
];

const ALLOWED_FILES: &[&str] = &["fixture_fleet.rs"];
const ALLOWED_LINE_MARKERS: &[(&str, &str)] =
    &[("scenario_service.rs", "MULTI_REGION_FAILING_REGION")];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn brace_delta(line: &str) -> i32 {
    line.chars().fold(0, |depth, c| match c {
        '{' => depth + 1,
        '}' => depth - 1,
        _ => depth,
    })
}

fn production_lines(text: &str) -> Vec<(usize, &str)> {
    let mut scanner = TestItemScanner::default();
    text.lines()
        .enumerate()
        .filter(|(_, line)| !scanner.consumes(line))
        .map(|(index, line)| (index + 1, line))
        .collect()
}

#[derive(Default)]
struct TestItemScanner {
    pending_cfg_test: bool,
    skipped_depth: Option<i32>,
}

impl TestItemScanner {
    fn consumes(&mut self, line: &str) -> bool {
        if let Some(depth) = self.skipped_depth.as_mut() {
            *depth += brace_delta(line);
            if *depth <= 0 {
                self.skipped_depth = None;
            }
            return true;
        }
        if line.trim_start().starts_with("#[cfg(test)]") {
            self.pending_cfg_test = true;
            return true;
        }
        if self.pending_cfg_test {
            self.start_item(line);
            return true;
        }
        false
    }

    fn start_item(&mut self, line: &str) {
        if line.contains('{') {
            self.skipped_depth = Some(brace_delta(line));
            self.pending_cfg_test = false;
        } else if line.trim_end().ends_with(';') {
            self.pending_cfg_test = false;
        }
    }
}

fn is_allowed_file(name: &str) -> bool {
    name.ends_with("_tests.rs") || ALLOWED_FILES.contains(&name)
}

#[test]
fn non_test_core_code_names_no_aws_regions() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    files.sort();

    let mut violations = Vec::new();
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if is_allowed_file(&name) {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        for (line_number, line) in production_lines(&text) {
            let allowed_marker = ALLOWED_LINE_MARKERS
                .iter()
                .any(|(file, marker)| *file == name && line.contains(marker));
            if allowed_marker {
                continue;
            }
            if let Some(region) = AWS_REGIONS.iter().find(|region| line.contains(*region)) {
                violations.push(format!(
                    "{}:{line_number} names {region}: {}",
                    path.strip_prefix(&src).unwrap().display(),
                    line.trim()
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "non-test core code must not name AWS regions:\n{}",
        violations.join("\n")
    );
}
