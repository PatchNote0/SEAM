//! Human-readable output.

use std::fmt::Write as _;

use seam_adapters::Capability;
use seam_analysis::Analysis;
use seam_core::{Classification, Comparison, Field, Lifecycle, Seam, Severity};
use seam_storage::LedgerRow;

use crate::i18n::{Lang, Text};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

impl ColorChoice {
    /// Colour only on a terminal, never with `NO_COLOR`, and not on Windows
    /// unless asked: older Windows consoles print the escape codes as text.
    pub fn enabled(self) -> bool {
        use std::io::IsTerminal;
        match self {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => {
                std::io::stdout().is_terminal()
                    && std::env::var_os("NO_COLOR").is_none()
                    && !cfg!(windows)
            }
        }
    }
}

#[derive(Clone, Copy)]
pub enum Style {
    Bold,
    Dim,
    Red,
    Yellow,
    Green,
    Cyan,
}

pub struct Out {
    pub text: Text,
    color: bool,
}

impl Out {
    pub fn new(text: Text, color: bool) -> Out {
        Out { text, color }
    }

    pub fn paint(&self, s: &str, style: Style) -> String {
        if !self.color {
            return s.to_owned();
        }
        let code = match style {
            Style::Bold => "1",
            Style::Dim => "2",
            Style::Red => "31",
            Style::Yellow => "33",
            Style::Green => "32",
            Style::Cyan => "36",
        };
        format!("\x1b[{code}m{s}\x1b[0m")
    }

    fn class(&self, c: Classification) -> String {
        let style = match c {
            Classification::Verified => Style::Green,
            Classification::Parsed => Style::Yellow,
            Classification::Inferred => Style::Cyan,
            Classification::Correlated | Classification::Unknown => Style::Dim,
        };
        self.paint(&self.text.class(c), style)
    }

    fn severity(&self, s: Severity) -> String {
        let style = match s {
            Severity::Breaking => Style::Red,
            Severity::Risk => Style::Yellow,
            Severity::Info => Style::Dim,
        };
        self.paint(self.text.severity(s), style)
    }

    fn sides(&self, seam: &Seam) -> String {
        let side = |component: &str, symbol: &Option<String>| match symbol {
            Some(s) => format!("{component} ({s})"),
            None => component.to_owned(),
        };
        let producer = seam.producer.as_ref().map_or_else(
            || self.text.l_no_producer().to_owned(),
            |p| side(&p.component, &p.symbol),
        );
        format!(
            "{} → {producer}",
            side(&seam.consumer.component, &seam.consumer.symbol)
        )
    }
}

/// The result of `seam scan`.
pub fn scan(
    out: &Out,
    a: &Analysis,
    states: &std::collections::BTreeMap<seam_core::SeamId, Lifecycle>,
    stored: Option<&str>,
    removed: usize,
) -> String {
    let t = &out.text;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{} · {}",
        out.paint(&format!("SEAM {}", a.engine_version), Style::Bold),
        t.scanned(&a.root, a.duration_ms)
    );
    let _ = writeln!(
        s,
        "{}",
        t.counts(
            a.files.len(),
            a.components.len(),
            a.routes.len(),
            a.seams.len()
        )
    );
    if a.seams.is_empty() {
        let _ = writeln!(s, "\n{}", t.no_seams());
    }
    for seam in &a.seams {
        let _ = writeln!(s);
        let _ = writeln!(
            s,
            "{}  {}",
            out.paint(&seam.mechanism.to_string(), Style::Bold),
            out.class(seam.classification)
        );
        let _ = writeln!(s, "  {}", out.sides(seam));
        let mut meta = vec![seam.id.to_string()];
        if let Some(state) = states.get(&seam.id) {
            meta.push(t.state(*state));
        }
        if let Some(op) = seam.contract.as_ref().and_then(|c| c.operation_id.clone()) {
            meta.push(format!("{} {op}", t.l_contract().to_lowercase()));
        }
        let _ = writeln!(s, "  {}", out.paint(&meta.join(" · "), Style::Dim));
        for f in &seam.findings {
            let _ = writeln!(
                s,
                "  {}  {}",
                out.severity(f.severity),
                t.sentence(&f.message)
            );
        }
    }
    let idle: Vec<_> = a.routes.iter().filter(|r| r.seams == 0).collect();
    if !idle.is_empty() {
        let _ = writeln!(
            s,
            "\n{}",
            out.paint(t.l_routes_without_consumers(), Style::Bold)
        );
        for r in idle {
            let _ = writeln!(
                s,
                "  {} {}  {}  {}:{}",
                r.method, r.route, r.component, r.path, r.line
            );
        }
    }
    if !a.skipped.is_empty() {
        let _ = writeln!(s, "\n{}", out.paint(t.l_skipped(), Style::Bold));
        for k in &a.skipped {
            let reason = serde_json::to_value(k.reason)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            let _ = writeln!(s, "  {}  {}", k.path, out.paint(&reason, Style::Dim));
        }
    }
    if !a.diagnostics.is_empty() {
        let _ = writeln!(s, "\n{}", out.paint(t.l_diagnostics(), Style::Bold));
        for d in &a.diagnostics {
            let _ = writeln!(s, "  {}  {}", d.path, d.message);
        }
    }
    let _ = writeln!(s);
    let mut classes: Vec<(String, usize)> = Vec::new();
    for c in [
        Classification::Verified,
        Classification::Inferred,
        Classification::Parsed,
        Classification::Correlated,
        Classification::Unknown,
    ] {
        let n = a.seams.iter().filter(|x| x.classification == c).count();
        if n > 0 {
            classes.push((t.class(c), n));
        }
    }
    if !classes.is_empty() {
        let _ = writeln!(s, "{}", t.by_class(&classes));
    }
    let count = |sev: Severity| {
        a.seams
            .iter()
            .flat_map(|x| &x.findings)
            .filter(|f| f.severity == sev)
            .count()
    };
    let _ = writeln!(
        s,
        "{}",
        t.by_severity(
            count(Severity::Breaking),
            count(Severity::Risk),
            count(Severity::Info)
        )
    );
    if removed > 0 {
        let _ = writeln!(s, "{}", t.removed_since(removed));
    }
    match stored {
        Some(path) => {
            let _ = writeln!(s, "{}", t.stored(path));
        }
        None => {
            let _ = writeln!(s, "{}", t.not_stored());
        }
    }
    if let Some(first) = a
        .seams
        .iter()
        .find(|x| !x.findings.is_empty())
        .or(a.seams.first())
    {
        let _ = writeln!(
            s,
            "{}",
            out.paint(&t.explain_hint(&first.id.as_str()[..15]), Style::Dim)
        );
    }
    s
}

/// The ledger, `seam seams`.
pub fn ledger(out: &Out, rows: &[LedgerRow], header: &str) -> String {
    let t = &out.text;
    let mut s = String::new();
    let _ = writeln!(s, "{}\n", out.paint(header, Style::Bold));
    for r in rows {
        let class = t.class_word(&r.class);
        let _ = writeln!(s, "{}  {}", out.paint(&r.mechanism, Style::Bold), class);
        let _ = writeln!(
            s,
            "  {}",
            r.key
                .split(" producer=")
                .nth(1)
                .map_or(String::new(), |rest| {
                    let (producer, consumer) = rest.split_once(" consumer=").unwrap_or((rest, ""));
                    format!("{consumer} → {producer}")
                })
        );
        let mut meta = vec![
            r.id.to_string(),
            t.state_word(&r.state),
            t.confidence_word(&r.confidence),
        ];
        let mut counts = Vec::new();
        if r.breaking > 0 {
            counts.push(format!("{} {}", r.breaking, t.severity(Severity::Breaking)));
        }
        if r.risk > 0 {
            counts.push(format!("{} {}", r.risk, t.severity(Severity::Risk)));
        }
        if r.info > 0 {
            counts.push(format!("{} {}", r.info, t.severity(Severity::Info)));
        }
        if !counts.is_empty() {
            meta.push(counts.join(if t.fa() { "، " } else { ", " }));
        }
        let _ = writeln!(s, "  {}", out.paint(&meta.join(" · "), Style::Dim));
    }
    s
}

fn field_type(f: Option<&Field>) -> String {
    match f {
        None => "-".to_owned(),
        Some(f) => {
            let mut text = f.shape.describe();
            if f.nullable {
                text.push_str(" | null");
            }
            if !f.required {
                text.push('?');
            }
            text
        }
    }
}

fn mappings(out: &Out, s: &mut String, title: &str, c: &Comparison) {
    if c.mappings.is_empty() {
        return;
    }
    let t = &out.text;
    let _ = writeln!(s, "\n{}", out.paint(title, Style::Bold));
    let rows: Vec<[String; 5]> = c
        .mappings
        .iter()
        .map(|m| {
            [
                m.json_name.clone(),
                field_type(m.consumer.as_ref()),
                field_type(m.producer.as_ref()),
                field_type(m.contract.as_ref()),
                t.mapping(m.status).to_owned(),
            ]
        })
        .collect();
    let header = [
        t.l_field(),
        t.l_consumer(),
        t.l_producer(),
        &t.l_contract().to_lowercase(),
        t.l_status(),
    ]
    .map(str::to_owned);
    let mut widths = [0usize; 5];
    for row in std::iter::once(&header).chain(&rows) {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    let line = |row: &[String; 5]| {
        row.iter()
            .enumerate()
            .map(|(i, cell)| {
                if i < 4 {
                    format!("{cell:<w$}", w = widths[i])
                } else {
                    cell.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    };
    let _ = writeln!(s, "  {}", out.paint(&line(&header), Style::Dim));
    for row in &rows {
        let _ = writeln!(s, "  {}", line(row));
    }
}

/// The Evidence Passport, `seam explain`.
pub fn passport(
    out: &Out,
    seam: &Seam,
    state: Lifecycle,
    observed_at: &str,
    stale: &[usize],
) -> String {
    let t = &out.text;
    let mut s = String::new();
    let _ = writeln!(s, "{} {}", out.paint(t.l_seam(), Style::Bold), seam.id);
    let _ = writeln!(s, "{}", out.paint(&seam.mechanism.to_string(), Style::Bold));
    let _ = writeln!(s, "{}", out.sides(seam));
    if let Some(c) = &seam.contract {
        let op = c
            .operation_id
            .as_deref()
            .map(|o| format!(" ({o})"))
            .unwrap_or_default();
        let _ = writeln!(s, "{}: {}{op}", t.l_contract(), c.document);
    }
    let labels = [t.l_class(), t.l_confidence(), t.l_state()];
    let width = labels.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let class = if stale.is_empty() {
        out.class(seam.classification)
    } else {
        out.paint(&t.stale_class(seam.classification), Style::Yellow)
    };
    let _ = writeln!(s);
    let _ = writeln!(s, "{:<width$}  {class}", labels[0]);
    let _ = writeln!(
        s,
        "{:<width$}  {}",
        labels[1],
        t.confidence(seam.confidence.level)
    );
    let _ = writeln!(s, "{:<width$}  {}", labels[2], t.state(state));
    let _ = writeln!(s, "{}", out.paint(&t.observed(observed_at), Style::Dim));
    if !stale.is_empty() {
        let _ = writeln!(
            s,
            "{}",
            out.paint(&t.stale_warning(stale.len()), Style::Yellow)
        );
    }
    if !seam.confidence.reasons.is_empty() {
        let _ = writeln!(s, "\n{}", out.paint(t.l_why(), Style::Bold));
        for r in &seam.confidence.reasons {
            let _ = writeln!(s, "  - {}", t.sentence(r));
        }
    }
    let _ = writeln!(s, "\n{}", out.paint(t.l_findings(), Style::Bold));
    if seam.findings.is_empty() {
        let _ = writeln!(s, "  {}", t.l_none());
    }
    for f in &seam.findings {
        let field = f
            .field
            .as_deref()
            .map(|x| format!("  {x}"))
            .unwrap_or_default();
        let _ = writeln!(
            s,
            "  {}  {}{field}",
            out.severity(f.severity),
            t.finding_kind(f.kind)
        );
        let _ = writeln!(s, "    {}", t.sentence(&f.message));
    }
    mappings(out, &mut s, t.l_response_fields(), &seam.response);
    mappings(out, &mut s, t.l_request_fields(), &seam.request);
    let _ = writeln!(s, "\n{}", out.paint(t.l_assumptions(), Style::Bold));
    if seam.assumptions.is_empty() {
        let _ = writeln!(s, "  {}", t.l_none());
    }
    for a in &seam.assumptions {
        let at = a
            .evidence
            .map(|i| format!(" [{}]", i + 1))
            .unwrap_or_default();
        let _ = writeln!(s, "  - {}{at}", t.sentence(&a.statement));
    }
    let _ = writeln!(s, "\n{}", out.paint(t.l_tests(), Style::Bold));
    if seam.tests.is_empty() {
        let _ = writeln!(s, "  {}", t.l_none());
    }
    for test in &seam.tests {
        let _ = writeln!(s, "  {}  {}  {}", t.side(test.side), test.path, test.name);
    }
    let _ = writeln!(s, "\n{}", out.paint(t.l_evidence(), Style::Bold));
    for (i, e) in seam.evidence.iter().enumerate() {
        let mark = if stale.contains(&i) {
            format!("  ({})", t.l_stale())
        } else {
            String::new()
        };
        let _ = writeln!(
            s,
            "  {}. {}  {}:{}{}",
            i + 1,
            out.paint(t.role(e.role), Style::Cyan),
            e.span.path,
            e.span.start.line,
            out.paint(&mark, Style::Yellow)
        );
        let _ = writeln!(s, "     {}", t.sentence(&e.note));
        for line in e.excerpt.lines() {
            let _ = writeln!(s, "     {}", out.paint(&format!("│ {line}"), Style::Dim));
        }
    }
    s
}

pub fn languages(out: &Out, caps: &[Capability]) -> String {
    let t = &out.text;
    let mut s = String::new();
    let _ = writeln!(s, "{}", t.languages_intro());
    let mut current = "";
    let width = caps
        .iter()
        .map(|c| t.support(c.support).chars().count())
        .max()
        .unwrap_or(0);
    for c in caps {
        if c.language != current {
            current = c.language;
            let _ = writeln!(
                s,
                "\n{}",
                out.paint(&language_name(t.lang, c.language), Style::Bold)
            );
        }
        let support = format!("{:<width$}", t.support(c.support));
        let style = match c.support {
            seam_adapters::Support::Read | seam_adapters::Support::Resolved => Style::Green,
            seam_adapters::Support::NotRead => Style::Dim,
        };
        let _ = writeln!(
            s,
            "  {}  {}",
            out.paint(&support, style),
            feature(t.lang, c.feature)
        );
    }
    s
}

pub fn language_name(lang: Lang, en: &str) -> String {
    if lang == Lang::En {
        return en.to_owned();
    }
    match en {
        "TypeScript and JavaScript" => "TypeScript و JavaScript",
        "Manifests" => "فایل‌های مانیفست",
        "Rust, Go, Java, SQL, GraphQL" => "Rust، Go، Java، SQL و GraphQL",
        "All" => "همه",
        other => other,
    }
    .to_owned()
}

pub fn feature(lang: Lang, en: &str) -> String {
    if lang == Lang::En {
        return en.to_owned();
    }
    match en {
        "fetch calls, with the method from a literal init object" => {
            "فراخوانی‌های fetch، با متدی که در شیء تنظیمات به‌صورت ثابت آمده است"
        }
        "axios.get, post, put, patch, delete, head and options" => {
            "axios.get و post و put و patch و delete و head و options"
        }
        "axios instances made with axios.create" => {
            "نمونه‌های axios که با axios.create ساخته می‌شوند"
        }
        "functions that wrap a request and take the path as a parameter" => {
            "تابع‌هایی که یک درخواست را در بر می‌گیرند و مسیر را به‌صورت پارامتر می‌گیرند"
        }
        "template literals and + concatenation in URLs" => "رشته‌های قالبی و الحاق با + در نشانی‌ها",
        "module constants, local or imported" => "ثابت‌های ماژول، محلی یا واردشده",
        "import.meta.env and process.env, with a ?? or || fallback" => {
            "import.meta.env و process.env، با مقدار پیش‌فرض ?? یا ||"
        }
        "environment values set in Docker Compose" => "مقدارهای محیطی تعیین‌شده در Docker Compose",
        "relative imports" => "importهای نسبی",
        "tsconfig path aliases and package imports" => {
            "نام‌های مستعار مسیر در tsconfig و importهای بسته‌ها"
        }
        "interfaces and type aliases, including extends" => {
            "interfaceها و type aliasها، همراه با extends"
        }
        "generic type arguments at call sites" => "آرگومان‌های نوع عمومی در محل فراخوانی",
        "generics other than Array, ReadonlyArray and Promise" => {
            "نوع‌های عمومی به‌جز Array و ReadonlyArray و Promise"
        }
        "intersection, mapped and conditional types" => "نوع‌های اشتراکی، نگاشتی و شرطی",
        "inferred types without an annotation" => "نوع‌های استنباطی بدون اعلان نوع",
        "class methods and object-literal methods" => "متدهای کلاس و متدهای شیء لفظی",
        "it and test blocks in *.test.* and *.spec.* files" => {
            "بلوک‌های it و test در فایل‌های *.test.* و *.spec.*"
        }
        "FastAPI routes on an app or an APIRouter" => "مسیرهای FastAPI روی app یا APIRouter",
        "api_route with a methods list" => "api_route با فهرست methods",
        "router prefixes and include_router across modules" => {
            "پیشوندهای روتر و include_router در میان ماژول‌ها"
        }
        "response_model and return annotations" => "response_model و نوع بازگشتی",
        "request bodies from parameters typed with a model" => {
            "بدنهٔ درخواست از پارامترهایی که نوعشان یک مدل است"
        }
        "Pydantic models, including inheritance between models" => {
            "مدل‌های Pydantic، همراه با ارث‌بری میان مدل‌ها"
        }
        "alias, serialization_alias and validation_alias" => {
            "alias و serialization_alias و validation_alias"
        }
        "alias generators in model_config" => "تولیدکننده‌های alias در model_config",
        "Optional, Union, X | None, list, dict, Literal and Annotated" => {
            "Optional و Union و X | None و list و dict و Literal و Annotated"
        }
        "TestClient requests in test functions" => "درخواست‌های TestClient در تابع‌های آزمون",
        "Flask, Django and other frameworks" => "Flask و Django و چارچوب‌های دیگر",
        "outgoing HTTP calls with httpx or requests" => {
            "فراخوانی‌های HTTP خروجی با httpx یا requests"
        }
        "OpenAPI 3.0 and 3.1 operations in YAML or JSON" => {
            "عملیات OpenAPI 3.0 و 3.1 در YAML یا JSON"
        }
        "local $ref" => "ارجاع‌های $ref محلی",
        "references to other files" => "ارجاع به فایل‌های دیگر",
        "allOf, oneOf, anyOf and nullable" => "allOf و oneOf و anyOf و nullable",
        "the first 2xx response of an operation" => "نخستین پاسخ 2xx هر عملیات",
        "services, build contexts and images" => "سرویس‌ها، زمینه‌های build و imageها",
        "environment as a map or a list" => "environment به‌صورت نگاشت یا فهرست",
        "env_file and ${VAR} interpolation" => "env_file و جایگذاری ${VAR}",
        "package.json, pyproject.toml, Cargo.toml and go.mod names" => {
            "نام‌ها در package.json و pyproject.toml و Cargo.toml و go.mod"
        }
        "any extraction" => "هیچ نوع استخراج",
        "running or importing the analysed code" => "اجرا یا import کردن کد تحلیل‌شده",
        other => other,
    }
    .to_owned()
}
