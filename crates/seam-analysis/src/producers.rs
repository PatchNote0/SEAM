//! The producer side: FastAPI routes with their full paths, the models they
//! send and read, the OpenAPI operations that describe them, and the tests
//! that call them.

use std::collections::HashSet;

use seam_adapters::FileFacts;
use seam_adapters::python::{self, AppKind, Class, Reference, TypeExpr};
use seam_core::{
    Assumption, Direction, Evidence, Field, HttpMethod, ObjectShape, Role, RouteTemplate, Shape,
};

use crate::workspace::Workspace;

pub struct ProducerRoute {
    pub component: usize,
    pub handler: String,
    pub method: HttpMethod,
    pub template: RouteTemplate,
    pub route: Evidence,
    pub steps: Vec<Evidence>,
    pub assumptions: Vec<Assumption>,
    pub response: Option<Typed>,
    pub request: Option<Typed>,
    /// Test name and the evidence of the request it sends.
    pub tests: Vec<(String, Evidence)>,
    pub contract: Option<ContractOp>,
}

/// A shape with the evidence of where it was declared.
#[derive(Clone)]
pub struct Typed {
    pub shape: Shape,
    pub evidence: Evidence,
}

pub struct ContractOp {
    pub document: String,
    pub operation_id: Option<String>,
    pub evidence: Evidence,
    pub request: Option<Shape>,
    pub response: Option<Typed>,
}

const MAX_DEPTH: usize = 6;

pub fn routes(ws: &Workspace) -> Vec<ProducerRoute> {
    let mut out = Vec::new();
    for (file, record) in ws.files.iter().enumerate() {
        let Some(facts) = ws.python(file) else {
            continue;
        };
        if facts.is_test_file {
            continue;
        }
        for route in &facts.routes {
            let Some(path) = &route.path else { continue };
            let Some((obj_file, obj_name)) = resolve_object(ws, file, &route.object, 0) else {
                continue;
            };
            let component = ws.component_of(&record.path);
            let route_note = ws.say(
                format!(
                    "{} serves {} {} on `{}`.",
                    route.handler, route.method, path, route.object
                ),
                format!(
                    "{} درخواست {} {} را روی `{}` پاسخ می‌دهد.",
                    route.handler, route.method, path, route.object
                ),
            );
            let route_ev = ws.evidence(
                Role::ProducerRoute,
                &route.loc,
                python::ADAPTER,
                "decorator:fastapi-route",
                route_note,
            );
            let response = route
                .response_model
                .as_ref()
                .map(|t| (t, "response_model"))
                .or_else(|| route.returns.as_ref().map(|t| (t, "return annotation")))
                .and_then(|(t, source)| typed(ws, file, t, Direction::Response, source));
            let request = route
                .params
                .iter()
                .filter(|p| p.marker.is_none() || p.marker.as_deref() == Some("Body"))
                .find_map(|p| {
                    let t = p.annotation.as_ref()?;
                    let TypeExpr::Name(name) = t else { return None };
                    resolve_class(ws, file, name, 0).filter(|(f, c)| is_model(ws, *f, c, 0))?;
                    typed(ws, file, t, Direction::Request, "parameter")
                });
            for mount in mounts(ws, (obj_file, obj_name.clone()), &mut HashSet::new(), 0) {
                let full = format!("{}{}", mount.prefix, path);
                out.push(ProducerRoute {
                    component,
                    handler: route.handler.clone(),
                    method: route.method,
                    template: RouteTemplate::parse(&full),
                    route: route_ev.clone(),
                    steps: mount.steps,
                    assumptions: mount.assumptions,
                    response: response.clone(),
                    request: request.clone(),
                    tests: Vec::new(),
                    contract: None,
                });
            }
        }
    }
    attach_contracts(ws, &mut out);
    attach_tests(ws, &mut out);
    out.sort_by(|a, b| {
        (a.component, a.template.key(), a.method, &a.handler).cmp(&(
            b.component,
            b.template.key(),
            b.method,
            &b.handler,
        ))
    });
    out
}

/// Where an app or router object is defined: its file and name.
fn resolve_object(
    ws: &Workspace,
    file: usize,
    name: &str,
    depth: usize,
) -> Option<(usize, String)> {
    let facts = ws.python(file)?;
    if facts.objects.iter().any(|o| o.name == name) {
        return Some((file, name.to_owned()));
    }
    if depth >= MAX_DEPTH {
        return None;
    }
    let import = facts.imports.iter().find(|i| i.local == name)?;
    let imported = import.name.as_deref()?;
    let target = ws.resolve_py_module(file, &import.module)?;
    resolve_object(ws, target, imported, depth + 1)
}

/// A module-level name that refers to a whole module: `accounts` after
/// `from app.routers import accounts`, or `acc` after `import x.y as acc`.
fn resolve_module_alias(ws: &Workspace, file: usize, local: &str) -> Option<usize> {
    let facts = ws.python(file)?;
    let import = facts.imports.iter().find(|i| i.local == local)?;
    match &import.name {
        Some(name) => {
            let joined = if import.module.ends_with('.') {
                format!("{}{name}", import.module)
            } else {
                format!("{}.{name}", import.module)
            };
            ws.resolve_py_module(file, &joined)
        }
        None => ws.resolve_py_module(file, &import.module),
    }
}

struct Mount {
    prefix: String,
    steps: Vec<Evidence>,
    assumptions: Vec<Assumption>,
}

/// Every path prefix under which the object's routes are served.
fn mounts(
    ws: &Workspace,
    object: (usize, String),
    seen: &mut HashSet<(usize, String)>,
    depth: usize,
) -> Vec<Mount> {
    let (file, name) = object.clone();
    let Some(facts) = ws.python(file) else {
        return vec![];
    };
    let Some(obj) = facts.objects.iter().find(|o| o.name == name) else {
        return vec![];
    };
    if obj.kind == AppKind::App {
        return vec![Mount {
            prefix: String::new(),
            steps: vec![],
            assumptions: vec![],
        }];
    }
    let own = obj.prefix.clone().unwrap_or_default();
    let mut decl_steps = Vec::new();
    if !own.is_empty() {
        let note = ws.say(
            format!("The router `{name}` adds the prefix {own}."),
            format!("روتر `{name}` پیشوند {own} را اضافه می‌کند."),
        );
        decl_steps.push(ws.evidence(
            Role::Resolution,
            &obj.loc,
            python::ADAPTER,
            "call:APIRouter",
            note,
        ));
    }
    if depth >= MAX_DEPTH || !seen.insert(object) {
        return vec![];
    }
    let mut out = Vec::new();
    for (parent_file, parent_facts) in ws
        .files
        .iter()
        .enumerate()
        .filter_map(|(i, _)| ws.python(i).map(|f| (i, f)))
    {
        for include in &parent_facts.includes {
            let child = match &include.router {
                Reference::Name { name } => resolve_object(ws, parent_file, name, 0),
                Reference::Attribute { module, name } => {
                    resolve_module_alias(ws, parent_file, module)
                        .and_then(|m| resolve_object(ws, m, name, 0))
                }
            };
            if child != Some((file, name.clone())) {
                continue;
            }
            let Some(parent) = resolve_object(ws, parent_file, &include.app, 0) else {
                continue;
            };
            let include_prefix = include.prefix.clone().unwrap_or_default();
            let router_text = match &include.router {
                Reference::Name { name } => name.clone(),
                Reference::Attribute { module, name } => format!("{module}.{name}"),
            };
            let note = if include_prefix.is_empty() {
                ws.say(
                    format!("`{}` includes `{router_text}`.", include.app),
                    format!("`{}` روتر `{router_text}` را include می‌کند.", include.app),
                )
            } else {
                ws.say(
                    format!(
                        "`{}` includes `{router_text}` with the prefix {include_prefix}.",
                        include.app
                    ),
                    format!(
                        "`{}` روتر `{router_text}` را با پیشوند {include_prefix} include می‌کند.",
                        include.app
                    ),
                )
            };
            let include_ev = ws.evidence(
                Role::Resolution,
                &include.loc,
                python::ADAPTER,
                "call:include_router",
                note,
            );
            for parent_mount in mounts(ws, parent, seen, depth + 1) {
                let mut steps = parent_mount.steps;
                steps.push(include_ev.clone());
                steps.extend(decl_steps.iter().cloned());
                out.push(Mount {
                    prefix: format!("{}{include_prefix}{own}", parent_mount.prefix),
                    steps,
                    assumptions: parent_mount.assumptions,
                });
            }
        }
    }
    if out.is_empty() {
        let path = &ws.files[file].path;
        let statement = ws.say(
            format!("No analysed application includes the router `{name}` in {path}; it was assumed to be served at its own prefix."),
            format!("هیچ برنامهٔ تحلیل‌شده‌ای روتر `{name}` در {path} را include نمی‌کند؛ فرض شد که با پیشوند خودش سرویس داده می‌شود."),
        );
        let note = ws.say(
            format!(
                "The router `{name}` is declared here and included nowhere in the analysed files."
            ),
            format!(
                "روتر `{name}` اینجا تعریف شده و در هیچ‌یک از فایل‌های تحلیل‌شده include نشده است."
            ),
        );
        out.push(Mount {
            prefix: own,
            steps: decl_steps,
            assumptions: vec![Assumption {
                statement,
                missing_capability: None,
                evidence: None,
            }],
        });
        // The evidence for the assumption is the router declaration itself.
        if let Some(last) = out.last_mut() {
            last.steps.push(ws.evidence(
                Role::Assumption,
                &obj.loc,
                python::ADAPTER,
                "call:APIRouter",
                note,
            ));
        }
    }
    out
}

fn resolve_class<'w>(
    ws: &'w Workspace,
    file: usize,
    name: &str,
    depth: usize,
) -> Option<(usize, &'w Class)> {
    let bare = name.rsplit('.').next().unwrap_or(name);
    let facts = ws.python(file)?;
    if let Some(class) = facts.classes.iter().find(|c| c.name == bare) {
        return Some((file, class));
    }
    if depth >= MAX_DEPTH {
        return None;
    }
    if let Some((module, attr)) = name.rsplit_once('.') {
        let target = resolve_module_alias(ws, file, module)?;
        return resolve_class(ws, target, attr, depth + 1);
    }
    let import = facts.imports.iter().find(|i| i.local == bare)?;
    let imported = import.name.as_deref()?;
    let target = ws.resolve_py_module(file, &import.module)?;
    resolve_class(ws, target, imported, depth + 1)
}

const MODEL_BASES: [&str; 3] = ["BaseModel", "SQLModel", "RootModel"];
const ENUM_BASES: [&str; 4] = ["Enum", "StrEnum", "IntEnum", "IntFlag"];

fn last(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn is_model(ws: &Workspace, file: usize, class: &Class, depth: usize) -> bool {
    class.bases.iter().any(|b| {
        let base = last(b.split('[').next().unwrap_or(b));
        MODEL_BASES.contains(&base)
            || (depth < MAX_DEPTH
                && resolve_class(ws, file, b, 0)
                    .is_some_and(|(f, c)| is_model(ws, f, c, depth + 1)))
    })
}

fn typed(
    ws: &Workspace,
    file: usize,
    t: &TypeExpr,
    direction: Direction,
    source: &str,
) -> Option<Typed> {
    let shape = lower(ws, file, t, direction, 0, &mut Vec::new());
    let name = describe(t);
    let (loc, note) = match model_class(ws, file, t) {
        Some((_, class)) => {
            let note = match direction {
                Direction::Response => ws.say(
                    format!("The producer sends {name} ({source})."),
                    format!("تولیدکننده {name} را می‌فرستد ({}).", fa_source(source)),
                ),
                Direction::Request => ws.say(
                    format!("The producer reads the request body as {name}."),
                    format!("تولیدکننده بدنهٔ درخواست را به‌صورت {name} می‌خواند."),
                ),
            };
            (class.loc.clone(), note)
        }
        None => return None,
    };
    Some(Typed {
        shape,
        evidence: ws.evidence(
            Role::ProducerModel,
            &loc,
            python::ADAPTER,
            "class:pydantic-model",
            note,
        ),
    })
}

fn fa_source(source: &str) -> &'static str {
    match source {
        "response_model" => "response_model",
        "return annotation" => "نوع بازگشتی",
        _ => "پارامتر",
    }
}

/// The model class a type names, looking through `list[...]` and `Optional[...]`.
fn model_class<'w>(ws: &'w Workspace, file: usize, t: &TypeExpr) -> Option<(usize, &'w Class)> {
    match t {
        TypeExpr::Name(name) => {
            resolve_class(ws, file, name, 0).filter(|(f, c)| is_model(ws, *f, c, 0))
        }
        TypeExpr::Generic { args, .. } => args.iter().find_map(|a| model_class(ws, file, a)),
        TypeExpr::Union(variants) => variants.iter().find_map(|v| model_class(ws, file, v)),
        _ => None,
    }
}

fn describe(t: &TypeExpr) -> String {
    match t {
        TypeExpr::Name(n) => n.clone(),
        TypeExpr::Generic { base, args } => {
            format!(
                "{base}[{}]",
                args.iter().map(describe).collect::<Vec<_>>().join(", ")
            )
        }
        TypeExpr::Union(v) => v.iter().map(describe).collect::<Vec<_>>().join(" | "),
        TypeExpr::None => "None".into(),
        TypeExpr::Other(text) => text.clone(),
    }
}

fn union(variants: Vec<Shape>) -> Shape {
    let mut out: Vec<Shape> = Vec::new();
    for v in variants {
        let flat = match v {
            Shape::Union { variants } => variants,
            other => vec![other],
        };
        for f in flat {
            if !out.contains(&f) {
                out.push(f);
            }
        }
    }
    if out.len() == 1 {
        out.remove(0)
    } else {
        Shape::Union { variants: out }
    }
}

/// Lowers a Python annotation to the shape of its JSON form.
fn lower(
    ws: &Workspace,
    file: usize,
    t: &TypeExpr,
    direction: Direction,
    depth: usize,
    seen: &mut Vec<(usize, String)>,
) -> Shape {
    if depth > MAX_DEPTH {
        return Shape::unknown("nested too deeply");
    }
    match t {
        TypeExpr::None => Shape::Null,
        TypeExpr::Other(text) => Shape::unknown(format!("`{text}` is not a type SEAM reads")),
        TypeExpr::Union(variants) => union(
            variants
                .iter()
                .map(|v| lower(ws, file, v, direction, depth + 1, seen))
                .collect(),
        ),
        TypeExpr::Generic { base, args } => match last(base) {
            "Optional" => union(vec![
                first_arg(ws, file, args, direction, depth, seen),
                Shape::Null,
            ]),
            "Union" => union(
                args.iter()
                    .map(|a| lower(ws, file, a, direction, depth + 1, seen))
                    .collect(),
            ),
            "list" | "List" | "Sequence" | "set" | "Set" | "frozenset" | "FrozenSet"
            | "Iterable" | "tuple" | "Tuple" => {
                Shape::array(first_arg(ws, file, args, direction, depth, seen))
            }
            "dict" | "Dict" | "Mapping" => Shape::Any,
            "Annotated" => first_arg(ws, file, args, direction, depth, seen),
            "Literal" => {
                if args
                    .iter()
                    .all(|a| matches!(a, TypeExpr::Other(t) if t.parse::<i64>().is_ok()))
                {
                    Shape::Integer
                } else if args
                    .iter()
                    .all(|a| matches!(a, TypeExpr::Other(t) if t == "True" || t == "False"))
                {
                    Shape::Boolean
                } else {
                    Shape::String
                }
            }
            other => Shape::unknown(format!("the generic type {other}[...] is not read")),
        },
        TypeExpr::Name(name) => match last(name) {
            "int" => Shape::Integer,
            "float" => Shape::Number,
            "str" | "bytes" | "datetime" | "date" | "time" | "timedelta" | "UUID" | "Decimal"
            | "EmailStr" | "HttpUrl" | "AnyUrl" | "AnyHttpUrl" | "Path" | "IPv4Address"
            | "IPv6Address" | "SecretStr" => Shape::String,
            "bool" => Shape::Boolean,
            "None" | "NoneType" => Shape::Null,
            "Any" | "object" | "dict" | "Dict" | "Json" => Shape::Any,
            "list" | "List" => Shape::array(Shape::Any),
            _ => match resolve_class(ws, file, name, 0) {
                Some((class_file, class)) if is_model(ws, class_file, class, 0) => {
                    let key = (class_file, class.name.clone());
                    if seen.contains(&key) {
                        return Shape::Named {
                            name: class.name.clone(),
                        };
                    }
                    seen.push(key);
                    let shape = Shape::Object(ObjectShape {
                        name: Some(class.name.clone()),
                        fields: model_fields(ws, class_file, class, direction, depth, seen),
                    });
                    seen.pop();
                    shape
                }
                Some((_, class)) if class.bases.iter().any(|b| ENUM_BASES.contains(&last(b))) => {
                    if class
                        .bases
                        .iter()
                        .any(|b| matches!(last(b), "IntEnum" | "IntFlag" | "int"))
                    {
                        Shape::Integer
                    } else {
                        Shape::String
                    }
                }
                Some(_) => Shape::unknown(format!("{name} is not a model SEAM reads")),
                None => Shape::unknown(format!("{name} is not declared in an analysed file")),
            },
        },
    }
}

fn first_arg(
    ws: &Workspace,
    file: usize,
    args: &[TypeExpr],
    direction: Direction,
    depth: usize,
    seen: &mut Vec<(usize, String)>,
) -> Shape {
    args.first().map_or(Shape::Any, |a| {
        lower(ws, file, a, direction, depth + 1, seen)
    })
}

/// A model's fields, inherited ones first, with the names they have on the
/// wire in `direction`.
fn model_fields(
    ws: &Workspace,
    file: usize,
    class: &Class,
    direction: Direction,
    depth: usize,
    seen: &mut Vec<(usize, String)>,
) -> Vec<Field> {
    let mut fields: Vec<Field> = Vec::new();
    for base in &class.bases {
        if let Some((base_file, base_class)) = resolve_class(ws, file, base, 0)
            && is_model(ws, base_file, base_class, 0)
            && depth < MAX_DEPTH
        {
            for inherited in model_fields(ws, base_file, base_class, direction, depth + 1, seen) {
                fields.retain(|f| f.declared_name != inherited.declared_name);
                fields.push(inherited);
            }
        }
    }
    for f in &class.fields {
        let wire = match direction {
            Direction::Response => f.serialization_alias.as_ref().or(f.alias.as_ref()),
            Direction::Request => f.validation_alias.as_ref().or(f.alias.as_ref()),
        };
        let (shape, nullable) =
            lower(ws, file, &f.annotation, direction, depth + 1, seen).without_null();
        fields.retain(|existing| existing.declared_name != f.name);
        fields.push(Field {
            json_name: wire.cloned().unwrap_or_else(|| f.name.clone()),
            declared_name: f.name.clone(),
            shape,
            // A response model is serialised with every field, defaults included.
            required: direction == Direction::Response || !f.has_default,
            nullable,
            span: Some(f.span.clone()),
        });
    }
    fields
}

fn attach_contracts(ws: &Workspace, routes: &mut [ProducerRoute]) {
    for (file, record) in ws.files.iter().enumerate() {
        let FileFacts::OpenApi(doc) = &record.facts else {
            continue;
        };
        let doc_component = ws.component_of(&record.path);
        let hosts: Vec<usize> = doc
            .servers
            .iter()
            .filter_map(|url| host_of(url))
            .filter_map(|host| ws.service_named(&host).and_then(|s| s.component))
            .collect();
        for op in &doc.operations {
            let template = RouteTemplate::parse(&op.path);
            let matches_route =
                |r: &ProducerRoute| r.method == op.method && r.template.key() == template.key();
            let in_component: Vec<usize> = routes
                .iter()
                .enumerate()
                .filter(|(_, r)| {
                    matches_route(r)
                        && (r.component == doc_component || hosts.contains(&r.component))
                })
                .map(|(i, _)| i)
                .collect();
            let note = match &op.operation_id {
                Some(id) => ws.say(
                    format!("The contract declares {} {} as {id}.", op.method, op.path),
                    format!(
                        "قرارداد، {} {} را با شناسهٔ {id} تعریف می‌کند.",
                        op.method, op.path
                    ),
                ),
                None => ws.say(
                    format!("The contract declares {} {}.", op.method, op.path),
                    format!("قرارداد، {} {} را تعریف می‌کند.", op.method, op.path),
                ),
            };
            let evidence = ws.evidence(
                Role::ContractOperation,
                &op.loc,
                seam_adapters::openapi::ADAPTER,
                "openapi:operation",
                note,
            );
            let response = op.response.as_ref().and_then(|r| {
                let shape = r.shape.clone()?;
                let note = ws.say(
                    format!("The contract describes the {} response.", r.status),
                    format!("قرارداد پاسخ {} را توصیف می‌کند.", r.status),
                );
                Some(Typed {
                    shape,
                    evidence: ws.evidence(
                        Role::ContractSchema,
                        &r.loc,
                        seam_adapters::openapi::ADAPTER,
                        "openapi:response-schema",
                        note,
                    ),
                })
            });
            for i in in_component {
                routes[i].contract = Some(ContractOp {
                    document: ws.files[file].path.clone(),
                    operation_id: op.operation_id.clone(),
                    evidence: evidence.clone(),
                    request: op.request.clone(),
                    response: response.clone(),
                });
            }
        }
    }
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn attach_tests(ws: &Workspace, routes: &mut [ProducerRoute]) {
    for (file, record) in ws.files.iter().enumerate() {
        let Some(facts) = ws.python(file) else {
            continue;
        };
        if facts.test_requests.is_empty() {
            continue;
        }
        let component = ws.component_of(&record.path);
        for request in &facts.test_requests {
            let sent = RouteTemplate::parse(&request.path);
            for route in routes
                .iter_mut()
                .filter(|r| r.component == component && r.method == request.method)
            {
                if sent.reaches(&route.template) != seam_core::RouteMatch::NoMatch {
                    let note = ws.say(
                        format!(
                            "The test {} sends {} {}.",
                            request.test, request.method, request.path
                        ),
                        format!(
                            "آزمون {} درخواست {} {} را می‌فرستد.",
                            request.test, request.method, request.path
                        ),
                    );
                    route.tests.push((
                        request.test.clone(),
                        ws.evidence(
                            Role::Test,
                            &request.loc,
                            python::ADAPTER,
                            "call:test-client",
                            note,
                        ),
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosts_come_from_urls() {
        assert_eq!(host_of("http://api:8000").as_deref(), Some("api"));
        assert_eq!(
            host_of("https://user@Example.com/x").as_deref(),
            Some("example.com")
        );
        assert_eq!(host_of("/relative"), None);
    }
}
