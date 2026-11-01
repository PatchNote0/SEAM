//! The SEAM domain model and the rules that judge it.
//!
//! Nothing in this crate performs I/O. Adapters produce facts, the analysis
//! crate links them, and the types here say what a Seam is and how its
//! evidence is weighed.

pub mod compat;
pub mod evidence;
pub mod route;
pub mod seam;
pub mod shape;
pub mod span;

pub use compat::{
    Comparison, Direction, FieldMapping, Finding, FindingKind, MappingStatus, Severity, Views,
};
pub use evidence::{ContentHash, Evidence, Method, Role};
pub use route::{HttpMethod, RouteMatch, RouteTemplate, Segment};
pub use seam::{
    Assumption, Classification, Confidence, ConfidenceLevel, ContractRef, Judgement, Lifecycle,
    Mechanism, Participant, Seam, SeamId, Side, TestRef,
};
pub use shape::{Field, ObjectShape, Shape};
pub use span::{Position, Span};
