//! The `seam` binary, run as a user would run it.

// Test code: a failed unwrap is the test failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;
use std::process::{Command, Output};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A private copy of the fixture, so scans can write their store.
fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/polyglot"),
        dir.path(),
    );
    dir
}

fn seam(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_seam"))
        .args(args)
        .current_dir(dir)
        .env_remove("SEAM_LANG")
        .env("LANG", "C")
        .env("NO_COLOR", "1")
        .output()
        .expect("the binary runs")
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

#[test]
fn scan_records_and_reports_the_fixture() {
    let dir = fixture();
    let out = seam(dir.path(), &["scan"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = stdout(&out);
    assert!(
        text.contains("GET /accounts/{account_id}  VERIFIED"),
        "{text}"
    );
    assert!(text.contains("The consumer reads `name`"), "{text}");
    assert!(
        text.contains("Classes: 3 VERIFIED, 1 PARSED, 1 UNKNOWN"),
        "{text}"
    );
    assert!(dir.path().join(".seam/seam.db").is_file());
    let again = stdout(&seam(dir.path(), &["scan"]));
    assert!(again.contains("reconfirmed"), "{again}");
}

#[test]
fn fail_on_turns_findings_into_an_exit_status() {
    let dir = fixture();
    assert_eq!(
        seam(dir.path(), &["scan", "--no-store", "--fail-on", "breaking"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        seam(dir.path(), &["scan", "--no-store", "--fail-on", "never"])
            .status
            .code(),
        Some(0)
    );
    assert!(
        !dir.path().join(".seam").exists(),
        "--no-store wrote a store"
    );
}

#[test]
fn explain_shows_the_evidence_and_notices_edits() {
    let dir = fixture();
    seam(dir.path(), &["scan"]);
    let id = "sm_hajbrsw7";
    let text = stdout(&seam(dir.path(), &["explain", id]));
    assert!(text.contains("Class       VERIFIED"), "{text}");
    assert!(text.contains("web/src/api/accounts.ts:5"), "{text}");
    assert!(text.contains("docker-compose.yml:5"), "{text}");
    std::fs::write(
        dir.path().join("web/src/api/types.ts"),
        "export interface Account { id: number }\n",
    )
    .unwrap();
    let stale = stdout(&seam(dir.path(), &["explain", id]));
    assert!(stale.contains("STALE (was VERIFIED)"), "{stale}");
    assert_eq!(
        seam(dir.path(), &["explain", "sm_zzzzzzzz"]).status.code(),
        Some(5)
    );
}

#[test]
fn persian_output_covers_labels_and_sentences() {
    let dir = fixture();
    let text = stdout(&seam(dir.path(), &["scan", "--lang", "fa", "--no-store"]));
    assert!(text.contains("تأییدشده (VERIFIED)"), "{text}");
    assert!(text.contains("مصرف‌کننده `name` را می‌خواند"), "{text}");
    let env = Command::new(env!("CARGO_BIN_EXE_seam"))
        .args(["languages"])
        .env("SEAM_LANG", "fa")
        .output()
        .unwrap();
    assert!(stdout(&env).contains("خوانده می‌شود"));
}

#[test]
fn reports_are_json_or_markdown() {
    let dir = fixture();
    let json: serde_json::Value =
        serde_json::from_str(&stdout(&seam(dir.path(), &["report"]))).unwrap();
    assert_eq!(json["schema"], "seam-report/1");
    assert_eq!(json["summary"]["seams"], 5);
    assert!(!dir.path().join(".seam").exists(), "report must not record");
    let md = seam(
        dir.path(),
        &[
            "report", "--format", "markdown", "--lang", "fa", "--output", "r.md",
        ],
    );
    assert_eq!(md.status.code(), Some(0));
    let text = std::fs::read_to_string(dir.path().join("r.md")).unwrap();
    assert!(text.starts_with("<div dir=\"rtl\" lang=\"fa\">"));
    assert!(text.contains("<pre dir=\"ltr\">"));
}

#[test]
fn errors_have_distinct_statuses() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        seam(dir.path(), &["scan", "does-not-exist"]).status.code(),
        Some(3)
    );
    assert_eq!(seam(dir.path(), &["seams"]).status.code(), Some(5));
    assert_eq!(
        seam(dir.path(), &["no-such-command"]).status.code(),
        Some(2)
    );
}

#[test]
fn scanning_changes_nothing_but_the_store() {
    let dir = fixture();
    let before: Vec<_> = walk(dir.path());
    seam(dir.path(), &["scan"]);
    let after: Vec<_> = walk(dir.path())
        .into_iter()
        .filter(|(p, _)| !p.starts_with(".seam"))
        .collect();
    assert_eq!(before, after);
}

fn walk(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((rel, std::fs::read(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}
