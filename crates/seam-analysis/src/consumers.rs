//! The consumer side: HTTP requests made from TypeScript and JavaScript,
//! followed through wrapper functions, constants and environment variables
//! to a method, a path and, when the repository names it, a target component.

use std::collections::HashMap;

use seam_adapters::syntax::Located;
use seam_adapters::typescript::{self, Expr, Function, TsParam, TypeDecl};
use seam_core::{
    Assumption, Evidence, HttpMethod, ObjectShape, Role, RouteTemplate, Segment, Shape,
};

use crate::producers::{Typed, host_of};
use crate::workspace::Workspace;

const MAX_DEPTH: usize = 6;
const ADAPTER: &str = typescript::ADAPTER;

pub struct ConsumerCall {
    pub component: usize,
    pub file: usize,
    /// The function the request is made from; empty at module level.
    pub function: String,
    pub method: Option<HttpMethod>,
    /// `None` when the path is known only at run time.
    pub path: Option<RouteTemplate>,
    /// The path as SEAM sees it, with `{}` for runtime values.
    pub path_text: String,
    /// The runtime expression the path starts with, when it does.
    pub dynamic_start: Option<String>,
    /// The component the base URL names, when the repository says which.
    pub target: Option<usize>,
    pub call: Evidence,
    pub steps: Vec<Evidence>,
    pub assumptions: Vec<(Assumption, Evidence)>,
    pub reasons: Vec<String>,
    pub response: Option<Typed>,
    pub request: Option<Typed>,
    /// Test name and the evidence of the test that calls this function.
    pub tests: Vec<(String, Evidence)>,
}

/// One piece of a URL as evaluated in a function's scope.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    /// A parameter of the function the URL was evaluated in.
    Param(usize),
    Env {
        name: String,
        fallback: Option<String>,
    },
    /// A value known only at run time, as source text.
    Runtime(String),
}

#[derive(Clone)]
enum Resp {
    /// The response type is the caller's type argument at this position.
    Generic(usize),
    Fixed(Box<Typed>),
    Unknown,
}

#[derive(Clone)]
enum Body {
    Param(usize),
    None,
}

#[derive(Clone)]
struct Wrapper {
    template: Vec<Piece>,
    method: Option<HttpMethod>,
    body: Body,
    response: Resp,
    client: String,
    steps: Vec<Evidence>,
}

type FnKey = (usize, String);

pub fn calls(ws: &Workspace) -> Vec<ConsumerCall> {
    let wrappers = wrappers(ws);
    let mut out = Vec::new();
    for file in 0..ws.files.len() {
        let Some(facts) = ws.ts(file) else { continue };
        if facts.is_test_file {
            continue;
        }
        let component = ws.component_of(&ws.files[file].path);
        for function in &facts.functions {
            let is_wrapper =
                wrappers.contains_key(&(file, function.name.clone())) && !function.name.is_empty();
            if !is_wrapper {
                for request in &function.requests {
                    let mut steps = Vec::new();
                    let pieces = eval(ws, file, &function.params, &request.url, &mut steps, 0);
                    let display = display(&pieces, &function.params);
                    let who = who(function);
                    let note = ws.say(
                        format!(
                            "{who} sends {} {display} with {}.",
                            method_text(request.method),
                            request.client
                        ),
                        format!(
                            "{who} با {} درخواست {} {display} را می‌فرستد.",
                            request.client,
                            method_text(request.method)
                        ),
                    );
                    let call = ws.evidence(
                        Role::ConsumerCall,
                        &request.loc,
                        ADAPTER,
                        &format!("call-expression:{}", request.client),
                        note,
                    );
                    let response = request
                        .response
                        .as_ref()
                        .or(if function.requests.len() == 1 {
                            function.returns.as_ref()
                        } else {
                            None
                        })
                        .and_then(|s| consumer_typed(ws, file, s, &function.loc, true));
                    let request_type = match &request.body {
                        Some(Expr::Ref { name }) => param_typed(ws, file, function, name),
                        _ => None,
                    };
                    out.push(build(
                        ws,
                        component,
                        file,
                        function,
                        request.method,
                        pieces,
                        call,
                        steps,
                        response,
                        request_type,
                    ));
                }
            }
            for site in &function.calls {
                let Some((wfile, wfunc)) = lookup_function(ws, file, &site.callee, 0) else {
                    continue;
                };
                let Some(wrapper) = wrappers.get(&(wfile, wfunc.name.clone())) else {
                    continue;
                };
                let mut steps = wrapper.steps.clone();
                let args: Vec<Vec<Piece>> = site
                    .args
                    .iter()
                    .map(|a| eval(ws, file, &function.params, a, &mut steps, 0))
                    .collect();
                let pieces = substitute(&wrapper.template, &args);
                if leading_param(&pieces).is_some() {
                    continue; // this call makes the function itself a wrapper
                }
                let who = who(function);
                let note = ws.say(
                    format!(
                        "{who} calls {} to send {} {}.",
                        wfunc.name,
                        method_text(wrapper.method),
                        display(&pieces, &function.params)
                    ),
                    format!(
                        "{who} تابع {} را صدا می‌زند تا درخواست {} {} فرستاده شود.",
                        wfunc.name,
                        method_text(wrapper.method),
                        display(&pieces, &function.params)
                    ),
                );
                let call = ws.evidence(
                    Role::ConsumerCall,
                    &site.loc,
                    ADAPTER,
                    "call-expression:wrapper",
                    note,
                );
                let response = match &wrapper.response {
                    Resp::Generic(k) => site
                        .type_args
                        .get(*k)
                        .and_then(|s| consumer_typed(ws, file, s, &site.loc, true)),
                    Resp::Fixed(t) => Some(t.as_ref().clone()),
                    Resp::Unknown => None,
                };
                let request_type = match wrapper.body {
                    Body::Param(j) => match site.args.get(j) {
                        Some(Expr::Ref { name }) => param_typed(ws, file, function, name),
                        _ => None,
                    },
                    Body::None => None,
                };
                out.push(build(
                    ws,
                    component,
                    file,
                    function,
                    wrapper.method,
                    pieces,
                    call,
                    steps,
                    response,
                    request_type,
                ));
            }
        }
    }
    attach_tests(ws, &mut out);
    out
}

fn who(function: &Function) -> String {
    if function.name.is_empty() {
        "Module-level code".to_owned()
    } else {
        function.name.clone()
    }
}

fn method_text(method: Option<HttpMethod>) -> String {
    method.map_or_else(|| "a request".to_owned(), |m| m.to_string())
}

/// Finds request wrappers: functions that pass one of their own parameters
/// in as the start of a request's path, directly or through another wrapper.
fn wrappers(ws: &Workspace) -> HashMap<FnKey, Wrapper> {
    let mut found: HashMap<FnKey, Wrapper> = HashMap::new();
    for file in 0..ws.files.len() {
        let Some(facts) = ws.ts(file) else { continue };
        if facts.is_test_file {
            continue;
        }
        for function in facts.functions.iter().filter(|f| !f.name.is_empty()) {
            for request in &function.requests {
                let mut steps = Vec::new();
                let pieces = eval(ws, file, &function.params, &request.url, &mut steps, 0);
                let Some(param) = leading_param(&pieces) else {
                    continue;
                };
                let param_name = function
                    .params
                    .get(param)
                    .map_or("path", |p| p.name.as_str());
                let note = ws.say(
                    format!(
                        "{} passes its `{param_name}` argument into the URL of its {} request.",
                        function.name, request.client
                    ),
                    format!(
                        "{} آرگومان `{param_name}` خود را در نشانی درخواست {} قرار می‌دهد.",
                        function.name, request.client
                    ),
                );
                steps.insert(
                    0,
                    ws.evidence(
                        Role::Resolution,
                        &request.loc,
                        ADAPTER,
                        &format!("call-expression:{}", request.client),
                        note,
                    ),
                );
                let body = match &request.body {
                    Some(Expr::Ref { name }) => function
                        .params
                        .iter()
                        .position(|p| &p.name == name)
                        .map_or(Body::None, Body::Param),
                    _ => Body::None,
                };
                let declared = request.response.as_ref().or(function.returns.as_ref());
                let response = match declared {
                    Some(Shape::Named { name }) if function.type_params.contains(name) => {
                        Resp::Generic(
                            function
                                .type_params
                                .iter()
                                .position(|t| t == name)
                                .unwrap_or(0),
                        )
                    }
                    Some(shape) => consumer_typed(ws, file, shape, &function.loc, true)
                        .map_or(Resp::Unknown, |t| Resp::Fixed(Box::new(t))),
                    None => Resp::Unknown,
                };
                found.insert(
                    (file, function.name.clone()),
                    Wrapper {
                        template: pieces,
                        method: request.method,
                        body,
                        response,
                        client: request.client.clone(),
                        steps,
                    },
                );
                break;
            }
        }
    }
    // Wrappers of wrappers, until nothing new turns up.
    for _ in 0..MAX_DEPTH {
        let mut added = Vec::new();
        for file in 0..ws.files.len() {
            let Some(facts) = ws.ts(file) else { continue };
            if facts.is_test_file {
                continue;
            }
            for function in facts.functions.iter().filter(|f| !f.name.is_empty()) {
                if found.contains_key(&(file, function.name.clone())) {
                    continue;
                }
                for site in &function.calls {
                    let Some((wfile, wfunc)) = lookup_function(ws, file, &site.callee, 0) else {
                        continue;
                    };
                    let Some(inner) = found.get(&(wfile, wfunc.name.clone())) else {
                        continue;
                    };
                    let mut steps = inner.steps.clone();
                    let args: Vec<Vec<Piece>> = site
                        .args
                        .iter()
                        .map(|a| eval(ws, file, &function.params, a, &mut steps, 0))
                        .collect();
                    let pieces = substitute(&inner.template, &args);
                    let Some(param) = leading_param(&pieces) else {
                        continue;
                    };
                    let param_name = function
                        .params
                        .get(param)
                        .map_or("path", |p| p.name.as_str());
                    let note = ws.say(
                        format!(
                            "{} passes its `{param_name}` argument on to {}.",
                            function.name, wfunc.name
                        ),
                        format!(
                            "{} آرگومان `{param_name}` خود را به {} می‌دهد.",
                            function.name, wfunc.name
                        ),
                    );
                    steps.push(ws.evidence(
                        Role::Resolution,
                        &site.loc,
                        ADAPTER,
                        "call-expression:wrapper",
                        note,
                    ));
                    let body = match inner.body {
                        Body::Param(j) => match site.args.get(j) {
                            Some(Expr::Ref { name }) => function
                                .params
                                .iter()
                                .position(|p| &p.name == name)
                                .map_or(Body::None, Body::Param),
                            _ => Body::None,
                        },
                        Body::None => Body::None,
                    };
                    let response = match &inner.response {
                        Resp::Generic(k) => match site.type_args.get(*k) {
                            Some(Shape::Named { name }) if function.type_params.contains(name) => {
                                Resp::Generic(
                                    function
                                        .type_params
                                        .iter()
                                        .position(|t| t == name)
                                        .unwrap_or(0),
                                )
                            }
                            Some(shape) => consumer_typed(ws, file, shape, &site.loc, true)
                                .map_or(Resp::Unknown, |t| Resp::Fixed(Box::new(t))),
                            None => Resp::Unknown,
                        },
                        other => other.clone(),
                    };
                    added.push((
                        (file, function.name.clone()),
                        Wrapper {
                            template: pieces,
                            method: inner.method,
                            body,
                            response,
                            client: inner.client.clone(),
                            steps,
                        },
                    ));
                    break;
                }
            }
        }
        if added.is_empty() {
            break;
        }
        for (key, wrapper) in added {
            found.entry(key).or_insert(wrapper);
        }
    }
    found
}

/// The index of the parameter that supplies the start of the path, if one does.
fn leading_param(pieces: &[Piece]) -> Option<usize> {
    let rest = match pieces.first()? {
        Piece::Env { .. } => &pieces[1..],
        Piece::Text(t) if t.contains("://") => {
            let after = &t[t.find("://")? + 3..];
            if after.contains('/') {
                return None;
            }
            &pieces[1..]
        }
        _ => pieces,
    };
    match rest.first()? {
        Piece::Param(i) => Some(*i),
        _ => None,
    }
}

fn substitute(template: &[Piece], args: &[Vec<Piece>]) -> Vec<Piece> {
    let mut out = Vec::new();
    for piece in template {
        match piece {
            Piece::Param(k) => match args.get(*k) {
                Some(arg) => out.extend(arg.iter().cloned()),
                None => out.push(Piece::Runtime(format!("argument {}", k + 1))),
            },
            other => out.push(other.clone()),
        }
    }
    merge_text(out)
}

fn merge_text(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    for piece in pieces {
        match (out.last_mut(), piece) {
            (Some(Piece::Text(last)), Piece::Text(t)) => last.push_str(&t),
            (_, p) => out.push(p),
        }
    }
    out
}

fn eval(
    ws: &Workspace,
    file: usize,
    params: &[TsParam],
    expr: &Expr,
    steps: &mut Vec<Evidence>,
    depth: usize,
) -> Vec<Piece> {
    if depth > MAX_DEPTH {
        return vec![Piece::Runtime("…".into())];
    }
    let pieces = match expr {
        Expr::Text { value } => vec![Piece::Text(value.clone())],
        Expr::Concat { parts } => parts
            .iter()
            .flat_map(|p| eval(ws, file, params, p, steps, depth + 1))
            .collect(),
        Expr::Env { name, fallback } => {
            let fallback = fallback.as_ref().and_then(|f| {
                let pieces = eval(ws, file, params, f, steps, depth + 1);
                pieces
                    .iter()
                    .map(|p| match p {
                        Piece::Text(t) => Some(t.clone()),
                        _ => None,
                    })
                    .collect::<Option<String>>()
            });
            vec![Piece::Env {
                name: name.clone(),
                fallback,
            }]
        }
        Expr::Ref { name } => {
            if let Some(i) = params.iter().position(|p| &p.name == name) {
                vec![Piece::Param(i)]
            } else if let Some((cfile, constant)) = lookup_constant(ws, file, name, 0) {
                let value = eval(ws, cfile, &[], &constant.value, steps, depth + 1);
                let shown = display(&value, &[]);
                let note = ws.say(
                    format!("`{name}` holds {shown}."),
                    format!("`{name}` مقدار {shown} را نگه می‌دارد."),
                );
                let ev = ws.evidence(
                    Role::Resolution,
                    &constant.loc,
                    ADAPTER,
                    "lexical-declaration:const",
                    note,
                );
                if !steps.contains(&ev) {
                    steps.push(ev);
                }
                value
            } else {
                vec![Piece::Runtime(name.clone())]
            }
        }
        Expr::Unknown { text } => vec![Piece::Runtime(text.clone())],
    };
    merge_text(pieces)
}

/// How a URL reads to a person: literal text, `{param}` for parameters,
/// `${NAME}` for environment variables and `{…}` for other runtime values.
fn display(pieces: &[Piece], params: &[TsParam]) -> String {
    pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) => t.clone(),
            Piece::Param(i) => format!(
                "{{{}}}",
                params.get(*i).map_or("param", |p| p.name.as_str())
            ),
            Piece::Env { name, .. } => format!("${{{name}}}"),
            Piece::Runtime(t) => format!("{{{}}}", short(t)),
        })
        .collect()
}

fn short(text: &str) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 32 {
        format!("{}…", flat.chars().take(31).collect::<String>())
    } else {
        flat
    }
}

#[allow(clippy::too_many_arguments)]
fn build(
    ws: &Workspace,
    component: usize,
    file: usize,
    function: &Function,
    method: Option<HttpMethod>,
    pieces: Vec<Piece>,
    call: Evidence,
    mut steps: Vec<Evidence>,
    response: Option<Typed>,
    request: Option<Typed>,
) -> ConsumerCall {
    let mut assumptions = Vec::new();
    let mut reasons = Vec::new();
    let comp_name = ws.components[component].name.clone();
    let everywhere_en = "so the route was matched among all analysed services";
    let everywhere_fa = "پس مسیر در میان همهٔ سرویس‌های تحلیل‌شده جست‌وجو شد";
    let assumption_loc = steps
        .iter()
        .rev()
        .find(|e| e.method.technique == "lexical-declaration:const")
        .map(|e| Located {
            span: e.span.clone(),
            excerpt: e.excerpt.clone(),
        })
        .unwrap_or_else(|| Located {
            span: call.span.clone(),
            excerpt: call.excerpt.clone(),
        });
    let assume = |ws: &Workspace, en: String, fa: String, note_en: String, note_fa: String| {
        let statement = ws.say(en, fa);
        let note = ws.say(note_en, note_fa);
        let ev = ws.evidence(Role::Assumption, &assumption_loc, ADAPTER, "url:base", note);
        (
            Assumption {
                statement,
                missing_capability: None,
                evidence: None,
            },
            ev,
        )
    };
    let mut target = None;
    let mut path_pieces: Vec<Piece> = pieces.clone();
    let mut dynamic_start = None;

    let origin_to_target = |ws: &Workspace,
                            origin: &str,
                            steps: &mut Vec<Evidence>,
                            assumptions: &mut Vec<(Assumption, Evidence)>,
                            reasons: &mut Vec<String>|
     -> Option<usize> {
        let service =
            host_of(origin).and_then(|h| ws.service_named(&h).filter(|s| s.component.is_some()));
        match service {
            Some(s) => {
                let target = s.component?;
                let ctx = s.context.clone().unwrap_or_default();
                let tname = &ws.components[target].name;
                let note = ws.say(
                    format!(
                        "Docker Compose builds the service {} from {}, the component {tname}.",
                        s.service.name,
                        display_dir(&ctx)
                    ),
                    format!(
                        "Docker Compose سرویس {} را از {} می‌سازد، یعنی مؤلفهٔ {tname}.",
                        s.service.name,
                        display_dir(&ctx)
                    ),
                );
                steps.push(ws.evidence(
                    Role::Resolution,
                    &s.service.loc,
                    seam_adapters::compose::ADAPTER,
                    "compose:service",
                    note,
                ));
                reasons.push(ws.say(
                    format!("The base URL {origin} names the Compose service {}, built from {}.", s.service.name, display_dir(&ctx)),
                    format!("نشانی پایهٔ {origin} به سرویس {} در Compose اشاره می‌کند که از {} ساخته می‌شود.", s.service.name, display_dir(&ctx)),
                ));
                Some(target)
            }
            None => {
                assumptions.push(assume(
                    ws,
                    format!(
                        "The base URL {origin} does not name an analysed service, {everywhere_en}."
                    ),
                    format!(
                        "نشانی پایهٔ {origin} به هیچ سرویس تحلیل‌شده‌ای اشاره نمی‌کند، {everywhere_fa}."
                    ),
                    format!("The base URL {origin} is used here."),
                    format!("نشانی پایهٔ {origin} اینجا به کار رفته است."),
                ));
                None
            }
        }
    };

    match pieces.first() {
        Some(Piece::Env { name, fallback }) => {
            path_pieces = pieces[1..].to_vec();
            match ws.env_for(component, name) {
                Some((service, entry)) => {
                    let value = entry.value.clone().unwrap_or_default();
                    let note = ws.say(
                        format!("Docker Compose sets {name}={value} for the service {}.", service.service.name),
                        format!("Docker Compose برای سرویس {} مقدار {name}={value} را تعیین می‌کند.", service.service.name),
                    );
                    steps.push(ws.evidence(Role::Resolution, &entry.loc, seam_adapters::compose::ADAPTER, "compose:environment", note));
                    target = origin_to_target(ws, &value, &mut steps, &mut assumptions, &mut reasons);
                }
                None => match fallback {
                    Some(fallback) if host_of(fallback).and_then(|h| ws.service_named(&h)).is_some() => {
                        assumptions.push(assume(
                            ws,
                            format!("{name} is not set for {comp_name} anywhere in the repository; its fallback {fallback} was used."),
                            format!("مقدار {name} برای {comp_name} در هیچ جای مخزن تعیین نشده است؛ مقدار پیش‌فرض آن، {fallback}، به کار رفت."),
                            format!("{name} is read here, with the fallback {fallback}."),
                            format!("{name} اینجا خوانده می‌شود، با مقدار پیش‌فرض {fallback}."),
                        ));
                        target = origin_to_target(ws, fallback, &mut steps, &mut assumptions, &mut reasons);
                    }
                    Some(fallback) => assumptions.push(assume(
                        ws,
                        format!("{name} is not set for {comp_name} anywhere in the repository, and its fallback {fallback} does not name an analysed service, {everywhere_en}."),
                        format!("مقدار {name} برای {comp_name} در هیچ جای مخزن تعیین نشده است و مقدار پیش‌فرض آن، {fallback}، به هیچ سرویس تحلیل‌شده‌ای اشاره نمی‌کند، {everywhere_fa}."),
                        format!("{name} is read here, with the fallback {fallback}."),
                        format!("{name} اینجا خوانده می‌شود، با مقدار پیش‌فرض {fallback}."),
                    )),
                    None => assumptions.push(assume(
                        ws,
                        format!("{name} is not set for {comp_name} anywhere in the repository, {everywhere_en}."),
                        format!("مقدار {name} برای {comp_name} در هیچ جای مخزن تعیین نشده است، {everywhere_fa}."),
                        format!("{name} is read here."),
                        format!("{name} اینجا خوانده می‌شود."),
                    )),
                },
            }
        }
        Some(Piece::Text(t)) if t.contains("://") => {
            let scheme_end = t.find("://").map_or(0, |i| i + 3);
            let split = t[scheme_end..]
                .find('/')
                .map_or(t.len(), |i| scheme_end + i);
            let origin = t[..split].to_owned();
            let mut rest = vec![Piece::Text(t[split..].to_owned())];
            rest.extend(pieces[1..].iter().cloned());
            path_pieces = merge_text(rest);
            target = origin_to_target(ws, &origin, &mut steps, &mut assumptions, &mut reasons);
        }
        Some(Piece::Text(t)) if t.starts_with('/') => {
            assumptions.push(assume(
                ws,
                format!("The request is relative to the page's own origin, which the repository does not name, {everywhere_en}."),
                format!("درخواست نسبت به مبدأ خودِ صفحه فرستاده می‌شود که در مخزن مشخص نشده است، {everywhere_fa}."),
                "The URL here has no origin.".to_owned(),
                "نشانی اینجا مبدأ ندارد.".to_owned(),
            ));
        }
        Some(first @ (Piece::Runtime(_) | Piece::Param(_))) => {
            let text = display(std::slice::from_ref(first), &function.params);
            if matches!(pieces.get(1), Some(Piece::Text(t)) if t.starts_with('/')) {
                path_pieces = pieces[1..].to_vec();
                assumptions.push(assume(
                    ws,
                    format!("The base URL comes from {text}, whose value is known only at run time, {everywhere_en}."),
                    format!("نشانی پایه از {text} می‌آید که مقدارش فقط هنگام اجرا معلوم است، {everywhere_fa}."),
                    format!("The base URL {text} is computed at run time."),
                    format!("نشانی پایهٔ {text} هنگام اجرا محاسبه می‌شود."),
                ));
            } else {
                dynamic_start = Some(text);
            }
        }
        _ => {}
    }

    let path_text_full = path_pieces
        .iter()
        .map(|p| match p {
            Piece::Text(t) => t.clone(),
            _ => "{}".to_owned(),
        })
        .collect::<String>();
    let path_only = path_text_full
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_owned();
    let path_text = if path_only.is_empty() {
        "/".to_owned()
    } else {
        path_only.clone()
    };
    let mut template = RouteTemplate::parse(&path_only);
    for segment in &mut template.segments {
        if matches!(segment, Segment::Literal(t) if t.contains("{}")) {
            *segment = Segment::Param(None);
        }
    }
    if dynamic_start.is_none() && template.starts_dynamic() {
        dynamic_start = path_pieces
            .iter()
            .find(|p| !matches!(p, Piece::Text(t) if t.chars().all(|c| c == '/')))
            .map(|p| display(std::slice::from_ref(p), &function.params));
    }
    let path = if dynamic_start.is_some() || method.is_none() {
        None
    } else {
        Some(template)
    };
    ConsumerCall {
        component,
        file,
        function: function.name.clone(),
        method,
        path,
        path_text,
        dynamic_start,
        target,
        call,
        steps,
        assumptions,
        reasons,
        response,
        request,
        tests: Vec::new(),
    }
}

fn display_dir(dir: &str) -> String {
    if dir.is_empty() {
        ".".to_owned()
    } else {
        format!("./{dir}")
    }
}

fn lookup_constant<'w>(
    ws: &'w Workspace,
    file: usize,
    name: &str,
    depth: usize,
) -> Option<(usize, &'w typescript::Constant)> {
    let facts = ws.ts(file)?;
    if let Some(c) = facts.constants.iter().find(|c| c.name == name) {
        return Some((file, c));
    }
    if depth >= MAX_DEPTH {
        return None;
    }
    let (import, binding) = facts.imports.iter().find_map(|i| {
        i.bindings
            .iter()
            .find(|b| b.local == name && b.imported != "*")
            .map(|b| (i, b))
    })?;
    let target = ws.resolve_ts_module(file, &import.module)?;
    lookup_constant(ws, target, &binding.imported, depth + 1)
}

pub fn lookup_function<'w>(
    ws: &'w Workspace,
    file: usize,
    callee: &str,
    depth: usize,
) -> Option<(usize, &'w Function)> {
    let facts = ws.ts(file)?;
    if depth >= MAX_DEPTH {
        return None;
    }
    if let Some((namespace, member)) = callee.split_once('.') {
        let import = facts.imports.iter().find(|i| {
            i.bindings
                .iter()
                .any(|b| b.local == namespace && b.imported == "*")
        })?;
        let target = ws.resolve_ts_module(file, &import.module)?;
        return lookup_function(ws, target, member, depth + 1);
    }
    if let Some(f) = facts
        .functions
        .iter()
        .find(|f| f.name == callee && !f.name.is_empty())
    {
        return Some((file, f));
    }
    let (import, binding) = facts.imports.iter().find_map(|i| {
        i.bindings
            .iter()
            .find(|b| b.local == callee && b.imported != "*" && b.imported != "default")
            .map(|b| (i, b))
    })?;
    let target = ws.resolve_ts_module(file, &import.module)?;
    lookup_function(ws, target, &binding.imported, depth + 1)
}

fn lookup_type<'w>(
    ws: &'w Workspace,
    file: usize,
    name: &str,
    depth: usize,
) -> Option<(usize, &'w TypeDecl)> {
    let facts = ws.ts(file)?;
    if let Some(t) = facts.types.iter().find(|t| t.name == name) {
        return Some((file, t));
    }
    if depth >= MAX_DEPTH {
        return None;
    }
    let (import, binding) = facts.imports.iter().find_map(|i| {
        i.bindings
            .iter()
            .find(|b| b.local == name && b.imported != "*")
            .map(|b| (i, b))
    })?;
    let target = ws.resolve_ts_module(file, &import.module)?;
    lookup_type(ws, target, &binding.imported, depth + 1)
}

/// Resolves the declared types a shape names, in the context of `file`.
fn resolve(
    ws: &Workspace,
    file: usize,
    shape: &Shape,
    depth: usize,
    seen: &mut Vec<(usize, String)>,
) -> Shape {
    if depth > MAX_DEPTH {
        return Shape::unknown("nested too deeply");
    }
    match shape {
        Shape::Named { name } => match lookup_type(ws, file, name, 0) {
            Some((tfile, decl)) => {
                let key = (tfile, decl.name.clone());
                if seen.contains(&key) {
                    return shape.clone();
                }
                seen.push(key);
                let mut resolved = resolve(ws, tfile, &decl.shape, depth + 1, seen);
                if let Shape::Object(object) = &mut resolved {
                    let mut inherited: Vec<seam_core::Field> = Vec::new();
                    for base in &decl.extends {
                        if let Shape::Object(base_obj) = resolve(
                            ws,
                            tfile,
                            &Shape::Named { name: base.clone() },
                            depth + 1,
                            seen,
                        ) {
                            inherited.extend(base_obj.fields);
                        }
                    }
                    if !inherited.is_empty() {
                        inherited.retain(|f| object.field(&f.json_name).is_none());
                        inherited.append(&mut object.fields);
                        object.fields = inherited;
                    }
                }
                seen.pop();
                resolved
            }
            None => Shape::unknown(format!("{name} is not declared in an analysed file")),
        },
        Shape::Object(object) => Shape::Object(ObjectShape {
            name: object.name.clone(),
            fields: object
                .fields
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.shape = resolve(ws, file, &f.shape, depth + 1, seen);
                    f
                })
                .collect(),
        }),
        Shape::Array { items } => Shape::array(resolve(ws, file, items, depth + 1, seen)),
        Shape::Union { variants } => Shape::Union {
            variants: variants
                .iter()
                .map(|v| resolve(ws, file, v, depth + 1, seen))
                .collect(),
        },
        other => other.clone(),
    }
}

/// The consumer's type, resolved, with evidence at its declaration when it
/// names a declared type and at `fallback` otherwise.
fn consumer_typed(
    ws: &Workspace,
    file: usize,
    shape: &Shape,
    fallback: &Located,
    response: bool,
) -> Option<Typed> {
    let resolved = resolve(ws, file, shape, 0, &mut Vec::new());
    if resolved == Shape::Any {
        return None; // `any` and `unknown` say nothing about the payload
    }
    let name = shape.describe();
    let named = match shape {
        Shape::Named { name } => Some(name.clone()),
        Shape::Array { items } => match items.as_ref() {
            Shape::Named { name } => Some(name.clone()),
            _ => None,
        },
        _ => None,
    };
    let loc = named
        .and_then(|n| lookup_type(ws, file, &n, 0))
        .map_or_else(|| fallback.clone(), |(_, decl)| decl.loc.clone());
    let note = if response {
        ws.say(
            format!("The consumer reads the response as {name}."),
            format!("مصرف‌کننده پاسخ را به‌صورت {name} می‌خواند."),
        )
    } else {
        ws.say(
            format!("The consumer sends the request body as {name}."),
            format!("مصرف‌کننده بدنهٔ درخواست را به‌صورت {name} می‌فرستد."),
        )
    };
    Some(Typed {
        shape: resolved,
        evidence: ws.evidence(Role::ConsumerType, &loc, ADAPTER, "type:declaration", note),
    })
}

fn param_typed(ws: &Workspace, file: usize, function: &Function, name: &str) -> Option<Typed> {
    let shape = function
        .params
        .iter()
        .find(|p| p.name == name)?
        .shape
        .as_ref()?;
    consumer_typed(ws, file, shape, &function.loc, false)
}

/// Links test cases to the functions that make the requests they exercise.
fn attach_tests(ws: &Workspace, calls: &mut [ConsumerCall]) {
    for file in 0..ws.files.len() {
        let Some(facts) = ws.ts(file) else { continue };
        for test in &facts.tests {
            for callee in &test.calls {
                let Some((target_file, target)) = lookup_function(ws, file, callee, 0) else {
                    continue;
                };
                for call in calls
                    .iter_mut()
                    .filter(|c| c.file == target_file && c.function == target.name)
                {
                    let note = ws.say(
                        format!("The test \"{}\" calls {}.", test.name, target.name),
                        format!("آزمون «{}» تابع {} را صدا می‌زند.", test.name, target.name),
                    );
                    let ev =
                        ws.evidence(Role::Test, &test.loc, ADAPTER, "call-expression:test", note);
                    if !call.tests.iter().any(|(_, e)| e == &ev) {
                        call.tests.push((test.name.clone(), ev));
                    }
                }
            }
        }
    }
}
