//! Finding the files to analyse.
//!
//! Discovery starts at the root, honours `.gitignore` files (inside a Git
//! repository or not), `.git/info/exclude` and `.seamignore`, never follows
//! symbolic links, and skips version-control, dependency, build and tool
//! folders. Hidden files are visited so that a skipped secret is reported
//! by name. It also skips files that conventionally hold secrets, files over
//! the size limit, binary files and files that are not UTF-8. The user's
//! global Git excludes file is not read, so a scan gives the same result on
//! every machine.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::overrides::OverrideBuilder;
use seam_adapters::FileKind;
use serde::Serialize;

/// Folders that hold dependencies, build output or tool state, never sources.
pub const SKIPPED_DIRS: [&str; 30] = [
    ".git",
    ".hg",
    ".svn",
    ".seam",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    "vendor",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".angular",
    ".turbo",
    "coverage",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".cache",
    ".gradle",
    ".idea",
    ".vscode",
    ".yarn",
    ".pnpm-store",
    ".terraform",
];

/// Lockfiles are generated and large; nothing in them describes a Seam.
const LOCKFILES: [&str; 6] = [
    "package-lock.json",
    "npm-shrinkwrap.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "composer.lock",
    "Pipfile.lock",
];

/// True for files that conventionally hold secrets. They are never read, so
/// nothing from them can reach a report or the database.
pub fn is_secret_file(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    if lower == ".env" || (lower.starts_with(".env.") && lower != ".env.example") {
        return true;
    }
    if [".pem", ".key", ".p12", ".pfx"]
        .iter()
        .any(|ext| lower.ends_with(ext))
    {
        return true;
    }
    lower.starts_with("id_rsa")
        || lower.starts_with("id_ed25519")
        || lower == ".npmrc"
        || lower == ".pypirc"
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscoveredFile {
    /// Relative to the root, with `/` separators.
    pub path: String,
    pub absolute: PathBuf,
    pub kind: FileKind,
    pub bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub path: String,
    pub reason: SkipReason,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    Secret,
    TooLarge,
    Binary,
    NotUtf8,
    Unreadable,
}

#[derive(Debug, thiserror::Error)]
pub enum DiscoverError {
    #[error("the exclusion pattern `{pattern}` is not valid: {reason}")]
    BadPattern { pattern: String, reason: String },
}

pub struct Discovery {
    pub files: Vec<DiscoveredFile>,
    pub skipped: Vec<Skipped>,
}

/// Walks `root`. `excludes` are extra glob patterns, matched like `.gitignore` lines.
pub fn discover(
    root: &Path,
    max_bytes: u64,
    excludes: &[String],
) -> Result<Discovery, DiscoverError> {
    let mut overrides = OverrideBuilder::new(root);
    for pattern in excludes {
        overrides
            .add(&format!("!{pattern}"))
            .map_err(|e| DiscoverError::BadPattern {
                pattern: pattern.clone(),
                reason: e.to_string(),
            })?;
    }
    let overrides = overrides.build().map_err(|e| DiscoverError::BadPattern {
        pattern: excludes.join(", "),
        reason: e.to_string(),
    })?;
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .parents(true)
        .ignore(true)
        .git_ignore(true)
        .git_exclude(true)
        .git_global(false)
        .require_git(false)
        .follow_links(false)
        .add_custom_ignore_filename(".seamignore")
        .overrides(overrides)
        .filter_entry(|entry| {
            let is_dir = entry.file_type().is_some_and(|t| t.is_dir());
            !(is_dir && SKIPPED_DIRS.contains(&entry.file_name().to_string_lossy().as_ref()))
        })
        .build();

    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(relative) = entry.path().strip_prefix(root) else {
            continue;
        };
        let path = relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        let file_name = entry.file_name().to_string_lossy();
        if is_secret_file(&file_name) {
            skipped.push(Skipped {
                path,
                reason: SkipReason::Secret,
            });
            continue;
        }
        if LOCKFILES.contains(&file_name.as_ref()) {
            continue;
        }
        let Some(kind) = seam_adapters::kind_for_path(&path) else {
            continue;
        };
        let bytes = match entry.metadata() {
            Ok(meta) => meta.len(),
            Err(_) => {
                skipped.push(Skipped {
                    path,
                    reason: SkipReason::Unreadable,
                });
                continue;
            }
        };
        if bytes > max_bytes {
            skipped.push(Skipped {
                path,
                reason: SkipReason::TooLarge,
            });
            continue;
        }
        files.push(DiscoveredFile {
            path,
            absolute: entry.path().to_path_buf(),
            kind,
            bytes,
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    skipped.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Discovery { files, skipped })
}

/// Reads a discovered file as text, or says why it cannot be analysed.
pub fn read_text(file: &DiscoveredFile) -> Result<(String, Vec<u8>), SkipReason> {
    let bytes = std::fs::read(&file.absolute).map_err(|_| SkipReason::Unreadable)?;
    if bytes.iter().take(8000).any(|&b| b == 0) {
        return Err(SkipReason::Binary);
    }
    let text = String::from_utf8(bytes.clone()).map_err(|_| SkipReason::NotUtf8)?;
    Ok((text, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(root: &Path, path: &str, text: &str) {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }

    #[test]
    fn secrets_are_recognised_by_name() {
        for secret in [
            ".env",
            ".env.local",
            "server.pem",
            "tls.key",
            "id_rsa",
            "id_ed25519.pub",
            ".npmrc",
            ".pypirc",
            "cert.p12",
        ] {
            assert!(is_secret_file(secret), "{secret}");
        }
        for ordinary in [".env.example", "env.ts", "keys.py", "monkey.ts"] {
            assert!(!is_secret_file(ordinary), "{ordinary}");
        }
    }

    #[test]
    fn honours_gitignore_skips_dependencies_secrets_and_large_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(root, ".gitignore", "generated/\n");
        write(root, "web/src/api.ts", "export const x = 1;\n");
        write(root, "web/node_modules/lib/index.js", "fetch('/x')\n");
        write(root, "generated/client.ts", "fetch('/x')\n");
        write(root, ".env", "SECRET=1\n");
        write(root, "api/main.py", "x = 1\n");
        write(root, "api/big.py", &"#\n".repeat(600));
        write(root, "notes.md", "# notes\n");
        write(root, "web/package-lock.json", "{}\n");
        let found = discover(root, 1000, &[]).unwrap();
        let paths: Vec<_> = found.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["api/main.py", "web/src/api.ts"]);
        let skipped: Vec<_> = found
            .skipped
            .iter()
            .map(|s| (s.path.as_str(), s.reason))
            .collect();
        assert_eq!(
            skipped,
            vec![
                (".env", SkipReason::Secret),
                ("api/big.py", SkipReason::TooLarge)
            ]
        );
    }

    #[test]
    fn extra_exclusions_apply() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "web/src/a.ts", "\n");
        write(dir.path(), "web/legacy/b.ts", "\n");
        let found = discover(dir.path(), 1 << 20, &["web/legacy/**".into()]).unwrap();
        let paths: Vec<_> = found.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["web/src/a.ts"]);
    }

    #[test]
    fn binary_files_are_not_read_as_text() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("blob.py"), [b'a', 0, b'b']).unwrap();
        let found = discover(dir.path(), 1 << 20, &[]).unwrap();
        assert_eq!(read_text(&found.files[0]).unwrap_err(), SkipReason::Binary);
    }
}
