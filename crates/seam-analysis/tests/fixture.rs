//! The polyglot fixture's documented defects, found from its sources.
//! See fixtures/polyglot/FIXTURE.md for what each one is.

// Test code: a failed unwrap is the test failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};

use seam_analysis::{Analysis, CancelToken, Options, analyse};
use seam_core::{Classification, FindingKind, MappingStatus, Role, Seam, Severity, Side};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/polyglot")
}

fn scan() -> Analysis {
    analyse(&fixture(), &Options::default(), &CancelToken::default()).expect("the fixture scans")
}

fn seam<'a>(a: &'a Analysis, method: &str, route_key: &str) -> &'a Seam {
    a.seams
        .iter()
        .find(|s| s.key.starts_with(&format!("http {method} {route_key} ")))
        .unwrap_or_else(|| {
            panic!(
                "no Seam for {method} {route_key}; found {:?}",
                a.seams.iter().map(|s| &s.key).collect::<Vec<_>>()
            )
        })
}

fn has(s: &Seam, kind: FindingKind, field: Option<&str>) -> bool {
    s.findings
        .iter()
        .any(|f| f.kind == kind && f.field.as_deref() == field)
}

#[test]
fn components_come_from_manifests() {
    let a = scan();
    let names: Vec<_> = a.components.iter().map(|c| c.name.as_str()).collect();
    for expected in ["@ledgerline/web", "ledgerline-api", "ledger"] {
        assert!(
            names.contains(&expected),
            "{expected} missing from {names:?}"
        );
    }
}

#[test]
fn defect_1_the_client_reads_a_field_the_api_renamed() {
    let a = scan();
    let s = seam(&a, "GET", "/accounts/{}");
    assert_eq!(s.classification, Classification::Verified);
    assert_eq!(s.consumer.component, "@ledgerline/web");
    assert_eq!(s.consumer.symbol.as_deref(), Some("fetchAccount"));
    let producer = s.producer.as_ref().expect("a producer");
    assert_eq!(
        (producer.component.as_str(), producer.symbol.as_deref()),
        ("ledgerline-api", Some("get_account"))
    );
    assert_eq!(
        s.contract.as_ref().and_then(|c| c.operation_id.as_deref()),
        Some("getAccount")
    );
    let missing = s
        .findings
        .iter()
        .find(|f| f.kind == FindingKind::MissingField)
        .expect("a missing-field finding");
    assert_eq!(
        (missing.field.as_deref(), missing.severity),
        (Some("name"), Severity::Breaking)
    );
    let display = s
        .response
        .mappings
        .iter()
        .find(|m| m.json_name == "display_name")
        .expect("display_name");
    assert_eq!(display.status, MappingStatus::Unread);
    // The match is proven, not assumed: Compose names the service.
    assert!(s.assumptions.is_empty());
    assert!(
        s.evidence
            .iter()
            .any(|e| e.role == Role::Resolution && e.span.path == "docker-compose.yml")
    );
    assert!(
        s.evidence
            .iter()
            .any(|e| e.role == Role::Resolution && e.span.path == "api/app/main.py")
    );
}

#[test]
fn defect_2_an_outdated_client_calls_a_route_nobody_serves() {
    let a = scan();
    let s = seam(&a, "GET", "/v1/users/{}");
    assert_eq!(s.classification, Classification::Parsed);
    assert!(s.producer.is_none());
    assert!(has(s, FindingKind::NoProvider, None));
}

#[test]
fn defect_5_payments_have_no_tests_on_either_side() {
    let a = scan();
    let payments = seam(&a, "POST", "/payments");
    assert_eq!(payments.classification, Classification::Verified);
    assert!(payments.tests.is_empty());
    assert!(has(payments, FindingKind::Untested, None));
    let accounts = seam(&a, "GET", "/accounts/{}");
    assert!(accounts.tests.iter().any(|t| t.side == Side::Consumer));
    assert!(accounts.tests.iter().any(|t| t.side == Side::Producer));
    assert!(!has(accounts, FindingKind::Untested, None));
}

#[test]
fn the_nullable_receipt_field_and_the_alias_are_judged_correctly() {
    let a = scan();
    let s = seam(&a, "POST", "/payments");
    let nullable = s
        .findings
        .iter()
        .find(|f| f.kind == FindingKind::NullableMismatch)
        .expect("a nullable finding");
    assert_eq!(
        (nullable.field.as_deref(), nullable.severity),
        (Some("ledger_entry_id"), Severity::Risk)
    );
    let alias = s
        .response
        .mappings
        .iter()
        .find(|m| m.json_name == "paymentId")
        .expect("paymentId");
    assert_eq!(alias.status, MappingStatus::Aligned);
    assert_eq!(
        alias.producer.as_ref().map(|f| f.declared_name.as_str()),
        Some("payment_id")
    );
    // The request body is compared too, with the roles flipped.
    assert!(
        s.request
            .mappings
            .iter()
            .any(|m| m.json_name == "currency" && m.status == MappingStatus::Aligned)
    );
    assert!(
        !s.findings
            .iter()
            .any(|f| f.kind == FindingKind::TypeMismatch)
    );
}

#[test]
fn a_path_built_from_runtime_data_is_unknown_not_guessed() {
    let a = scan();
    let s = a
        .seams
        .iter()
        .find(|s| s.key.contains("/search"))
        .expect("the search call is recorded");
    assert_eq!(s.classification, Classification::Unknown);
    assert!(s.producer.is_none());
    assert!(!has(s, FindingKind::NoProvider, None));
}

#[test]
fn every_claim_points_at_lines_that_say_it() {
    let a = scan();
    let root = fixture();
    for s in &a.seams {
        for e in &s.evidence {
            let text = std::fs::read_to_string(root.join(&e.span.path)).expect("cited file exists");
            let first = text
                .lines()
                .nth(e.span.start.line as usize - 1)
                .expect("cited line exists");
            assert!(
                e.excerpt.lines().next().is_some_and(|l| l == first),
                "{}: excerpt does not match line {} of {}",
                s.key,
                e.span.start.line,
                e.span.path
            );
            assert_eq!(
                e.file_hash,
                seam_core::ContentHash::of(text.as_bytes()),
                "{} hash",
                e.span.path
            );
        }
    }
}

#[test]
fn two_scans_give_identical_results() {
    let one = serde_json::to_string(&scan().seams).expect("json");
    let two = serde_json::to_string(&scan().seams).expect("json");
    assert_eq!(one, two);
}

#[test]
fn a_cancelled_scan_returns_nothing() {
    let cancel = CancelToken::default();
    cancel.cancel();
    assert!(matches!(
        analyse(&fixture(), &Options::default(), &cancel),
        Err(seam_analysis::ScanError::Cancelled)
    ));
}
