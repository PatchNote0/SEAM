//! One SQLite database per analysed repository, by default at
//! `<repo>/.seam/seam.db` (ADR 0004).
//!
//! A scan is recorded as a snapshot. Each Seam seen in it gets an
//! observation holding its full JSON, which is the source of truth for
//! `seam explain`; the relational columns exist so the ledger can be
//! filtered without parsing JSON. Lifecycle is decided against the Seam's
//! observation in the previous snapshot. Seams that disappear are kept as
//! `removed` history.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};
use seam_core::seam::lifecycle;
use seam_core::{Lifecycle, Seam, SeamId};
use serde::Serialize;

/// Applied in order inside one transaction; the count is kept in
/// `PRAGMA user_version`.
const MIGRATIONS: [&str; 1] = [r#"
CREATE TABLE snapshot (
    id INTEGER PRIMARY KEY,
    root TEXT NOT NULL,
    engine_version TEXT NOT NULL,
    started_at TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    file_count INTEGER NOT NULL,
    translations TEXT NOT NULL
);
CREATE TABLE file (
    snapshot_id INTEGER NOT NULL REFERENCES snapshot(id),
    path TEXT NOT NULL,
    hash TEXT NOT NULL,
    kind TEXT NOT NULL,
    bytes INTEGER NOT NULL,
    PRIMARY KEY (snapshot_id, path)
);
CREATE TABLE seam (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL,
    mechanism TEXT NOT NULL,
    first_snapshot INTEGER NOT NULL REFERENCES snapshot(id),
    last_snapshot INTEGER NOT NULL REFERENCES snapshot(id),
    state TEXT NOT NULL
);
CREATE TABLE observation (
    snapshot_id INTEGER NOT NULL REFERENCES snapshot(id),
    seam_id TEXT NOT NULL REFERENCES seam(id),
    class TEXT NOT NULL,
    confidence TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    state TEXT NOT NULL,
    json TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, seam_id)
);
CREATE TABLE evidence (
    snapshot_id INTEGER NOT NULL,
    seam_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    role TEXT NOT NULL,
    path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER NOT NULL,
    hash TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, seam_id, ordinal)
);
CREATE INDEX evidence_by_path ON evidence(path);
CREATE TABLE finding (
    snapshot_id INTEGER NOT NULL,
    seam_id TEXT NOT NULL,
    ordinal INTEGER NOT NULL,
    kind TEXT NOT NULL,
    severity TEXT NOT NULL,
    field TEXT,
    message TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, seam_id, ordinal)
);
"#];

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("the store at {} could not be opened: {source}", path.display())]
    Open {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("the folder {} could not be created: {source}", path.display())]
    Folder {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("the store at {} was written by a newer SEAM (schema {found}, this version knows {known})", path.display())]
    TooNew {
        path: PathBuf,
        found: i64,
        known: i64,
    },
    #[error("the store could not be read or written: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("a stored Seam could not be decoded: {0}")]
    Json(#[from] serde_json::Error),
}

/// A scan as it is recorded.
pub struct NewSnapshot<'a> {
    pub root: &'a str,
    pub engine_version: &'a str,
    pub started_at: &'a str,
    pub duration_ms: u64,
    pub files: Vec<(String, String, String, u64)>,
    pub seams: &'a [Seam],
    pub translations: &'a BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SnapshotInfo {
    pub id: i64,
    pub root: String,
    pub engine_version: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub file_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Recorded {
    pub snapshot: i64,
    /// The lifecycle state of every Seam present in the snapshot.
    pub states: BTreeMap<SeamId, Lifecycle>,
    /// Seams present in the previous snapshot and absent from this one.
    pub removed: Vec<(SeamId, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct LedgerRow {
    pub id: SeamId,
    pub key: String,
    pub mechanism: String,
    pub class: String,
    pub confidence: String,
    pub state: String,
    pub breaking: u32,
    pub risk: u32,
    pub info: u32,
}

/// A Seam as last observed, with the snapshot it was seen in.
pub struct StoredSeam {
    pub seam: Seam,
    pub state: Lifecycle,
    pub snapshot: SnapshotInfo,
    /// Persian renderings recorded with that snapshot.
    pub translations: BTreeMap<String, String>,
}

pub struct Store {
    conn: Connection,
    path: PathBuf,
}

/// The default location: `<root>/.seam/seam.db`.
pub fn default_path(root: &Path) -> PathBuf {
    root.join(".seam").join("seam.db")
}

fn text<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn parse_state(text: &str) -> Option<Lifecycle> {
    serde_json::from_value(serde_json::Value::String(text.to_owned())).ok()
}

impl Store {
    /// Opens the store, creating it and its folder when needed, and brings
    /// the schema up to date.
    pub fn open(path: &Path) -> Result<Store, StoreError> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|source| StoreError::Folder {
                path: parent.to_path_buf(),
                source,
            })?;
            // Keep the store out of version control and out of scans.
            let ignore = parent.join(".gitignore");
            if parent.file_name().is_some_and(|n| n == ".seam") && !ignore.exists() {
                let _ = std::fs::write(ignore, "*\n");
            }
        }
        let conn = Connection::open(path).map_err(|source| StoreError::Open {
            path: path.to_path_buf(),
            source,
        })?;
        let mut store = Store {
            conn,
            path: path.to_path_buf(),
        };
        store.migrate()?;
        Ok(store)
    }

    /// Opens an existing store without creating anything.
    pub fn open_existing(path: &Path) -> Result<Option<Store>, StoreError> {
        if !path.is_file() {
            return Ok(None);
        }
        Store::open(path).map(Some)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&mut self) -> Result<(), StoreError> {
        self.conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = self
            .conn
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        let known = MIGRATIONS.len() as i64;
        if version > known {
            return Err(StoreError::TooNew {
                path: self.path.clone(),
                found: version,
                known,
            });
        }
        let tx = self.conn.transaction()?;
        for migration in MIGRATIONS.iter().skip(version as usize) {
            tx.execute_batch(migration)?;
        }
        tx.pragma_update(None, "user_version", known)?;
        tx.commit()?;
        Ok(())
    }

    pub fn latest_snapshot(&self) -> Result<Option<SnapshotInfo>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, root, engine_version, started_at, duration_ms, file_count FROM snapshot ORDER BY id DESC LIMIT 1",
                [],
                |r| {
                    Ok(SnapshotInfo {
                        id: r.get(0)?,
                        root: r.get(1)?,
                        engine_version: r.get(2)?,
                        started_at: r.get(3)?,
                        duration_ms: r.get::<_, i64>(4)? as u64,
                        file_count: r.get::<_, i64>(5)? as u64,
                    })
                },
            )
            .optional()?)
    }

    /// The fingerprint each Seam had in the latest snapshot.
    fn previous_fingerprints(&self) -> Result<BTreeMap<String, String>, StoreError> {
        let Some(latest) = self.latest_snapshot()? else {
            return Ok(BTreeMap::new());
        };
        let mut stmt = self
            .conn
            .prepare("SELECT seam_id, fingerprint FROM observation WHERE snapshot_id = ?1")?;
        let rows = stmt.query_map([latest.id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Lifecycle states for `seams` against the latest snapshot, without writing.
    pub fn preview(&self, seams: &[Seam]) -> Result<BTreeMap<SeamId, Lifecycle>, StoreError> {
        let previous = self.previous_fingerprints()?;
        Ok(seams
            .iter()
            .map(|s| {
                (
                    s.id.clone(),
                    lifecycle(previous.get(s.id.as_str()).map(String::as_str), s),
                )
            })
            .collect())
    }

    /// Records a scan in one transaction.
    pub fn record(&mut self, snap: &NewSnapshot<'_>) -> Result<Recorded, StoreError> {
        let previous = self.previous_fingerprints()?;
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO snapshot (root, engine_version, started_at, duration_ms, file_count, translations) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                snap.root,
                snap.engine_version,
                snap.started_at,
                snap.duration_ms as i64,
                snap.files.len() as i64,
                serde_json::to_string(snap.translations)?
            ],
        )?;
        let snapshot = tx.last_insert_rowid();
        {
            let mut insert_file = tx.prepare("INSERT INTO file (snapshot_id, path, hash, kind, bytes) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            for (path, hash, kind, bytes) in &snap.files {
                insert_file.execute(params![snapshot, path, hash, kind, *bytes as i64])?;
            }
        }
        let mut states = BTreeMap::new();
        for seam in snap.seams {
            let state = lifecycle(previous.get(seam.id.as_str()).map(String::as_str), seam);
            states.insert(seam.id.clone(), state);
            let state_text = text(&state);
            tx.execute(
                "INSERT INTO seam (id, key, mechanism, first_snapshot, last_snapshot, state) VALUES (?1, ?2, ?3, ?4, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET key = excluded.key, mechanism = excluded.mechanism, last_snapshot = excluded.last_snapshot, state = excluded.state",
                params![seam.id.as_str(), seam.key, seam.mechanism.to_string(), snapshot, state_text],
            )?;
            tx.execute(
                "INSERT INTO observation (snapshot_id, seam_id, class, confidence, fingerprint, state, json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    snapshot,
                    seam.id.as_str(),
                    seam.classification.to_string(),
                    text(&seam.confidence.level),
                    seam.fingerprint,
                    state_text,
                    serde_json::to_string(seam)?
                ],
            )?;
            for (i, e) in seam.evidence.iter().enumerate() {
                tx.execute(
                    "INSERT INTO evidence (snapshot_id, seam_id, ordinal, role, path, start_line, end_line, hash) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![snapshot, seam.id.as_str(), i as i64, text(&e.role), e.span.path, e.span.start.line, e.span.end.line, e.file_hash.to_hex()],
                )?;
            }
            for (i, f) in seam.findings.iter().enumerate() {
                tx.execute(
                    "INSERT INTO finding (snapshot_id, seam_id, ordinal, kind, severity, field, message) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![snapshot, seam.id.as_str(), i as i64, text(&f.kind), text(&f.severity), f.field, f.message],
                )?;
            }
        }
        let mut removed = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, key FROM seam WHERE state != 'removed' AND last_snapshot < ?1",
            )?;
            let gone = stmt.query_map([snapshot], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in gone {
                let (id, key) = row?;
                if let Some(id) = SeamId::parse(&id) {
                    removed.push((id, key));
                }
            }
        }
        for (id, _) in &removed {
            tx.execute(
                "UPDATE seam SET state = 'removed' WHERE id = ?1",
                [id.as_str()],
            )?;
        }
        tx.commit()?;
        Ok(Recorded {
            snapshot,
            states,
            removed,
        })
    }

    /// The ledger: every Seam in the latest snapshot, then removed ones.
    pub fn ledger(&self) -> Result<Vec<LedgerRow>, StoreError> {
        let Some(latest) = self.latest_snapshot()? else {
            return Ok(vec![]);
        };
        let mut stmt = self.conn.prepare(
            "SELECT o.seam_id, s.key, s.mechanism, o.class, o.confidence, o.state,
                    (SELECT COUNT(*) FROM finding f WHERE f.snapshot_id = o.snapshot_id AND f.seam_id = o.seam_id AND f.severity = 'breaking'),
                    (SELECT COUNT(*) FROM finding f WHERE f.snapshot_id = o.snapshot_id AND f.seam_id = o.seam_id AND f.severity = 'risk'),
                    (SELECT COUNT(*) FROM finding f WHERE f.snapshot_id = o.snapshot_id AND f.seam_id = o.seam_id AND f.severity = 'info')
             FROM observation o JOIN seam s ON s.id = o.seam_id
             WHERE o.snapshot_id = ?1 ORDER BY s.key",
        )?;
        let mut rows: Vec<LedgerRow> = stmt
            .query_map([latest.id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, u32>(6)?,
                    r.get::<_, u32>(7)?,
                    r.get::<_, u32>(8)?,
                ))
            })?
            .filter_map(Result::ok)
            .filter_map(
                |(id, key, mechanism, class, confidence, state, breaking, risk, info)| {
                    Some(LedgerRow {
                        id: SeamId::parse(&id)?,
                        key,
                        mechanism,
                        class,
                        confidence,
                        state,
                        breaking,
                        risk,
                        info,
                    })
                },
            )
            .collect();
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.key, s.mechanism, o.class, o.confidence FROM seam s
             JOIN observation o ON o.seam_id = s.id AND o.snapshot_id = s.last_snapshot
             WHERE s.state = 'removed' ORDER BY s.key",
        )?;
        let removed = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?;
        for row in removed.filter_map(Result::ok) {
            let (id, key, mechanism, class, confidence) = row;
            if let Some(id) = SeamId::parse(&id) {
                rows.push(LedgerRow {
                    id,
                    key,
                    mechanism,
                    class,
                    confidence,
                    state: "removed".into(),
                    breaking: 0,
                    risk: 0,
                    info: 0,
                });
            }
        }
        Ok(rows)
    }

    /// A Seam as last observed, with its state, its snapshot and the
    /// Persian renderings recorded with that snapshot.
    pub fn seam(&self, id: &SeamId) -> Result<Option<StoredSeam>, StoreError> {
        let row = self
            .conn
            .query_row(
                "SELECT o.json, s.state, o.snapshot_id FROM seam s JOIN observation o ON o.seam_id = s.id AND o.snapshot_id = s.last_snapshot WHERE s.id = ?1",
                [id.as_str()],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
            )
            .optional()?;
        let Some((json, state, snapshot)) = row else {
            return Ok(None);
        };
        let seam: Seam = serde_json::from_str(&json)?;
        let state = parse_state(&state).unwrap_or(Lifecycle::Discovered);
        let (info, translations) = self.conn.query_row(
            "SELECT id, root, engine_version, started_at, duration_ms, file_count, translations FROM snapshot WHERE id = ?1",
            [snapshot],
            |r| {
                Ok((
                    SnapshotInfo {
                        id: r.get(0)?,
                        root: r.get(1)?,
                        engine_version: r.get(2)?,
                        started_at: r.get(3)?,
                        duration_ms: r.get::<_, i64>(4)? as u64,
                        file_count: r.get::<_, i64>(5)? as u64,
                    },
                    r.get::<_, String>(6)?,
                ))
            },
        )?;
        let translations = serde_json::from_str(&translations).unwrap_or_default();
        Ok(Some(StoredSeam {
            seam,
            state,
            snapshot: info,
            translations,
        }))
    }

    /// Ids that start with `prefix`, for short ids typed by hand.
    pub fn ids_starting_with(&self, prefix: &str) -> Result<Vec<SeamId>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM seam WHERE id LIKE ?1 || '%' ORDER BY id")?;
        let ids = stmt.query_map([prefix], |r| r.get::<_, String>(0))?;
        Ok(ids
            .filter_map(Result::ok)
            .filter_map(|i| SeamId::parse(&i))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seam_core::{
        Classification, Comparison, Confidence, ConfidenceLevel, Finding, FindingKind, HttpMethod,
        Mechanism, Participant, RouteTemplate, Severity,
    };

    fn seam(key: &str, fingerprint: &str, findings: Vec<Finding>) -> Seam {
        Seam {
            id: SeamId::from_key(key),
            key: key.into(),
            mechanism: Mechanism::Http {
                method: HttpMethod::Get,
                route: RouteTemplate::parse("/a/{id}"),
            },
            consumer: Participant {
                component: "web".into(),
                symbol: None,
                evidence: vec![],
            },
            producer: Some(Participant {
                component: "api".into(),
                symbol: None,
                evidence: vec![],
            }),
            contract: None,
            request: Comparison::default(),
            response: Comparison::default(),
            findings,
            classification: Classification::Verified,
            confidence: Confidence {
                level: ConfidenceLevel::Strong,
                reasons: vec![],
            },
            assumptions: vec![],
            evidence: vec![],
            tests: vec![],
            fingerprint: fingerprint.into(),
        }
    }

    fn record(store: &mut Store, seams: &[Seam]) -> Recorded {
        store
            .record(&NewSnapshot {
                root: "/repo",
                engine_version: "0.1.0",
                started_at: "2026-11-01T00:00:00Z",
                duration_ms: 1,
                files: vec![("a.ts".into(), "00".into(), "type_script".into(), 1)],
                seams,
                translations: &BTreeMap::new(),
            })
            .unwrap()
    }

    #[test]
    fn lifecycle_moves_through_discovered_reconfirmed_changed_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&default_path(dir.path())).unwrap();
        let a = seam("http GET /a/{} producer=api consumer=web", "f1", vec![]);
        let b = seam("http GET /b producer=api consumer=web", "f2", vec![]);
        let first = record(&mut store, &[a.clone(), b.clone()]);
        assert_eq!(first.states[&a.id], Lifecycle::Discovered);
        let second = record(&mut store, &[a.clone(), b.clone()]);
        assert_eq!(second.states[&a.id], Lifecycle::Reconfirmed);
        let changed = seam("http GET /a/{} producer=api consumer=web", "f9", vec![]);
        let third = record(&mut store, std::slice::from_ref(&changed));
        assert_eq!(third.states[&a.id], Lifecycle::Changed);
        assert_eq!(third.removed, vec![(b.id.clone(), b.key.clone())]);
        let ledger = store.ledger().unwrap();
        assert_eq!(ledger.len(), 2);
        assert_eq!(
            ledger
                .iter()
                .find(|r| r.id == b.id)
                .map(|r| r.state.as_str()),
            Some("removed")
        );
    }

    #[test]
    fn a_seam_round_trips_through_its_json() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(&dir.path().join("x.db")).unwrap();
        let finding = Finding {
            kind: FindingKind::MissingField,
            severity: Severity::Breaking,
            field: Some("name".into()),
            message: "m".into(),
        };
        let s = seam(
            "http GET /a/{} producer=api consumer=web",
            "f1",
            vec![finding],
        );
        record(&mut store, std::slice::from_ref(&s));
        let stored = store.seam(&s.id).unwrap().unwrap();
        assert_eq!(stored.seam, s);
        assert_eq!(stored.state, Lifecycle::PotentiallyIncompatible);
        assert_eq!(store.ledger().unwrap()[0].breaking, 1);
        assert_eq!(
            store.ids_starting_with(&s.id.as_str()[..8]).unwrap(),
            vec![s.id.clone()]
        );
    }

    #[test]
    fn a_newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.db");
        drop(Store::open(&path).unwrap());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", 99)
            .unwrap();
        assert!(matches!(Store::open(&path), Err(StoreError::TooNew { .. })));
    }

    #[test]
    fn the_store_folder_ignores_itself() {
        let dir = tempfile::tempdir().unwrap();
        drop(Store::open(&default_path(dir.path())).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(".seam/.gitignore")).unwrap(),
            "*\n"
        );
    }
}
