//! Discovery, extraction, linking and Seam assembly.
//!
//! [`analyse`] walks a repository, reads every supported file in parallel,
//! links what the files say about each other and returns the Seams with
//! their evidence. It reads files and nothing else: no network, no process,
//! no write.

mod assemble;
mod consumers;
pub mod discover;
mod fa;
mod producers;
mod workspace;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use rayon::prelude::*;
use seam_adapters::FileKind;
use seam_core::{ContentHash, Seam};
use serde::Serialize;

pub use assemble::RouteSummary;
pub use discover::{SkipReason, Skipped};
pub use workspace::Component;

pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 2 MiB: larger files are almost always generated or vendored.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Options {
    pub max_file_bytes: u64,
    /// Extra patterns to leave out, written like `.gitignore` lines.
    pub excludes: Vec<String>,
    /// Worker threads; `None` uses one per core.
    pub threads: Option<usize>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            excludes: Vec::new(),
            threads: None,
        }
    }
}

/// Cancels a running scan from another thread, for example on Ctrl-C.
#[derive(Clone, Debug, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("{} is not a directory that can be read", .0.display())]
    Root(PathBuf),
    #[error(transparent)]
    Discover(#[from] discover::DiscoverError),
    #[error("the worker threads could not be started: {0}")]
    Threads(String),
    #[error("the scan was cancelled")]
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
pub struct FileSummary {
    pub path: String,
    pub kind: FileKind,
    pub bytes: u64,
    pub hash: ContentHash,
}

#[derive(Clone, Debug, Serialize)]
pub struct Diagnostic {
    pub path: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ComponentSummary {
    pub name: String,
    pub root: String,
    pub manifest: Option<String>,
    pub files: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct Analysis {
    pub root: String,
    pub engine_version: &'static str,
    pub duration_ms: u64,
    pub files: Vec<FileSummary>,
    pub skipped: Vec<Skipped>,
    pub components: Vec<ComponentSummary>,
    pub seams: Vec<Seam>,
    pub routes: Vec<RouteSummary>,
    pub diagnostics: Vec<Diagnostic>,
    /// Persian renderings of the sentences in `seams`, keyed by the English.
    #[serde(skip)]
    pub translations: BTreeMap<String, String>,
}

/// Analyses the repository at `root`.
pub fn analyse(
    root: &Path,
    options: &Options,
    cancel: &CancelToken,
) -> Result<Analysis, ScanError> {
    let started = Instant::now();
    if !root.is_dir() {
        return Err(ScanError::Root(root.to_path_buf()));
    }
    let root = root
        .canonicalize()
        .map_err(|_| ScanError::Root(root.to_path_buf()))?;
    let found = discover::discover(&root, options.max_file_bytes, &options.excludes)?;
    let mut skipped = found.skipped;

    let read = |file: &discover::DiscoveredFile| -> Option<Result<workspace::FileRecord, Skipped>> {
        if cancel.is_cancelled() {
            return None;
        }
        Some(match discover::read_text(file) {
            Ok((text, bytes)) => {
                let extraction = seam_adapters::extract(&file.path, file.kind, &text);
                Ok(workspace::FileRecord {
                    path: file.path.clone(),
                    hash: ContentHash::of(&bytes),
                    kind: file.kind,
                    bytes: file.bytes,
                    facts: extraction.facts,
                    diagnostics: extraction.diagnostics,
                })
            }
            Err(reason) => Err(Skipped {
                path: file.path.clone(),
                reason,
            }),
        })
    };
    let results: Vec<Option<Result<workspace::FileRecord, Skipped>>> = match options.threads {
        Some(n) => rayon::ThreadPoolBuilder::new()
            .num_threads(n.max(1))
            .build()
            .map_err(|e| ScanError::Threads(e.to_string()))?
            .install(|| found.files.par_iter().map(read).collect()),
        None => found.files.par_iter().map(read).collect(),
    };
    if cancel.is_cancelled() {
        return Err(ScanError::Cancelled);
    }
    let mut records = Vec::new();
    for result in results.into_iter().flatten() {
        match result {
            Ok(record) => records.push(record),
            Err(skip) => skipped.push(skip),
        }
    }
    skipped.sort_by(|a, b| a.path.cmp(&b.path));

    let root_name = root
        .file_name()
        .map_or_else(|| "root".to_owned(), |n| n.to_string_lossy().into_owned());
    let ws = workspace::Workspace::new(records, &root_name);
    let routes = producers::routes(&ws);
    let calls = consumers::calls(&ws);
    if cancel.is_cancelled() {
        return Err(ScanError::Cancelled);
    }
    let (seams, route_summaries) = assemble::seams(&ws, &routes, calls);

    let files: Vec<FileSummary> = ws
        .files
        .iter()
        .map(|f| FileSummary {
            path: f.path.clone(),
            kind: f.kind,
            bytes: f.bytes,
            hash: f.hash,
        })
        .collect();
    let diagnostics = ws
        .files
        .iter()
        .flat_map(|f| {
            f.diagnostics.iter().map(|m| Diagnostic {
                path: f.path.clone(),
                message: m.clone(),
            })
        })
        .collect();
    let components = ws
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| ComponentSummary {
            name: c.name.clone(),
            root: c.root.clone(),
            manifest: c.manifest.clone(),
            files: ws
                .files
                .iter()
                .filter(|f| ws.component_of(&f.path) == i)
                .count(),
        })
        .collect();
    Ok(Analysis {
        root: root.display().to_string(),
        engine_version: ENGINE_VERSION,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        files,
        skipped,
        components,
        seams,
        routes: route_summaries,
        diagnostics,
        translations: ws.translations(),
    })
}
