//! `seam`: find the Seams between the parts of a system, prove them with
//! source evidence, and track how they change.
//!
//! Exit status: 0 success; 1 findings at or above `--fail-on`; 2 usage
//! error; 3 the repository could not be read; 4 the store could not be
//! opened or written; 5 nothing recorded yet, or no such Seam; 130 cancelled.

mod i18n;
mod render;
mod report;

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand, ValueEnum};
use seam_analysis::{CancelToken, Options, ScanError};
use seam_core::{ContentHash, Lifecycle, SeamId, Severity};
use seam_storage::{NewSnapshot, Store, StoreError};

use i18n::{Lang, Text};
use render::{ColorChoice, Out};

#[derive(Parser)]
#[command(
    name = "seam",
    version,
    about = "Find, prove and track the Seams between the parts of a system.",
    long_about = "Find, prove and track the Seams between the parts of a system.\n\nSEAM reads source files and nothing else: it opens no network connection, runs nothing from the repository and keeps its results in <repo>/.seam/seam.db."
)]
struct Cli {
    /// Language of human-readable output. Defaults to SEAM_LANG, then LANG.
    #[arg(long, global = true, value_enum)]
    lang: Option<Lang>,
    /// When to colour the output.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scan a repository, record the result and print its Seams.
    Scan(ScanArgs),
    /// List the Seams recorded by the last scan, and the ones removed since.
    Seams(SeamsArgs),
    /// Show the Evidence Passport of one Seam, checked against the files today.
    Explain(ExplainArgs),
    /// Scan without recording and write a JSON or Markdown report.
    Report(ReportArgs),
    /// Show what each language adapter reads, resolves and does not handle.
    Languages(LanguagesArgs),
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Format {
    Text,
    Json,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ReportFormat {
    Json,
    Markdown,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum FailOn {
    Never,
    Breaking,
    Risk,
    Info,
}

#[derive(Args)]
struct ScanOptions {
    /// Leave out paths matching this pattern, written like a .gitignore line. Repeatable.
    #[arg(long = "exclude", value_name = "PATTERN")]
    excludes: Vec<String>,
    /// Worker threads. Defaults to one per core.
    #[arg(long, value_name = "N")]
    threads: Option<usize>,
}

#[derive(Args)]
struct ScanArgs {
    /// The repository to scan.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Where to record the scan. Defaults to <path>/.seam/seam.db.
    #[arg(long, value_name = "FILE")]
    store: Option<PathBuf>,
    /// Do not record the scan.
    #[arg(long, conflicts_with = "store")]
    no_store: bool,
    /// Exit with status 1 when a finding of this severity or worse exists.
    #[arg(long, value_enum, default_value_t = FailOn::Never)]
    fail_on: FailOn,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
    #[command(flatten)]
    options: ScanOptions,
}

#[derive(Args)]
struct SeamsArgs {
    /// The repository whose ledger to show.
    #[arg(default_value = ".")]
    path: PathBuf,
    /// The store to read. Defaults to <path>/.seam/seam.db.
    #[arg(long, value_name = "FILE")]
    store: Option<PathBuf>,
    /// Show only Seams of these classes, for example VERIFIED.
    #[arg(long = "class", value_name = "CLASS")]
    classes: Vec<String>,
    /// Show only Seams in these states, for example potentially-incompatible.
    #[arg(long = "state", value_name = "STATE")]
    states: Vec<String>,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Args)]
struct ExplainArgs {
    /// The Seam id, or enough of its start to be unique.
    id: String,
    /// The repository the Seam was recorded in.
    #[arg(long, default_value = ".")]
    repo: PathBuf,
    /// The store to read. Defaults to <repo>/.seam/seam.db.
    #[arg(long, value_name = "FILE")]
    store: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Args)]
struct ReportArgs {
    /// The repository to scan.
    #[arg(default_value = ".")]
    path: PathBuf,
    #[arg(long, value_enum, default_value_t = ReportFormat::Json)]
    format: ReportFormat,
    /// Write the report to this file instead of standard output.
    #[arg(long, short, value_name = "FILE")]
    output: Option<PathBuf>,
    /// Compare with this store for lifecycle states. Defaults to <path>/.seam/seam.db when it exists.
    #[arg(long, value_name = "FILE")]
    store: Option<PathBuf>,
    #[command(flatten)]
    options: ScanOptions,
}

#[derive(Args)]
struct LanguagesArgs {
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

enum Failure {
    Message(u8, String),
    Cancelled,
}

impl From<StoreError> for Failure {
    fn from(e: StoreError) -> Failure {
        Failure::Message(4, e.to_string())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let lang = Lang::resolve(cli.lang);
    let color = cli.color.enabled();
    let result = match cli.command {
        Command::Scan(args) => scan(args, lang, color),
        Command::Seams(args) => seams(args, lang, color),
        Command::Explain(args) => explain(args, lang, color),
        Command::Report(args) => report_cmd(args, lang),
        Command::Languages(args) => languages(args, lang, color),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(Failure::Cancelled) => {
            eprintln!("{}", Text::new(lang, BTreeMap::new()).cancelled());
            ExitCode::from(130)
        }
        Err(Failure::Message(code, message)) => {
            let t = Text::new(lang, BTreeMap::new());
            eprintln!("{}: {message}", t.error_prefix());
            ExitCode::from(code)
        }
    }
}

fn print(text: &str) -> Result<u8, Failure> {
    let mut stdout = std::io::stdout().lock();
    // A closed pipe (`seam scan | head`) is not an error worth reporting.
    let _ = stdout.write_all(text.as_bytes());
    let _ = stdout.flush();
    Ok(0)
}

fn json<T: serde::Serialize>(value: &T) -> Result<String, Failure> {
    serde_json::to_string_pretty(value)
        .map(|mut s| {
            s.push('\n');
            s
        })
        .map_err(|e| Failure::Message(3, e.to_string()))
}

fn analyse(path: &Path, options: &ScanOptions) -> Result<seam_analysis::Analysis, Failure> {
    let cancel = CancelToken::default();
    let handler = cancel.clone();
    // A second Ctrl-C, or a handler that cannot be installed, falls back to
    // the default behaviour: the process stops and nothing is recorded.
    let _ = ctrlc::set_handler(move || handler.cancel());
    let opts = Options {
        excludes: options.excludes.clone(),
        threads: options.threads,
        ..Options::default()
    };
    seam_analysis::analyse(path, &opts, &cancel).map_err(|e| match e {
        ScanError::Cancelled => Failure::Cancelled,
        ScanError::Discover(e) => Failure::Message(2, e.to_string()),
        other => Failure::Message(3, other.to_string()),
    })
}

fn store_path(path: &Path, store: Option<&PathBuf>) -> PathBuf {
    store
        .cloned()
        .unwrap_or_else(|| seam_storage::default_path(path))
}

fn files_of(a: &seam_analysis::Analysis) -> Vec<(String, String, String, u64)> {
    a.files
        .iter()
        .map(|f| {
            let kind = serde_json::to_value(f.kind)
                .map(|v| v.to_string())
                .unwrap_or_default();
            (
                f.path.clone(),
                f.hash.to_hex(),
                kind.trim_matches('"').to_owned(),
                f.bytes,
            )
        })
        .collect()
}

fn fail_code(fail_on: FailOn, a: &seam_analysis::Analysis) -> (u8, usize) {
    let threshold = match fail_on {
        FailOn::Never => return (0, 0),
        FailOn::Breaking => Severity::Breaking,
        FailOn::Risk => Severity::Risk,
        FailOn::Info => Severity::Info,
    };
    let n = a
        .seams
        .iter()
        .flat_map(|s| &s.findings)
        .filter(|f| f.severity <= threshold)
        .count();
    (u8::from(n > 0), n)
}

fn scan(args: ScanArgs, lang: Lang, color: bool) -> Result<u8, Failure> {
    let started_at = utc_now();
    let a = analyse(&args.path, &args.options)?;
    let mut states = BTreeMap::new();
    let mut stored = None;
    let mut removed = Vec::new();
    let mut recorded = None;
    if !args.no_store {
        let path = store_path(&args.path, args.store.as_ref());
        let mut store = Store::open(&path)?;
        let result = store.record(&NewSnapshot {
            root: &a.root,
            engine_version: a.engine_version,
            started_at: &started_at,
            duration_ms: a.duration_ms,
            files: files_of(&a),
            seams: &a.seams,
            translations: &a.translations,
        })?;
        states = result.states.clone();
        removed = result.removed.clone();
        stored = Some(path.display().to_string());
        recorded = Some(report::Recorded {
            store: path.display().to_string(),
            snapshot: result.snapshot,
            removed: result
                .removed
                .iter()
                .map(|(id, _)| id.to_string())
                .collect(),
        });
    }
    let (code, failing) = fail_code(args.fail_on, &a);
    match args.format {
        Format::Json => print(&json(&report::report(&a, &states, &started_at, recorded))?)?,
        Format::Text => {
            let out = Out::new(Text::new(lang, a.translations.clone()), color);
            print(&render::scan(
                &out,
                &a,
                &states,
                stored.as_deref(),
                removed.len(),
            ))?
        }
    };
    if code != 0 {
        let level = args
            .fail_on
            .to_possible_value()
            .map(|v| v.get_name().to_owned())
            .unwrap_or_default();
        eprintln!(
            "{}",
            Text::new(lang, BTreeMap::new()).failed_on(failing, &level)
        );
    }
    Ok(code)
}

fn seams(args: SeamsArgs, lang: Lang, color: bool) -> Result<u8, Failure> {
    let path = store_path(&args.path, args.store.as_ref());
    let t = Text::new(lang, BTreeMap::new());
    let Some(store) = Store::open_existing(&path)? else {
        return Err(Failure::Message(
            5,
            t.no_snapshot(&path.display().to_string()),
        ));
    };
    let Some(snapshot) = store.latest_snapshot()? else {
        return Err(Failure::Message(
            5,
            t.no_snapshot(&path.display().to_string()),
        ));
    };
    let wanted = |value: &str, filter: &[String]| {
        filter.is_empty() || filter.iter().any(|f| f.eq_ignore_ascii_case(value))
    };
    let rows: Vec<_> = store
        .ledger()?
        .into_iter()
        .filter(|r| wanted(&r.class, &args.classes) && wanted(&r.state, &args.states))
        .collect();
    match args.format {
        Format::Json => print(&json(&rows)?),
        Format::Text => {
            let out = Out::new(t, color);
            let header = out.text.last_scan(&snapshot.started_at, &snapshot.root);
            print(&render::ledger(&out, &rows, &header))
        }
    }
}

fn explain(args: ExplainArgs, lang: Lang, color: bool) -> Result<u8, Failure> {
    let path = store_path(&args.repo, args.store.as_ref());
    let t = Text::new(lang, BTreeMap::new());
    let Some(store) = Store::open_existing(&path)? else {
        return Err(Failure::Message(
            5,
            t.no_snapshot(&path.display().to_string()),
        ));
    };
    let wanted = if args.id.starts_with("sm_") {
        args.id.clone()
    } else {
        format!("sm_{}", args.id)
    };
    let ids = store.ids_starting_with(&wanted)?;
    let id: SeamId = match ids.as_slice() {
        [one] => one.clone(),
        [] => return Err(Failure::Message(5, t.not_found(&args.id))),
        many => return Err(Failure::Message(5, t.ambiguous(&args.id, many.len()))),
    };
    let Some(seam_storage::StoredSeam {
        seam,
        state,
        snapshot,
        translations,
    }) = store.seam(&id)?
    else {
        return Err(Failure::Message(5, t.not_found(&args.id)));
    };
    // Revalidate: every cited file is hashed again as it is now.
    let root = PathBuf::from(&snapshot.root);
    let mut hashes: BTreeMap<String, Option<ContentHash>> = BTreeMap::new();
    let stale: Vec<usize> = seam
        .evidence
        .iter()
        .enumerate()
        .filter(|(_, e)| {
            let now = hashes.entry(e.span.path.clone()).or_insert_with(|| {
                std::fs::read(root.join(&e.span.path))
                    .ok()
                    .map(|b| ContentHash::of(&b))
            });
            *now != Some(e.file_hash)
        })
        .map(|(i, _)| i)
        .collect();
    match args.format {
        Format::Json => {
            #[derive(serde::Serialize)]
            struct Passport<'a> {
                seam: &'a seam_core::Seam,
                state: Lifecycle,
                observed_at: &'a str,
                stale_evidence: &'a [usize],
            }
            print(&json(&Passport {
                seam: &seam,
                state,
                observed_at: &snapshot.started_at,
                stale_evidence: &stale,
            })?)
        }
        Format::Text => {
            let out = Out::new(Text::new(lang, translations), color);
            print(&render::passport(
                &out,
                &seam,
                state,
                &snapshot.started_at,
                &stale,
            ))
        }
    }
}

fn report_cmd(args: ReportArgs, lang: Lang) -> Result<u8, Failure> {
    let generated_at = utc_now();
    let a = analyse(&args.path, &args.options)?;
    let path = store_path(&args.path, args.store.as_ref());
    let states = match Store::open_existing(&path)? {
        Some(store) => store.preview(&a.seams)?,
        None => BTreeMap::new(),
    };
    let text = match args.format {
        ReportFormat::Json => json(&report::report(&a, &states, &generated_at, None))?,
        ReportFormat::Markdown => report::markdown(
            &Text::new(lang, a.translations.clone()),
            &a,
            &states,
            &generated_at,
        ),
    };
    match args.output {
        Some(file) => {
            std::fs::write(&file, text)
                .map_err(|e| Failure::Message(3, format!("{}: {e}", file.display())))?;
            Ok(0)
        }
        None => print(&text),
    }
}

fn languages(args: LanguagesArgs, lang: Lang, color: bool) -> Result<u8, Failure> {
    let caps = seam_adapters::capabilities();
    match args.format {
        Format::Json => print(&json(&caps)?),
        Format::Text => print(&render::languages(
            &Out::new(Text::new(lang, BTreeMap::new()), color),
            &caps,
        )),
    }
}

/// The current time in UTC as RFC 3339, without a date-time dependency.
fn utc_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Howard Hinnant's days-to-civil algorithm.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_formats_as_rfc3339() {
        let now = utc_now();
        assert_eq!(now.len(), 20, "{now}");
        assert!(now.ends_with('Z') && now.as_bytes()[10] == b'T');
        assert!(now.as_str() >= "2026-01-01");
    }

    #[test]
    fn the_command_line_is_well_formed() {
        use clap::CommandFactory;
        Cli::command().debug_assert();
    }
}
