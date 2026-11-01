//! The Seam: a boundary between two independently built components.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::compat::{Comparison, Finding, FindingKind, Severity};
use crate::evidence::Evidence;
use crate::route::{HttpMethod, RouteTemplate};

/// A stable identifier derived from the Seam's key, so it survives line
/// moves and reformatting.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SeamId(String);

impl SeamId {
    pub fn from_key(key: &str) -> SeamId {
        let digest = blake3::hash(key.as_bytes());
        let bytes = digest.as_bytes();
        // 20 base32 characters carry 100 bits, read from the first 13 bytes.
        let mut bits: u128 = 0;
        for &byte in &bytes[..13] {
            bits = (bits << 8) | u128::from(byte);
        }
        bits >>= 4;
        let mut body = [0u8; 20];
        for slot in body.iter_mut().rev() {
            *slot = BASE32[(bits & 0x1f) as usize];
            bits >>= 5;
        }
        SeamId(format!("sm_{}", String::from_utf8_lossy(&body)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Accepts any id this version could have produced.
    pub fn parse(text: &str) -> Option<SeamId> {
        let body = text.strip_prefix("sm_")?;
        let ok = body.len() == 20 && body.bytes().all(|b| BASE32.contains(&b));
        ok.then(|| SeamId(text.to_owned()))
    }
}

impl fmt::Display for SeamId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lower-case Crockford base32: no `i`, `l`, `o` or `u` to misread.
const BASE32: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Mechanism {
    Http {
        method: HttpMethod,
        route: RouteTemplate,
    },
}

impl fmt::Display for Mechanism {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mechanism::Http { method, route } => write!(f, "{method} {route}"),
        }
    }
}

/// One side of a Seam.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Participant {
    pub component: String,
    /// The function or handler on this side, when there is one.
    pub symbol: Option<String>,
    /// Indexes into [`Seam::evidence`] of the records that establish this side.
    pub evidence: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractRef {
    /// Path of the contract document.
    pub document: String,
    pub operation_id: Option<String>,
    /// Index into [`Seam::evidence`].
    pub evidence: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Classification {
    Verified,
    Parsed,
    Inferred,
    Correlated,
    Unknown,
}

impl fmt::Display for Classification {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Classification::Verified => "VERIFIED",
            Classification::Parsed => "PARSED",
            Classification::Inferred => "INFERRED",
            Classification::Correlated => "CORRELATED",
            Classification::Unknown => "UNKNOWN",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConfidenceLevel {
    Strong,
    Moderate,
    Weak,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confidence {
    pub level: ConfidenceLevel,
    pub reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assumption {
    /// A sentence a reviewer can check, naming what was not known.
    pub statement: String,
    /// The adapter capability whose absence forced the assumption, if any.
    pub missing_capability: Option<String>,
    /// Index into [`Seam::evidence`] of the code that needed the assumption.
    pub evidence: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRef {
    pub path: String,
    pub name: String,
    pub side: Side,
    /// Index into [`Seam::evidence`].
    pub evidence: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Consumer,
    Producer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lifecycle {
    Discovered,
    Reconfirmed,
    Changed,
    PotentiallyIncompatible,
    Unresolved,
    Removed,
}

impl fmt::Display for Lifecycle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Lifecycle::Discovered => "discovered",
            Lifecycle::Reconfirmed => "reconfirmed",
            Lifecycle::Changed => "changed",
            Lifecycle::PotentiallyIncompatible => "potentially-incompatible",
            Lifecycle::Unresolved => "unresolved",
            Lifecycle::Removed => "removed",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seam {
    pub id: SeamId,
    pub key: String,
    pub mechanism: Mechanism,
    pub consumer: Participant,
    pub producer: Option<Participant>,
    pub contract: Option<ContractRef>,
    pub request: Comparison,
    pub response: Comparison,
    /// Every finding on the Seam, including those from `request` and
    /// `response`, sorted by severity.
    pub findings: Vec<Finding>,
    pub classification: Classification,
    pub confidence: Confidence,
    pub assumptions: Vec<Assumption>,
    pub evidence: Vec<Evidence>,
    pub tests: Vec<TestRef>,
    /// Changes when the mechanism, the shapes or the contract change; used to
    /// tell `changed` from `reconfirmed`.
    pub fingerprint: String,
}

/// What the linker learned about one candidate Seam, reduced to the facts
/// that decide its class.
#[derive(Clone, Copy, Debug)]
pub struct Judgement {
    /// False when the consumer's target cannot be determined from source.
    pub consumer_resolved: bool,
    /// How many producer routes the consumer could reach after every
    /// deterministic narrowing step.
    pub producer_candidates: usize,
    pub assumptions: usize,
}

/// Classifies by the weakest link. Deterministic resolution does not lower
/// the class; assumptions do (ADR 0006).
pub fn classify(judgement: Judgement) -> (Classification, ConfidenceLevel) {
    if !judgement.consumer_resolved {
        return (Classification::Unknown, ConfidenceLevel::Weak);
    }
    match judgement.producer_candidates {
        0 => (Classification::Parsed, ConfidenceLevel::Weak),
        1 if judgement.assumptions == 0 => (Classification::Verified, ConfidenceLevel::Strong),
        1 => (Classification::Inferred, ConfidenceLevel::Moderate),
        _ => (Classification::Inferred, ConfidenceLevel::Weak),
    }
}

/// Decides the lifecycle state of a Seam that is present in this snapshot,
/// given the fingerprint it had in the previous one. Absent Seams are
/// `Removed`, which the store decides because only it knows the history.
pub fn lifecycle(previous_fingerprint: Option<&str>, seam: &Seam) -> Lifecycle {
    if seam.findings.iter().any(is_incompatibility) {
        return Lifecycle::PotentiallyIncompatible;
    }
    if seam.producer.is_none() || seam.classification == Classification::Unknown {
        return Lifecycle::Unresolved;
    }
    match previous_fingerprint {
        None => Lifecycle::Discovered,
        Some(previous) if previous != seam.fingerprint => Lifecycle::Changed,
        Some(_) => Lifecycle::Reconfirmed,
    }
}

/// The compatibility findings that make a Seam potentially incompatible.
pub fn is_incompatibility(finding: &Finding) -> bool {
    matches!(
        finding.kind,
        FindingKind::MissingField
            | FindingKind::TypeMismatch
            | FindingKind::NullableMismatch
            | FindingKind::ContractDrift
    ) && matches!(finding.severity, Severity::Breaking | Severity::Risk)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compat::Comparison;

    fn seam(findings: Vec<Finding>, producer: bool, classification: Classification) -> Seam {
        let key = "http GET /accounts/{} producer=api consumer=web".to_owned();
        Seam {
            id: SeamId::from_key(&key),
            key,
            mechanism: Mechanism::Http {
                method: HttpMethod::Get,
                route: RouteTemplate::parse("/accounts/{id}"),
            },
            consumer: Participant {
                component: "web".into(),
                symbol: None,
                evidence: vec![],
            },
            producer: producer.then(|| Participant {
                component: "api".into(),
                symbol: None,
                evidence: vec![],
            }),
            contract: None,
            request: Comparison::default(),
            response: Comparison::default(),
            findings,
            classification,
            confidence: Confidence {
                level: ConfidenceLevel::Strong,
                reasons: vec![],
            },
            assumptions: vec![],
            evidence: vec![],
            tests: vec![],
            fingerprint: "f1".into(),
        }
    }

    fn finding(kind: FindingKind, severity: Severity) -> Finding {
        Finding {
            kind,
            severity,
            field: None,
            message: String::new(),
        }
    }

    #[test]
    fn ids_are_stable_short_and_key_derived() {
        let a = SeamId::from_key("http GET /accounts/{} producer=api consumer=web");
        let b = SeamId::from_key("http GET /accounts/{} producer=api consumer=web");
        let c = SeamId::from_key("http GET /accounts/{} producer=ledger consumer=web");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.as_str().len(), 23);
        assert!(a.as_str().starts_with("sm_"));
        assert_eq!(SeamId::parse(a.as_str()), Some(a.clone()));
        assert_eq!(SeamId::parse("sm_not-an-id"), None);
    }

    #[test]
    fn a_chain_without_assumptions_is_verified_and_strong() {
        let j = Judgement {
            consumer_resolved: true,
            producer_candidates: 1,
            assumptions: 0,
        };
        assert_eq!(
            classify(j),
            (Classification::Verified, ConfidenceLevel::Strong)
        );
    }

    #[test]
    fn any_assumption_makes_it_inferred() {
        let j = Judgement {
            consumer_resolved: true,
            producer_candidates: 1,
            assumptions: 1,
        };
        assert_eq!(
            classify(j),
            (Classification::Inferred, ConfidenceLevel::Moderate)
        );
    }

    #[test]
    fn several_candidates_are_inferred_and_weak() {
        let j = Judgement {
            consumer_resolved: true,
            producer_candidates: 2,
            assumptions: 0,
        };
        assert_eq!(
            classify(j),
            (Classification::Inferred, ConfidenceLevel::Weak)
        );
    }

    #[test]
    fn a_missing_producer_is_parsed_and_an_unresolvable_target_unknown() {
        let missing = Judgement {
            consumer_resolved: true,
            producer_candidates: 0,
            assumptions: 0,
        };
        assert_eq!(
            classify(missing),
            (Classification::Parsed, ConfidenceLevel::Weak)
        );
        let dynamic = Judgement {
            consumer_resolved: false,
            producer_candidates: 0,
            assumptions: 0,
        };
        assert_eq!(
            classify(dynamic),
            (Classification::Unknown, ConfidenceLevel::Weak)
        );
    }

    #[test]
    fn lifecycle_follows_the_documented_priority() {
        let healthy = seam(vec![], true, Classification::Verified);
        assert_eq!(lifecycle(None, &healthy), Lifecycle::Discovered);
        assert_eq!(lifecycle(Some("f1"), &healthy), Lifecycle::Reconfirmed);
        assert_eq!(lifecycle(Some("f0"), &healthy), Lifecycle::Changed);

        let broken = seam(
            vec![finding(FindingKind::MissingField, Severity::Breaking)],
            true,
            Classification::Verified,
        );
        assert_eq!(
            lifecycle(Some("f0"), &broken),
            Lifecycle::PotentiallyIncompatible
        );

        let orphan = seam(
            vec![finding(FindingKind::NoProvider, Severity::Risk)],
            false,
            Classification::Parsed,
        );
        assert_eq!(lifecycle(None, &orphan), Lifecycle::Unresolved);

        let untested = seam(
            vec![finding(FindingKind::Untested, Severity::Info)],
            true,
            Classification::Verified,
        );
        assert_eq!(lifecycle(Some("f1"), &untested), Lifecycle::Reconfirmed);
    }
}
