//! Package manifests, which mark where a component begins and name it.

use serde::{Deserialize, Serialize};

use crate::syntax::{Located, Source};

pub const ADAPTER: &str = "manifest@0.1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestKind {
    Npm,
    Python,
    Cargo,
    Go,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub kind: ManifestKind,
    pub name: Option<String>,
    pub loc: Located,
}

pub fn kind_for_file_name(file_name: &str) -> Option<ManifestKind> {
    match file_name {
        "package.json" => Some(ManifestKind::Npm),
        "pyproject.toml" => Some(ManifestKind::Python),
        "Cargo.toml" => Some(ManifestKind::Cargo),
        "go.mod" => Some(ManifestKind::Go),
        _ => None,
    }
}

pub fn read(source: &Source<'_>, kind: ManifestKind) -> Manifest {
    let (name, line) = match kind {
        ManifestKind::Npm => npm_name(source.text),
        ManifestKind::Python => toml_name(source.text, &["project", "tool.poetry"]),
        ManifestKind::Cargo => toml_name(source.text, &["package"]),
        ManifestKind::Go => go_module(source.text),
    };
    Manifest {
        kind,
        name,
        loc: line_loc(source, line.unwrap_or(1)),
    }
}

fn npm_name(text: &str) -> (Option<String>, Option<u32>) {
    let name = serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_owned));
    let line = text
        .lines()
        .position(|l| l.trim_start().starts_with("\"name\""))
        .map(|i| i as u32 + 1);
    (name, line)
}

/// Reads `name` from the first of `tables` that has one, with its line.
fn toml_name(text: &str, tables: &[&str]) -> (Option<String>, Option<u32>) {
    let Ok(doc) = text.parse::<toml::Table>() else {
        return (None, None);
    };
    for table in tables {
        let mut node = Some(&doc);
        for part in table.split('.') {
            node = node.and_then(|t| t.get(part)).and_then(|v| v.as_table());
        }
        if let Some(name) = node.and_then(|t| t.get("name")).and_then(|v| v.as_str()) {
            return (Some(name.to_owned()), name_line(text, table));
        }
    }
    (None, None)
}

/// The line of `name = ...` inside `[table]`, found by scanning headers.
fn name_line(text: &str, table: &str) -> Option<u32> {
    let header = format!("[{table}]");
    let mut inside = false;
    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            inside = trimmed == header;
        } else if inside
            && trimmed
                .split('=')
                .next()
                .is_some_and(|key| key.trim() == "name")
        {
            return Some(i as u32 + 1);
        }
    }
    None
}

fn go_module(text: &str) -> (Option<String>, Option<u32>) {
    for (i, line) in text.lines().enumerate() {
        if let Some(module) = line.trim().strip_prefix("module ") {
            return (Some(module.trim().to_owned()), Some(i as u32 + 1));
        }
    }
    (None, None)
}

fn line_loc(source: &Source<'_>, line: u32) -> Located {
    let text = source
        .text
        .lines()
        .nth(line as usize - 1)
        .unwrap_or_default();
    let start_byte: usize = source
        .text
        .lines()
        .take(line as usize - 1)
        .map(|l| l.len() + 1)
        .sum();
    let span = seam_core::Span {
        path: source.path.to_owned(),
        start: seam_core::Position { line, column: 1 },
        end: seam_core::Position {
            line,
            column: text.len() as u32 + 1,
        },
        start_byte,
        end_byte: start_byte + text.len(),
    };
    Located {
        span,
        excerpt: text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_names_from_each_manifest_kind() {
        let npm = read(
            &Source::new(
                "web/package.json",
                "{\n  \"name\": \"@ledgerline/web\",\n  \"private\": true\n}",
            ),
            ManifestKind::Npm,
        );
        assert_eq!(
            (npm.name.as_deref(), npm.loc.span.start.line),
            (Some("@ledgerline/web"), 2)
        );
        let py = read(
            &Source::new(
                "api/pyproject.toml",
                "[build-system]\nname = \"wrong\"\n\n[project]\nname = \"ledgerline-api\"\n",
            ),
            ManifestKind::Python,
        );
        assert_eq!(
            (py.name.as_deref(), py.loc.span.start.line),
            (Some("ledgerline-api"), 5)
        );
        let poetry = read(
            &Source::new("p/pyproject.toml", "[tool.poetry]\nname = \"poet\"\n"),
            ManifestKind::Python,
        );
        assert_eq!(poetry.name.as_deref(), Some("poet"));
        let cargo = read(
            &Source::new("ledger/Cargo.toml", "[package]\nname = \"ledger\"\n"),
            ManifestKind::Cargo,
        );
        assert_eq!(cargo.name.as_deref(), Some("ledger"));
        let go = read(
            &Source::new("svc/go.mod", "module example.com/svc\n\ngo 1.23\n"),
            ManifestKind::Go,
        );
        assert_eq!(go.name.as_deref(), Some("example.com/svc"));
    }

    #[test]
    fn broken_manifests_have_no_name() {
        let broken = read(
            &Source::new("package.json", "{ not json"),
            ManifestKind::Npm,
        );
        assert_eq!(broken.name, None);
        assert_eq!(broken.loc.span.start.line, 1);
    }
}
