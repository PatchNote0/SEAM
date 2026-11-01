//! Matching consumer calls to producer routes and assembling Seams.

use std::collections::BTreeMap;

use seam_core::seam::classify;
use seam_core::{
    Classification, Comparison, Confidence, ContractRef, Direction, Evidence, Finding, FindingKind,
    HttpMethod, Judgement, Mechanism, Participant, Role, RouteMatch, RouteTemplate, Seam, SeamId,
    Severity, Side, TestRef, Views,
};
use serde::Serialize;

use crate::consumers::ConsumerCall;
use crate::fa;
use crate::producers::ProducerRoute;
use crate::workspace::Workspace;

enum Outcome<'r> {
    /// The path is known only at run time.
    Unknown,
    NoProvider,
    One(&'r ProducerRoute),
    Many(Vec<&'r ProducerRoute>),
}

fn outcome<'r>(routes: &'r [ProducerRoute], call: &ConsumerCall) -> Outcome<'r> {
    let Some(path) = &call.path else {
        return Outcome::Unknown;
    };
    let mut best: Vec<&ProducerRoute> = Vec::new();
    let mut best_rank = (0, 0);
    for route in routes {
        if call.method.is_some_and(|m| m != route.method) {
            continue;
        }
        if call.target.is_some_and(|t| t != route.component) {
            continue;
        }
        let rank = match path.reaches(&route.template) {
            RouteMatch::Exact => (2, 0),
            RouteMatch::Literal { specific } => (1, specific),
            RouteMatch::NoMatch => continue,
        };
        if rank > best_rank {
            best_rank = rank;
            best.clear();
        }
        if rank == best_rank {
            best.push(route);
        }
    }
    match best.len() {
        0 => Outcome::NoProvider,
        1 => Outcome::One(best[0]),
        _ => Outcome::Many(best),
    }
}

/// A producer route and how many Seams reach it, for the route listing.
#[derive(Clone, Debug, Serialize)]
pub struct RouteSummary {
    pub component: String,
    pub method: HttpMethod,
    pub route: String,
    pub handler: String,
    pub path: String,
    pub line: u32,
    pub seams: usize,
}

pub fn seams(
    ws: &Workspace,
    routes: &[ProducerRoute],
    calls: Vec<ConsumerCall>,
) -> (Vec<Seam>, Vec<RouteSummary>) {
    let comp = |i: usize| ws.components[i].name.clone();
    let mut groups: BTreeMap<String, Vec<ConsumerCall>> = BTreeMap::new();
    let mut route_use = vec![0usize; routes.len()];
    for call in calls {
        let method = call.method.map_or("*".to_owned(), |m| m.to_string());
        let key = match outcome(routes, &call) {
            Outcome::One(r) => format!(
                "http {} {} producer={} consumer={}",
                r.method,
                r.template.key(),
                comp(r.component),
                comp(call.component)
            ),
            Outcome::Many(_) => format!(
                "http {method} {} producer=ambiguous consumer={}",
                call.path
                    .as_ref()
                    .map_or(call.path_text.clone(), RouteTemplate::key),
                comp(call.component)
            ),
            Outcome::NoProvider => format!(
                "http {method} {} producer=none consumer={}",
                call.path
                    .as_ref()
                    .map_or(call.path_text.clone(), RouteTemplate::key),
                comp(call.component)
            ),
            Outcome::Unknown => format!(
                "http {method} {} producer=unknown consumer={}",
                call.path_text,
                comp(call.component)
            ),
        };
        groups.entry(key).or_default().push(call);
    }
    let mut seams = Vec::new();
    for (key, calls) in groups {
        let first = &calls[0];
        let result = outcome(routes, first);
        if let Outcome::One(r) = &result
            && let Some(i) = routes.iter().position(|x| std::ptr::eq(x, *r))
        {
            route_use[i] += 1;
        }
        seams.push(assemble(ws, key, &calls, result));
    }
    seams.sort_by(|a, b| a.key.cmp(&b.key));
    let summaries = routes
        .iter()
        .zip(route_use)
        .map(|(r, n)| RouteSummary {
            component: comp(r.component),
            method: r.method,
            route: r.template.to_string(),
            handler: r.handler.clone(),
            path: r.route.span.path.clone(),
            line: r.route.span.start.line,
            seams: n,
        })
        .collect();
    (seams, summaries)
}

struct Evidences(Vec<Evidence>);

impl Evidences {
    fn push(&mut self, e: &Evidence) -> usize {
        if let Some(i) = self.0.iter().position(|x| x == e) {
            return i;
        }
        self.0.push(e.clone());
        self.0.len() - 1
    }
}

fn assemble(ws: &Workspace, key: String, calls: &[ConsumerCall], result: Outcome<'_>) -> Seam {
    let first = &calls[0];
    let mut ev = Evidences(Vec::new());
    let consumer_ev: Vec<usize> = calls.iter().map(|c| ev.push(&c.call)).collect();
    for call in calls {
        for step in &call.steps {
            ev.push(step);
        }
    }
    let consumer_response = calls.iter().find_map(|c| c.response.as_ref());
    let consumer_request = calls.iter().find_map(|c| c.request.as_ref());
    if let Some(t) = consumer_response {
        ev.push(&t.evidence);
    }
    if let Some(t) = consumer_request {
        ev.push(&t.evidence);
    }

    let mut reasons: Vec<String> = Vec::new();
    for call in calls {
        for reason in &call.reasons {
            if !reasons.contains(reason) {
                reasons.push(reason.clone());
            }
        }
    }
    let method = first.method.unwrap_or(HttpMethod::Get);
    let path_text = first
        .path
        .as_ref()
        .map_or(first.path_text.clone(), RouteTemplate::to_string);
    let mut producer = None;
    let mut contract = None;
    let mut producer_route: Option<&ProducerRoute> = None;
    let mut extra_findings = Vec::new();
    let mut tests = Vec::new();
    let mut assumptions = Vec::new();
    for call in calls {
        for (assumption, evidence) in &call.assumptions {
            let mut a = assumption.clone();
            a.evidence = Some(ev.push(evidence));
            if !assumptions.contains(&a) {
                assumptions.push(a);
            }
        }
    }
    let candidates = match &result {
        Outcome::Unknown => {
            let start = first
                .dynamic_start
                .clone()
                .unwrap_or_else(|| "a runtime value".to_owned());
            reasons.push(ws.say(
                format!("The path starts with {start}, which is known only at run time, so the target route cannot be determined from source."),
                format!("مسیر با {start} شروع می‌شود که فقط هنگام اجرا معلوم است، پس مسیر مقصد را نمی‌توان از روی کد تعیین کرد."),
            ));
            if first.method.is_none() {
                reasons.push(ws.say(
                    "The HTTP method is computed at run time.".to_owned(),
                    "متد HTTP هنگام اجرا محاسبه می‌شود.".to_owned(),
                ));
            }
            0
        }
        Outcome::NoProvider => {
            let message = ws.say(
                format!("No analysed service declares {method} {path_text}."),
                format!("هیچ سرویس تحلیل‌شده‌ای {method} {path_text} را تعریف نکرده است."),
            );
            reasons.push(message.clone());
            extra_findings.push(Finding {
                kind: FindingKind::NoProvider,
                severity: Severity::Risk,
                field: None,
                message,
            });
            0
        }
        Outcome::One(route) => {
            producer_route = Some(route);
            let route_index = ev.push(&route.route);
            producer = Some(Participant {
                component: ws.components[route.component].name.clone(),
                symbol: Some(route.handler.clone()),
                evidence: vec![route_index],
            });
            for step in &route.steps {
                ev.push(step);
            }
            if let Some(t) = &route.response {
                ev.push(&t.evidence);
            }
            if let Some(t) = &route.request {
                ev.push(&t.evidence);
            }
            if let Some(op) = &route.contract {
                let index = ev.push(&op.evidence);
                contract = Some(ContractRef {
                    document: op.document.clone(),
                    operation_id: op.operation_id.clone(),
                    evidence: index,
                });
                if let Some(t) = &op.response {
                    ev.push(&t.evidence);
                }
            }
            for a in &route.assumptions {
                let mut a = a.clone();
                a.evidence = route
                    .steps
                    .iter()
                    .find(|s| s.role == Role::Assumption)
                    .map(|s| ev.push(s));
                if !assumptions.contains(&a) {
                    assumptions.push(a);
                }
            }
            let tname = &ws.components[route.component].name;
            reasons.push(ws.say(
                format!(
                    "Exactly one analysed route serves {} {}: {} in {tname}.",
                    route.method, route.template, route.handler
                ),
                format!(
                    "دقیقاً یک مسیر تحلیل‌شده {} {} را پاسخ می‌دهد: {} در {tname}.",
                    route.method, route.template, route.handler
                ),
            ));
            for (name, test) in &route.tests {
                let index = ev.push(test);
                tests.push(TestRef {
                    path: test.span.path.clone(),
                    name: name.clone(),
                    side: Side::Producer,
                    evidence: index,
                });
            }
            1
        }
        Outcome::Many(routes) => {
            let list = routes
                .iter()
                .map(|r| format!("{} in {}", r.handler, ws.components[r.component].name))
                .collect::<Vec<_>>()
                .join(", ");
            let list_fa = routes
                .iter()
                .map(|r| format!("{} در {}", r.handler, ws.components[r.component].name))
                .collect::<Vec<_>>()
                .join("، ");
            for r in routes {
                ev.push(&r.route);
            }
            reasons.push(ws.say(
                format!(
                    "{} analysed routes could serve {method} {path_text}: {list}.",
                    routes.len()
                ),
                format!(
                    "{} مسیر تحلیل‌شده می‌توانند {method} {path_text} را پاسخ دهند: {list_fa}.",
                    routes.len()
                ),
            ));
            routes.len()
        }
    };
    for call in calls {
        for (name, test) in &call.tests {
            let index = ev.push(test);
            let t = TestRef {
                path: test.span.path.clone(),
                name: name.clone(),
                side: Side::Consumer,
                evidence: index,
            };
            if !tests.contains(&t) {
                tests.push(t);
            }
        }
    }

    let contract_op = producer_route.and_then(|r| r.contract.as_ref());
    let response = compare_if_any(
        Direction::Response,
        consumer_response.map(|t| &t.shape),
        producer_route
            .and_then(|r| r.response.as_ref())
            .map(|t| &t.shape),
        contract_op
            .and_then(|op| op.response.as_ref())
            .map(|t| &t.shape),
    );
    let request = compare_if_any(
        Direction::Request,
        consumer_request.map(|t| &t.shape),
        producer_route
            .and_then(|r| r.request.as_ref())
            .map(|t| &t.shape),
        contract_op.and_then(|op| op.request.as_ref()),
    );
    let mut findings: Vec<Finding> = response
        .findings
        .iter()
        .chain(&request.findings)
        .cloned()
        .collect();
    for f in &findings {
        if let Some(persian) = fa::finding(&f.message) {
            ws.say(f.message.clone(), persian);
        }
    }
    findings.extend(extra_findings);
    let unknown = matches!(result, Outcome::Unknown);
    if !unknown && tests.is_empty() {
        findings.push(Finding {
            kind: FindingKind::Untested,
            severity: Severity::Info,
            field: None,
            message: ws.say(
                "No test exercises either side of this Seam.".to_owned(),
                "هیچ آزمونی هیچ‌یک از دو سوی این درز را نمی‌آزماید.".to_owned(),
            ),
        });
    }
    findings.sort_by(|a, b| (a.severity, a.kind, &a.field).cmp(&(b.severity, b.kind, &b.field)));

    let (classification, level) = classify(Judgement {
        consumer_resolved: !unknown,
        producer_candidates: candidates,
        assumptions: assumptions.len(),
    });
    if classification == Classification::Inferred && !assumptions.is_empty() {
        reasons.push(ws.say(
            format!(
                "The match rests on {} assumption{}, listed with this Seam.",
                assumptions.len(),
                if assumptions.len() == 1 { "" } else { "s" }
            ),
            format!(
                "این تطبیق بر {} فرض استوار است که همراه این درز آمده‌اند.",
                assumptions.len()
            ),
        ));
    }
    let mechanism = Mechanism::Http {
        method: producer_route.map_or(method, |r| r.method),
        route: producer_route.map_or_else(
            || {
                first
                    .path
                    .clone()
                    .unwrap_or_else(|| RouteTemplate::parse(&first.path_text))
            },
            |r| r.template.clone(),
        ),
    };
    let consumer = Participant {
        component: ws.components[first.component].name.clone(),
        symbol: calls
            .iter()
            .map(|c| c.function.clone())
            .find(|f| !f.is_empty()),
        evidence: consumer_ev,
    };
    let fingerprint = fingerprint(
        &mechanism,
        &response,
        &request,
        contract.as_ref(),
        producer.as_ref(),
    );
    tests.sort_by(|a, b| (a.side as u8, &a.path, &a.name).cmp(&(b.side as u8, &b.path, &b.name)));
    Seam {
        id: SeamId::from_key(&key),
        key,
        mechanism,
        consumer,
        producer,
        contract,
        request,
        response,
        findings,
        classification,
        confidence: Confidence { level, reasons },
        assumptions,
        evidence: ev.0,
        tests,
        fingerprint,
    }
}

fn compare_if_any(
    direction: Direction,
    consumer: Option<&seam_core::Shape>,
    producer: Option<&seam_core::Shape>,
    contract: Option<&seam_core::Shape>,
) -> Comparison {
    if consumer.is_none() && producer.is_none() && contract.is_none() {
        return Comparison::default();
    }
    seam_core::compat::compare(
        direction,
        Views {
            consumer,
            producer,
            contract,
        },
    )
}

fn fingerprint(
    mechanism: &Mechanism,
    response: &Comparison,
    request: &Comparison,
    contract: Option<&ContractRef>,
    producer: Option<&Participant>,
) -> String {
    #[derive(Serialize)]
    struct Print<'a> {
        mechanism: &'a Mechanism,
        response: &'a [seam_core::FieldMapping],
        request: &'a [seam_core::FieldMapping],
        contract: Option<&'a Option<String>>,
        producer: Option<(&'a str, &'a Option<String>)>,
    }
    let print = Print {
        mechanism,
        response: &response.mappings,
        request: &request.mappings,
        contract: contract.map(|c| &c.operation_id),
        producer: producer.map(|p| (p.component.as_str(), &p.symbol)),
    };
    // Spans move with every edit; the fingerprint is about the contract, so
    // field positions are left out.
    let mut value = serde_json::to_value(&print).unwrap_or_default();
    strip_spans(&mut value);
    let digest = blake3::hash(value.to_string().as_bytes());
    digest.to_hex()[..16].to_owned()
}

fn strip_spans(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            map.remove("span");
            for v in map.values_mut() {
                strip_spans(v);
            }
        }
        serde_json::Value::Array(items) => items.iter_mut().for_each(strip_spans),
        _ => {}
    }
}
