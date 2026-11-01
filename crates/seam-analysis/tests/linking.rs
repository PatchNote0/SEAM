//! Linking rules on small repositories written for each case.

// Test code: a failed unwrap is the test failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::fs;
use std::path::Path;

use seam_analysis::{CancelToken, Options, analyse};
use seam_core::{Classification, ConfidenceLevel};

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    fs::create_dir_all(full.parent().unwrap()).unwrap();
    fs::write(full, text).unwrap();
}

const API: &str = "from fastapi import FastAPI\n\napp = FastAPI()\n\n@app.get(\"/items/{item_id}\")\ndef get_item(item_id: int):\n    return {}\n";

fn repo(client: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "api/pyproject.toml",
        "[project]\nname = \"shop-api\"\n",
    );
    write(dir.path(), "api/main.py", API);
    write(dir.path(), "web/package.json", "{\"name\": \"shop-web\"}\n");
    write(dir.path(), "web/src/client.ts", client);
    dir
}

fn only_seam(dir: &Path) -> seam_core::Seam {
    let a = analyse(dir, &Options::default(), &CancelToken::default()).unwrap();
    assert_eq!(
        a.seams.len(),
        1,
        "{:?}",
        a.seams.iter().map(|s| &s.key).collect::<Vec<_>>()
    );
    a.seams.into_iter().next().unwrap()
}

#[test]
fn an_environment_value_missing_from_the_repository_is_an_assumption() {
    let dir = repo(
        "const BASE = process.env.API_URL || \"http://localhost:8000\";\nexport async function item(id: number) {\n  return fetch(`${BASE}/items/${id}`);\n}\n",
    );
    let s = only_seam(dir.path());
    assert_eq!(s.classification, Classification::Inferred);
    assert_eq!(s.confidence.level, ConfidenceLevel::Moderate);
    assert_eq!(s.assumptions.len(), 1);
    assert!(
        s.assumptions[0].statement.contains("API_URL"),
        "{}",
        s.assumptions[0].statement
    );
    assert!(s.assumptions[0].evidence.is_some());
}

#[test]
fn compose_turns_the_same_call_into_a_verified_seam() {
    let dir = repo(
        "const BASE = process.env.API_URL || \"http://localhost:8000\";\nexport async function item(id: number) {\n  return fetch(`${BASE}/items/${id}`);\n}\n",
    );
    write(
        dir.path(),
        "compose.yaml",
        "services:\n  web:\n    build: ./web\n    environment:\n      API_URL: http://shop:8000\n  shop:\n    build: ./api\n",
    );
    let s = only_seam(dir.path());
    assert_eq!(s.classification, Classification::Verified);
    assert!(s.assumptions.is_empty());
}

#[test]
fn a_relative_url_is_matched_with_an_assumption() {
    let s = only_seam(repo("export const item = (id: number) => fetch(`/items/${id}`);\n").path());
    assert_eq!(s.classification, Classification::Inferred);
    assert!(s.assumptions[0].statement.contains("origin"));
}

#[test]
fn a_router_no_app_includes_is_served_only_by_assumption() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "api/pyproject.toml",
        "[project]\nname = \"shop-api\"\n",
    );
    write(
        dir.path(),
        "api/routes.py",
        "from fastapi import APIRouter\n\nrouter = APIRouter(prefix=\"/items\")\n\n@router.get(\"/{item_id}\")\ndef get_item(item_id: int):\n    return {}\n",
    );
    write(dir.path(), "web/package.json", "{\"name\": \"shop-web\"}\n");
    write(
        dir.path(),
        "web/src/client.ts",
        "export const item = (id: number) => fetch(`http://localhost/items/${id}`);\n",
    );
    let s = only_seam(dir.path());
    assert_eq!(s.classification, Classification::Inferred);
    assert!(
        s.assumptions
            .iter()
            .any(|a| a.statement.contains("includes the router"))
    );
}

#[test]
fn two_services_with_the_same_route_make_a_weak_match() {
    let dir = repo("export const item = (id: number) => fetch(`/items/${id}`);\n");
    write(
        dir.path(),
        "admin/pyproject.toml",
        "[project]\nname = \"admin-api\"\n",
    );
    write(dir.path(), "admin/main.py", API);
    let s = only_seam(dir.path());
    assert_eq!(s.classification, Classification::Inferred);
    assert_eq!(s.confidence.level, ConfidenceLevel::Weak);
    assert!(s.producer.is_none());
}

#[test]
fn a_literal_segment_may_fill_a_parameter_but_not_the_reverse() {
    let s = only_seam(
        repo("export const me = () => fetch(\"http://localhost/items/latest\");\n").path(),
    );
    assert!(s.producer.is_some());
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "api/main.py",
        "from fastapi import FastAPI\napp = FastAPI()\n@app.get(\"/items/latest\")\ndef latest():\n    return {}\n",
    );
    write(
        dir.path(),
        "web/client.ts",
        "export const item = (id: number) => fetch(`http://localhost/items/${id}`);\n",
    );
    let s = only_seam(dir.path());
    assert!(
        s.producer.is_none(),
        "a runtime id must not be claimed to be `latest`"
    );
}

#[test]
fn secret_files_are_never_read() {
    let dir = repo("export const item = (id: number) => fetch(`/items/${id}`);\n");
    write(dir.path(), "web/.env", "API_URL=http://secret-host:9999\n");
    let a = analyse(dir.path(), &Options::default(), &CancelToken::default()).unwrap();
    assert!(a.skipped.iter().any(|s| s.path == "web/.env"));
    let json = serde_json::to_string(&a.seams).unwrap();
    assert!(!json.contains("secret-host"));
}
