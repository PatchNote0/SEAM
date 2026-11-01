//! Field-level comparison of the data that crosses a Seam.
//!
//! Comparisons are directional. One side writes the JSON and the other reads
//! it: for a response the producer writes and the consumer reads, for a
//! request body it is the other way round. A field the reader requires and
//! the writer never sends is breaking; a field the writer sends and the
//! reader ignores is not.

use serde::{Deserialize, Serialize};

use crate::shape::{Field, ObjectShape, Shape};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FindingKind {
    MissingField,
    TypeMismatch,
    NullableMismatch,
    ContractDrift,
    NoProvider,
    Untested,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    Breaking,
    Risk,
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub kind: FindingKind,
    pub severity: Severity,
    /// The JSON field, dotted for nested fields: `items[].amount`.
    pub field: Option<String>,
    pub message: String,
}

/// Which way the data flows across the Seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    /// The producer writes, the consumer reads.
    Response,
    /// The consumer writes, the producer reads.
    Request,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MappingStatus {
    Aligned,
    /// The reader requires it and the writer never sends it.
    Missing,
    /// The reader may do without it and the writer never sends it.
    OptionalUnsent,
    /// The writer sends it and the reader ignores it.
    Unread,
    TypeMismatch,
    NullableMismatch,
    /// Both sides agree but the contract says otherwise, or is silent.
    ContractDrift,
    /// At least one side's type could not be resolved.
    Indeterminate,
}

/// One JSON field seen from the three places that describe it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldMapping {
    pub json_name: String,
    pub consumer: Option<Field>,
    pub producer: Option<Field>,
    pub contract: Option<Field>,
    pub status: MappingStatus,
}

/// The shapes known for one direction of a Seam. Any of them may be missing.
#[derive(Clone, Copy, Debug, Default)]
pub struct Views<'a> {
    pub consumer: Option<&'a Shape>,
    pub producer: Option<&'a Shape>,
    pub contract: Option<&'a Shape>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comparison {
    pub mappings: Vec<FieldMapping>,
    pub findings: Vec<Finding>,
}

/// Whether a written shape can be read as the reader's shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fit {
    Yes,
    No(String),
    /// One side is `any`, unresolved or unknown; no judgement is possible.
    Unknown,
}

pub fn fits(read: &Shape, written: &Shape) -> Fit {
    use Shape::*;
    match (read, written) {
        (Any | Unknown { .. } | Named { .. }, _) | (_, Any | Unknown { .. } | Named { .. }) => {
            Fit::Unknown
        }
        (_, Union { variants }) => {
            let mut undecided = false;
            for variant in variants {
                match fits(read, variant) {
                    Fit::No(_) => {
                        return Fit::No(format!(
                            "it may be {}, which does not fit {}",
                            variant.describe(),
                            read.describe()
                        ));
                    }
                    Fit::Unknown => undecided = true,
                    Fit::Yes => {}
                }
            }
            if undecided { Fit::Unknown } else { Fit::Yes }
        }
        (Union { variants }, _) => {
            let mut undecided = false;
            for variant in variants {
                match fits(variant, written) {
                    Fit::Yes => return Fit::Yes,
                    Fit::Unknown => undecided = true,
                    Fit::No(_) => {}
                }
            }
            if undecided {
                Fit::Unknown
            } else {
                Fit::No(format!(
                    "{} is not one of {}",
                    written.describe(),
                    read.describe()
                ))
            }
        }
        (Number, Integer) => Fit::Yes,
        (Integer, Number) => {
            Fit::No("a number may carry a fraction where an integer is expected".into())
        }
        (Array { items: r }, Array { items: w }) => match fits(r, w) {
            Fit::No(why) => Fit::No(format!("array items differ: {why}")),
            other => other,
        },
        // Objects are compared field by field by `compare`, which descends
        // into them; at this level any object fits any object.
        (Object(_), Object(_)) => Fit::Yes,
        (String, String) | (Integer, Integer) | (Boolean, Boolean) | (Null, Null) => Fit::Yes,
        (r, w) => Fit::No(format!("{} does not fit {}", w.describe(), r.describe())),
    }
}

/// Compares what the consumer, the producer and the contract say about the
/// data flowing in `direction`.
pub fn compare(direction: Direction, views: Views<'_>) -> Comparison {
    let mut out = Comparison::default();
    compare_at(direction, "", views, &mut out, Rows::Record, 0);
    out.mappings.sort_by(|a, b| a.json_name.cmp(&b.json_name));
    out.findings
        .sort_by(|a, b| (&a.field, a.kind).cmp(&(&b.field, b.kind)));
    out
}

/// Nesting deeper than this is not compared; real payloads rarely need it
/// and recursive types would otherwise never end.
const MAX_DEPTH: usize = 4;

/// Nested objects contribute findings but not mapping rows: the mapping
/// table lists the fields of the payload itself, and a nested finding names
/// its full path.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rows {
    Record,
    Skip,
}

fn array_items(shape: Option<&Shape>) -> Option<&Shape> {
    match shape {
        Some(Shape::Array { items }) => Some(items),
        _ => None,
    }
}

fn as_object(shape: Option<&Shape>) -> Option<&ObjectShape> {
    match shape {
        Some(Shape::Object(object)) => Some(object),
        _ => None,
    }
}

fn compare_at(
    direction: Direction,
    path: &str,
    views: Views<'_>,
    out: &mut Comparison,
    rows: Rows,
    depth: usize,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let known = [views.consumer, views.producer, views.contract];
    if known.iter().flatten().count() > 0
        && known
            .iter()
            .flatten()
            .all(|s| matches!(s, Shape::Array { .. }))
    {
        let items = Views {
            consumer: array_items(views.consumer),
            producer: array_items(views.producer),
            contract: array_items(views.contract),
        };
        compare_at(direction, &join(path, "[]"), items, out, rows, depth + 1);
        return;
    }
    let consumer = as_object(views.consumer);
    let producer = as_object(views.producer);
    let contract = as_object(views.contract);
    if consumer.is_none() && producer.is_none() && contract.is_none() {
        return;
    }
    let mut names: Vec<&str> = [consumer, producer, contract]
        .into_iter()
        .flatten()
        .flat_map(|o| o.fields.iter().map(|f| f.json_name.as_str()))
        .collect();
    names.sort_unstable();
    names.dedup();

    let roles = Roles {
        direction,
        contract_stands_in: producer.is_none(),
    };
    let both_known = roles.reader_known(consumer, producer, contract)
        && roles.writer_known(consumer, producer, contract);
    for name in names {
        let c = consumer.and_then(|o| o.field(name));
        let p = producer.and_then(|o| o.field(name));
        let k = contract.and_then(|o| o.field(name));
        let field_path = join(path, name);
        let (reader, writer) = roles.pick(c, p, k);
        let mut status = if both_known {
            judge_field(&roles, &field_path, reader, writer, out)
        } else {
            MappingStatus::Indeterminate
        };
        if let (Some(p_obj), Some(k_obj)) = (producer, contract) {
            let drifted = drift(
                direction,
                &field_path,
                p_obj.field(name),
                k_obj.field(name),
                out,
            );
            if drifted
                && matches!(
                    status,
                    MappingStatus::Aligned | MappingStatus::Unread | MappingStatus::Indeterminate
                )
            {
                status = MappingStatus::ContractDrift;
            }
        }
        if let (Some(r), Some(w)) = (reader, writer) {
            let nested = Views {
                consumer: c.map(|f| &f.shape),
                producer: p.map(|f| &f.shape),
                contract: k.map(|f| &f.shape),
            };
            if fits(&r.shape, &w.shape) == Fit::Yes && holds_objects(nested) {
                compare_at(direction, &field_path, nested, out, Rows::Skip, depth + 1);
            }
        }
        if rows == Rows::Record {
            out.mappings.push(FieldMapping {
                json_name: field_path,
                consumer: c.cloned(),
                producer: p.cloned(),
                contract: k.cloned(),
                status,
            });
        }
    }
}

fn holds_objects(views: Views<'_>) -> bool {
    [views.consumer, views.producer, views.contract]
        .into_iter()
        .flatten()
        .any(|s| match s {
            Shape::Object(_) => true,
            Shape::Array { items } => matches!(**items, Shape::Object(_)),
            _ => false,
        })
}

/// Who reads and who writes, and who stands in for a missing writer.
struct Roles {
    direction: Direction,
    /// In a response with no known producer, the contract describes what is
    /// written; in a request with no known producer, what is read.
    contract_stands_in: bool,
}

impl Roles {
    fn pick<'f>(
        &self,
        c: Option<&'f Field>,
        p: Option<&'f Field>,
        k: Option<&'f Field>,
    ) -> (Option<&'f Field>, Option<&'f Field>) {
        let producer_side = if self.contract_stands_in { k } else { p };
        match self.direction {
            Direction::Response => (c, producer_side),
            Direction::Request => (producer_side, c),
        }
    }

    fn producer_known(
        &self,
        producer: Option<&ObjectShape>,
        contract: Option<&ObjectShape>,
    ) -> bool {
        if self.contract_stands_in {
            contract.is_some()
        } else {
            producer.is_some()
        }
    }

    fn reader_known(
        &self,
        consumer: Option<&ObjectShape>,
        producer: Option<&ObjectShape>,
        contract: Option<&ObjectShape>,
    ) -> bool {
        match self.direction {
            Direction::Response => consumer.is_some(),
            Direction::Request => self.producer_known(producer, contract),
        }
    }

    fn writer_known(
        &self,
        consumer: Option<&ObjectShape>,
        producer: Option<&ObjectShape>,
        contract: Option<&ObjectShape>,
    ) -> bool {
        match self.direction {
            Direction::Response => self.producer_known(producer, contract),
            Direction::Request => consumer.is_some(),
        }
    }

    fn producer_name(&self) -> &'static str {
        if self.contract_stands_in {
            "contract"
        } else {
            "producer"
        }
    }

    fn reader(&self) -> &'static str {
        match self.direction {
            Direction::Response => "consumer",
            Direction::Request => self.producer_name(),
        }
    }

    fn writer(&self) -> &'static str {
        match self.direction {
            Direction::Response => self.producer_name(),
            Direction::Request => "consumer",
        }
    }

    fn payload(&self) -> &'static str {
        match self.direction {
            Direction::Response => "response",
            Direction::Request => "request",
        }
    }
}

fn judge_field(
    roles: &Roles,
    path: &str,
    reader: Option<&Field>,
    writer: Option<&Field>,
    out: &mut Comparison,
) -> MappingStatus {
    match (reader, writer) {
        (Some(r), None) if r.required => {
            let verb = if roles.direction == Direction::Response {
                "reads"
            } else {
                "requires"
            };
            out.findings.push(Finding {
                kind: FindingKind::MissingField,
                severity: Severity::Breaking,
                field: Some(path.to_owned()),
                message: format!(
                    "The {} {verb} `{path}`; the {}'s {} has no field `{path}`.",
                    roles.reader(),
                    roles.writer(),
                    roles.payload()
                ),
            });
            MappingStatus::Missing
        }
        (Some(_), None) => MappingStatus::OptionalUnsent,
        (None, Some(_)) => MappingStatus::Unread,
        (None, None) => MappingStatus::Indeterminate,
        (Some(r), Some(w)) => match fits(&r.shape, &w.shape) {
            Fit::No(_) => {
                out.findings.push(Finding {
                    kind: FindingKind::TypeMismatch,
                    severity: Severity::Breaking,
                    field: Some(path.to_owned()),
                    message: format!(
                        "The {} reads `{path}` as {}; the {} sends {}.",
                        roles.reader(),
                        r.shape.describe(),
                        roles.writer(),
                        w.shape.describe()
                    ),
                });
                MappingStatus::TypeMismatch
            }
            Fit::Unknown => MappingStatus::Indeterminate,
            Fit::Yes if w.nullable && !r.nullable => {
                out.findings.push(Finding {
                    kind: FindingKind::NullableMismatch,
                    severity: Severity::Risk,
                    field: Some(path.to_owned()),
                    message: format!(
                        "The {} may send null for `{path}`; the {}'s type does not allow null.",
                        roles.writer(),
                        roles.reader()
                    ),
                });
                MappingStatus::NullableMismatch
            }
            Fit::Yes => MappingStatus::Aligned,
        },
    }
}

/// Reports where the contract and the implementation disagree. Returns true
/// when they do.
fn drift(
    direction: Direction,
    path: &str,
    implemented: Option<&Field>,
    declared: Option<&Field>,
    out: &mut Comparison,
) -> bool {
    let verb = match direction {
        Direction::Response => "send",
        Direction::Request => "read",
    };
    let message = match (implemented, declared) {
        (Some(_), None) => {
            format!("The producer {verb}s `{path}`; the contract does not declare it.")
        }
        (None, Some(_)) => {
            format!("The contract declares `{path}`; the producer does not {verb} it.")
        }
        (Some(i), Some(d)) => match fits(&d.shape, &i.shape) {
            Fit::No(_) => format!(
                "The contract types `{path}` as {}; the producer has {}.",
                d.shape.describe(),
                i.shape.describe()
            ),
            _ if i.nullable && !d.nullable => {
                format!("The producer allows null for `{path}`; the contract does not.")
            }
            _ => return false,
        },
        (None, None) => return false,
    };
    out.findings.push(Finding {
        kind: FindingKind::ContractDrift,
        severity: Severity::Risk,
        field: Some(path.to_owned()),
        message,
    });
    true
}

fn join(path: &str, segment: &str) -> String {
    if path.is_empty() {
        segment.to_owned()
    } else if segment == "[]" {
        format!("{path}[]")
    } else {
        format!("{path}.{segment}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(name: &str, shape: Shape) -> Field {
        Field {
            json_name: name.into(),
            declared_name: name.into(),
            shape,
            required: true,
            nullable: false,
            span: None,
        }
    }

    fn object(name: &str, fields: Vec<Field>) -> Shape {
        Shape::Object(ObjectShape {
            name: Some(name.into()),
            fields,
        })
    }

    fn kinds(c: &Comparison) -> Vec<(FindingKind, Severity, Option<String>)> {
        c.findings
            .iter()
            .map(|f| (f.kind, f.severity, f.field.clone()))
            .collect()
    }

    #[test]
    fn integers_fit_numbers_but_numbers_do_not_fit_integers() {
        assert_eq!(fits(&Shape::Number, &Shape::Integer), Fit::Yes);
        assert!(matches!(fits(&Shape::Integer, &Shape::Number), Fit::No(_)));
        assert!(matches!(fits(&Shape::String, &Shape::Boolean), Fit::No(_)));
        assert_eq!(fits(&Shape::Any, &Shape::Boolean), Fit::Unknown);
        assert_eq!(
            fits(&Shape::String, &Shape::Named { name: "X".into() }),
            Fit::Unknown
        );
    }

    #[test]
    fn arrays_and_unions_are_compared_element_wise() {
        assert_eq!(
            fits(&Shape::array(Shape::Number), &Shape::array(Shape::Integer)),
            Fit::Yes
        );
        assert!(matches!(
            fits(&Shape::array(Shape::String), &Shape::String),
            Fit::No(_)
        ));
        let reads_either = Shape::Union {
            variants: vec![Shape::String, Shape::Integer],
        };
        assert_eq!(fits(&reads_either, &Shape::Integer), Fit::Yes);
        let writes_either = Shape::Union {
            variants: vec![Shape::String, Shape::Integer],
        };
        assert!(matches!(fits(&Shape::String, &writes_either), Fit::No(_)));
    }

    #[test]
    fn a_required_field_the_producer_never_sends_is_breaking() {
        let consumer = object(
            "Account",
            vec![field("id", Shape::Number), field("name", Shape::String)],
        );
        let producer = object(
            "AccountOut",
            vec![
                field("id", Shape::Integer),
                field("display_name", Shape::String),
            ],
        );
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        assert_eq!(
            kinds(&c),
            vec![(
                FindingKind::MissingField,
                Severity::Breaking,
                Some("name".into())
            )]
        );
        let display = c
            .mappings
            .iter()
            .find(|m| m.json_name == "display_name")
            .map(|m| m.status);
        assert_eq!(display, Some(MappingStatus::Unread));
    }

    #[test]
    fn an_optional_unsent_field_is_not_a_finding() {
        let mut nickname = field("nickname", Shape::String);
        nickname.required = false;
        let consumer = object("Account", vec![field("id", Shape::Number), nickname]);
        let producer = object("AccountOut", vec![field("id", Shape::Integer)]);
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        assert!(c.findings.is_empty());
        assert_eq!(
            c.mappings
                .iter()
                .find(|m| m.json_name == "nickname")
                .map(|m| m.status),
            Some(MappingStatus::OptionalUnsent)
        );
    }

    #[test]
    fn null_the_reader_does_not_expect_is_a_risk() {
        let consumer = object("Receipt", vec![field("ledger_entry_id", Shape::Number)]);
        let mut sent = field("ledger_entry_id", Shape::Integer);
        sent.nullable = true;
        let producer = object("PaymentReceipt", vec![sent]);
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        assert_eq!(
            kinds(&c),
            vec![(
                FindingKind::NullableMismatch,
                Severity::Risk,
                Some("ledger_entry_id".into())
            )]
        );
    }

    #[test]
    fn type_mismatches_are_breaking_and_name_both_types() {
        let consumer = object("Invoice", vec![field("paid", Shape::String)]);
        let producer = object("InvoiceOut", vec![field("paid", Shape::Boolean)]);
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        assert_eq!(c.findings.len(), 1);
        assert_eq!(c.findings[0].kind, FindingKind::TypeMismatch);
        assert!(
            c.findings[0].message.contains("string") && c.findings[0].message.contains("boolean")
        );
    }

    #[test]
    fn arrays_of_objects_compare_their_items() {
        let consumer = Shape::array(object("Invoice", vec![field("amount", Shape::Number)]));
        let producer = Shape::array(object(
            "InvoiceOut",
            vec![field("amount_cents", Shape::Integer)],
        ));
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        assert_eq!(
            kinds(&c),
            vec![(
                FindingKind::MissingField,
                Severity::Breaking,
                Some("[].amount".into())
            )]
        );
    }

    #[test]
    fn requests_flip_the_roles() {
        let consumer = object("PaymentRequest", vec![field("account_id", Shape::Number)]);
        let producer = object(
            "PaymentIn",
            vec![
                field("account_id", Shape::Integer),
                field("currency", Shape::String),
            ],
        );
        let c = compare(
            Direction::Request,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        let found = kinds(&c);
        assert!(found.contains(&(
            FindingKind::MissingField,
            Severity::Breaking,
            Some("currency".into())
        )));
        // The consumer sends `number`, the producer reads `integer`.
        assert!(
            found
                .iter()
                .any(|(k, _, f)| *k == FindingKind::TypeMismatch
                    && f.as_deref() == Some("account_id"))
        );
    }

    #[test]
    fn contract_drift_is_reported_against_the_implementation() {
        let producer = object(
            "AccountOut",
            vec![
                field("id", Shape::Integer),
                field("display_name", Shape::String),
            ],
        );
        let contract = object(
            "Account",
            vec![field("id", Shape::Integer), field("name", Shape::String)],
        );
        let c = compare(
            Direction::Response,
            Views {
                consumer: None,
                producer: Some(&producer),
                contract: Some(&contract),
            },
        );
        let drift: Vec<_> = c
            .findings
            .iter()
            .filter(|f| f.kind == FindingKind::ContractDrift)
            .map(|f| f.field.clone())
            .collect();
        assert_eq!(
            drift,
            vec![Some("display_name".into()), Some("name".into())]
        );
    }

    #[test]
    fn without_a_producer_the_consumer_is_checked_against_the_contract() {
        let consumer = object("Account", vec![field("name", Shape::String)]);
        let contract = object("Account", vec![field("display_name", Shape::String)]);
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: None,
                contract: Some(&contract),
            },
        );
        assert_eq!(
            kinds(&c),
            vec![(
                FindingKind::MissingField,
                Severity::Breaking,
                Some("name".into())
            )]
        );
        assert!(c.findings[0].message.contains("contract"));
    }

    #[test]
    fn unresolved_types_never_produce_findings() {
        let consumer = object(
            "Account",
            vec![field(
                "owner",
                Shape::Named {
                    name: "Owner".into(),
                },
            )],
        );
        let producer = object("AccountOut", vec![field("owner", Shape::Integer)]);
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        assert!(c.findings.is_empty());
        assert_eq!(c.mappings[0].status, MappingStatus::Indeterminate);
    }

    #[test]
    fn mappings_are_sorted_by_field_name() {
        let consumer = object(
            "A",
            vec![field("b", Shape::String), field("a", Shape::String)],
        );
        let producer = object(
            "B",
            vec![
                field("c", Shape::String),
                field("a", Shape::String),
                field("b", Shape::String),
            ],
        );
        let c = compare(
            Direction::Response,
            Views {
                consumer: Some(&consumer),
                producer: Some(&producer),
                contract: None,
            },
        );
        let names: Vec<_> = c.mappings.iter().map(|m| m.json_name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }
}
