//! Reports: JSON for machines (always English) and Markdown for people.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use seam_analysis::{Analysis, ComponentSummary, Diagnostic, RouteSummary, Skipped};
use seam_core::{Classification, Comparison, Field, Lifecycle, Seam, SeamId, Severity};
use serde::Serialize;

use crate::i18n::Text;

/// Bumped whenever a field is removed or changes meaning.
pub const SCHEMA: &str = "seam-report/1";

#[derive(Serialize)]
pub struct Report<'a> {
    pub schema: &'static str,
    pub engine_version: &'a str,
    pub generated_at: &'a str,
    pub root: &'a str,
    pub duration_ms: u64,
    pub summary: Summary,
    pub components: &'a [ComponentSummary],
    pub routes: &'a [RouteSummary],
    pub seams: Vec<ReportSeam<'a>>,
    pub skipped: &'a [Skipped],
    pub diagnostics: &'a [Diagnostic],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recorded: Option<Recorded>,
}

#[derive(Serialize)]
pub struct Recorded {
    pub store: String,
    pub snapshot: i64,
    pub removed: Vec<String>,
}

#[derive(Serialize)]
pub struct ReportSeam<'a> {
    #[serde(flatten)]
    pub seam: &'a Seam,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<Lifecycle>,
}

#[derive(Serialize)]
pub struct Summary {
    pub files: usize,
    pub components: usize,
    pub routes: usize,
    pub seams: usize,
    pub classes: BTreeMap<Classification, usize>,
    pub findings: BTreeMap<Severity, usize>,
}

pub fn report<'a>(
    a: &'a Analysis,
    states: &BTreeMap<SeamId, Lifecycle>,
    generated_at: &'a str,
    recorded: Option<Recorded>,
) -> Report<'a> {
    let mut classes = BTreeMap::new();
    let mut findings = BTreeMap::new();
    for s in &a.seams {
        *classes.entry(s.classification).or_insert(0) += 1;
        for f in &s.findings {
            *findings.entry(f.severity).or_insert(0) += 1;
        }
    }
    Report {
        schema: SCHEMA,
        engine_version: a.engine_version,
        generated_at,
        root: &a.root,
        duration_ms: a.duration_ms,
        summary: Summary {
            files: a.files.len(),
            components: a.components.len(),
            routes: a.routes.len(),
            seams: a.seams.len(),
            classes,
            findings,
        },
        components: &a.components,
        routes: &a.routes,
        seams: a
            .seams
            .iter()
            .map(|s| ReportSeam {
                seam: s,
                state: states.get(&s.id).copied(),
            })
            .collect(),
        skipped: &a.skipped,
        diagnostics: &a.diagnostics,
        recorded,
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn cap(text: &str, fa: bool) -> String {
    if fa {
        return text.to_owned();
    }
    let mut chars = text.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

fn field_type(f: Option<&Field>) -> String {
    f.map_or_else(
        || "-".to_owned(),
        |f| {
            let mut text = f.shape.describe();
            if f.nullable {
                text.push_str(" | null");
            }
            if !f.required {
                text.push('?');
            }
            text
        },
    )
}

fn table(s: &mut String, t: &Text, title: &str, c: &Comparison) {
    if c.mappings.is_empty() {
        return;
    }
    let fa = t.fa();
    let _ = writeln!(s, "**{title}**\n");
    let _ = writeln!(
        s,
        "| {} | {} | {} | {} | {} |\n|---|---|---|---|---|",
        cap(t.l_field(), fa),
        cap(t.l_consumer(), fa),
        cap(t.l_producer(), fa),
        t.l_contract(),
        cap(t.l_status(), fa)
    );
    for m in &c.mappings {
        let _ = writeln!(
            s,
            "| `{}` | {} | {} | {} | {} |",
            m.json_name,
            cell(&field_type(m.consumer.as_ref())),
            cell(&field_type(m.producer.as_ref())),
            cell(&field_type(m.contract.as_ref())),
            t.mapping(m.status)
        );
    }
    let _ = writeln!(s);
}

/// A Markdown report. In Persian the text runs right to left and code
/// excerpts stay left to right.
pub fn markdown(
    t: &Text,
    a: &Analysis,
    states: &BTreeMap<SeamId, Lifecycle>,
    generated_at: &str,
) -> String {
    let fa = t.fa();
    let mut s = String::new();
    if fa {
        let _ = writeln!(s, "<div dir=\"rtl\" lang=\"fa\">\n");
    }
    let name = std::path::Path::new(&a.root)
        .file_name()
        .map_or_else(|| a.root.clone(), |n| n.to_string_lossy().into_owned());
    let _ = writeln!(s, "# {}\n", t.report_title(&name));
    let _ = writeln!(
        s,
        "{}\n",
        t.report_generated(a.engine_version, generated_at, &a.root)
    );
    let _ = writeln!(
        s,
        "{}\n",
        t.counts(
            a.files.len(),
            a.components.len(),
            a.routes.len(),
            a.seams.len()
        )
    );
    let mut classes = Vec::new();
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
    let count = |sev: Severity| {
        a.seams
            .iter()
            .flat_map(|x| &x.findings)
            .filter(|f| f.severity == sev)
            .count()
    };
    let _ = writeln!(
        s,
        "- {}\n- {}\n",
        t.by_class(&classes),
        t.by_severity(
            count(Severity::Breaking),
            count(Severity::Risk),
            count(Severity::Info)
        )
    );
    if a.seams.is_empty() {
        let _ = writeln!(s, "{}\n", t.no_seams());
    }
    for seam in &a.seams {
        let _ = writeln!(
            s,
            "## {} · {}\n",
            seam.mechanism,
            t.class(seam.classification)
        );
        let side = |component: &str, symbol: &Option<String>| match symbol {
            Some(x) => format!("{component} (`{x}`)"),
            None => component.to_owned(),
        };
        let _ = writeln!(
            s,
            "- **{}:** {}",
            cap(t.l_consumer(), fa),
            side(&seam.consumer.component, &seam.consumer.symbol)
        );
        let producer = seam.producer.as_ref().map_or_else(
            || t.l_no_producer().to_owned(),
            |p| side(&p.component, &p.symbol),
        );
        let _ = writeln!(s, "- **{}:** {producer}", cap(t.l_producer(), fa));
        if let Some(c) = &seam.contract {
            let op = c
                .operation_id
                .as_deref()
                .map(|o| format!(" ({o})"))
                .unwrap_or_default();
            let _ = writeln!(s, "- **{}:** `{}`{op}", t.l_contract(), c.document);
        }
        if let Some(state) = states.get(&seam.id) {
            let _ = writeln!(s, "- **{}:** {}", t.l_state(), t.state(*state));
        }
        let _ = writeln!(
            s,
            "- **{}:** {}",
            t.l_confidence(),
            t.confidence(seam.confidence.level)
        );
        let _ = writeln!(
            s,
            "- **{}:** `{}`\n",
            if fa { "شناسه" } else { "Id" },
            seam.id
        );
        if !seam.confidence.reasons.is_empty() {
            let _ = writeln!(s, "**{}**\n", t.l_why());
            for r in &seam.confidence.reasons {
                let _ = writeln!(s, "- {}", t.sentence(r));
            }
            let _ = writeln!(s);
        }
        if !seam.findings.is_empty() {
            let _ = writeln!(s, "**{}**\n", t.l_findings());
            for f in &seam.findings {
                let comma = if fa { "،" } else { "," };
                let _ = writeln!(
                    s,
                    "- **{}**{comma} {}: {}",
                    t.severity(f.severity),
                    t.finding_kind(f.kind),
                    t.sentence(&f.message)
                );
            }
            let _ = writeln!(s);
        }
        table(&mut s, t, t.l_response_fields(), &seam.response);
        table(&mut s, t, t.l_request_fields(), &seam.request);
        if !seam.assumptions.is_empty() {
            let _ = writeln!(s, "**{}**\n", t.l_assumptions());
            for x in &seam.assumptions {
                let _ = writeln!(s, "- {}", t.sentence(&x.statement));
            }
            let _ = writeln!(s);
        }
        if !seam.tests.is_empty() {
            let _ = writeln!(s, "**{}**\n", t.l_tests());
            for test in &seam.tests {
                let _ = writeln!(s, "- {}: `{}`, {}", t.side(test.side), test.path, test.name);
            }
            let _ = writeln!(s);
        }
        let _ = writeln!(
            s,
            "<details><summary>{} ({})</summary>\n",
            t.l_evidence(),
            seam.evidence.len()
        );
        for (i, e) in seam.evidence.iter().enumerate() {
            let _ = writeln!(
                s,
                "{}. {}, `{}:{}`: {}\n",
                i + 1,
                t.role(e.role),
                e.span.path,
                e.span.start.line,
                t.sentence(&e.note)
            );
            if fa {
                let _ = writeln!(
                    s,
                    "<pre dir=\"ltr\"><code>{}</code></pre>\n",
                    html(&e.excerpt)
                );
            } else {
                let fence = if e.excerpt.contains("```") {
                    "````"
                } else {
                    "```"
                };
                let _ = writeln!(s, "{fence}\n{}\n{fence}\n", e.excerpt);
            }
        }
        let _ = writeln!(s, "</details>\n");
    }
    if fa {
        let _ = writeln!(s, "</div>");
    }
    s
}
