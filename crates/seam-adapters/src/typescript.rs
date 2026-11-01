//! TypeScript and JavaScript: HTTP requests made with `fetch` or `axios`,
//! the functions that wrap them, the constants that hold URLs, interfaces
//! and type aliases lowered to shapes, imports, and the calls tests make.
//!
//! The adapter does not decide which calls are Seams. It records every
//! request and every call to a named function, with the arguments that can
//! contribute to a URL; the linker follows wrappers across files.

use std::str::FromStr;

use seam_core::{Field, HttpMethod, ObjectShape, Shape};
use serde::{Deserialize, Serialize};
use tree_sitter::{Language, Node};

use crate::syntax::{self, Located, Source, named_children, visit};

pub const ADAPTER: &str = "typescript@0.1";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsFacts {
    pub imports: Vec<TsImport>,
    pub constants: Vec<Constant>,
    pub functions: Vec<Function>,
    pub types: Vec<TypeDecl>,
    pub tests: Vec<TestCase>,
    pub is_test_file: bool,
    /// The parser recovered from syntax errors; facts come from the parts that parsed.
    pub syntax_errors: bool,
}

/// `import { a as b } from "./x"`. `imported` is `default` for a default
/// import and `*` for a namespace import.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsImport {
    pub module: String,
    pub bindings: Vec<Binding>,
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub imported: String,
    pub local: String,
}

/// A module-level `const` whose value may contribute to a URL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constant {
    pub name: String,
    pub value: Expr,
    pub loc: Located,
}

/// What an expression contributes to a URL.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Expr {
    Text {
        value: String,
    },
    /// A template literal or a `+` concatenation, in order.
    Concat {
        parts: Vec<Expr>,
    },
    /// `import.meta.env.NAME` or `process.env.NAME`, with the value after
    /// `??` or `||` when there is one.
    Env {
        name: String,
        fallback: Option<Box<Expr>>,
    },
    /// A plain identifier: a parameter, a constant or an import.
    Ref {
        name: String,
    },
    /// Anything else, kept as source text. Its value is known only at run time.
    Unknown {
        text: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub params: Vec<TsParam>,
    pub type_params: Vec<String>,
    /// The declared return type with `Promise<...>` removed.
    pub returns: Option<Shape>,
    pub requests: Vec<Request>,
    pub calls: Vec<CallSite>,
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TsParam {
    pub name: String,
    pub shape: Option<Shape>,
}

/// A request made directly with an HTTP client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// `fetch` or `axios`.
    pub client: String,
    /// `None` when the method is given but is not a literal.
    pub method: Option<HttpMethod>,
    pub url: Expr,
    /// The value sent as the body, unwrapped from `JSON.stringify`.
    pub body: Option<Expr>,
    /// The response type when the call itself names one, as in `axios.get<T>`.
    pub response: Option<Shape>,
    pub loc: Located,
}

/// A call to a named function, which may turn out to be a request wrapper.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    /// `getJson`, or `api.getJson` for a member of a namespace import.
    pub callee: String,
    pub type_args: Vec<Shape>,
    pub args: Vec<Expr>,
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeDecl {
    pub name: String,
    pub shape: Shape,
    /// Interfaces this one extends; the linker merges their fields.
    pub extends: Vec<String>,
    pub loc: Located,
}

/// An `it(...)` or `test(...)` block and the functions it calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    pub calls: Vec<String>,
    pub loc: Located,
}

pub fn is_test_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    stem.ends_with(".test") || stem.ends_with(".spec") || path.contains("/__tests__/")
}

/// The grammar for a file: plain TypeScript for `.ts`, and the TSX grammar,
/// which also reads JavaScript and JSX, for everything else.
pub fn language_for(path: &str) -> Language {
    if path.ends_with(".ts") || path.ends_with(".mts") || path.ends_with(".cts") {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    }
}

pub fn extract(source: &Source<'_>) -> Result<TsFacts, syntax::ParseError> {
    let tree = syntax::parse(&language_for(source.path), source.text)?;
    let root = tree.root_node();
    let mut facts = TsFacts {
        is_test_file: is_test_path(source.path),
        syntax_errors: root.has_error(),
        ..TsFacts::default()
    };
    let mut module_scope = Function {
        name: String::new(),
        params: vec![],
        type_params: vec![],
        returns: None,
        requests: vec![],
        calls: vec![],
        loc: source.locate(root),
    };
    for statement in named_children(root) {
        let declaration = if statement.kind() == "export_statement" {
            match statement.child_by_field_name("declaration") {
                Some(d) => d,
                None => continue,
            }
        } else {
            statement
        };
        match declaration.kind() {
            "import_statement" => {
                if let Some(import) = import(source, declaration) {
                    facts.imports.push(import);
                }
            }
            "function_declaration" | "generator_function_declaration" => {
                if let Some(f) = function(source, declaration, declaration, None) {
                    facts.functions.push(f);
                }
            }
            "lexical_declaration" | "variable_declaration" => {
                declarators(source, statement, declaration, &mut facts)
            }
            "interface_declaration" => facts.types.push(interface(source, declaration)),
            "type_alias_declaration" => {
                if let Some(t) = type_alias(source, declaration) {
                    facts.types.push(t);
                }
            }
            _ => body_calls(source, declaration, &mut module_scope),
        }
    }
    if facts.is_test_file {
        facts.tests = tests(source, root);
    }
    if !module_scope.requests.is_empty() || !module_scope.calls.is_empty() {
        facts.functions.push(module_scope);
    }
    Ok(facts)
}

fn import(source: &Source<'_>, node: Node<'_>) -> Option<TsImport> {
    let module = node
        .child_by_field_name("source")
        .and_then(|s| syntax::string_value(source, s))?;
    let mut bindings = Vec::new();
    for clause in named_children(node)
        .into_iter()
        .filter(|n| n.kind() == "import_clause")
    {
        for part in named_children(clause) {
            match part.kind() {
                "identifier" => bindings.push(Binding {
                    imported: "default".into(),
                    local: source.text(part).to_owned(),
                }),
                "namespace_import" => {
                    if let Some(name) = named_children(part).into_iter().next() {
                        bindings.push(Binding {
                            imported: "*".into(),
                            local: source.text(name).to_owned(),
                        });
                    }
                }
                "named_imports" => {
                    for spec in named_children(part)
                        .into_iter()
                        .filter(|s| s.kind() == "import_specifier")
                    {
                        let Some(name) = spec.child_by_field_name("name") else {
                            continue;
                        };
                        let imported = source.text(name).to_owned();
                        let local = spec
                            .child_by_field_name("alias")
                            .map(|a| source.text(a).to_owned())
                            .unwrap_or_else(|| imported.clone());
                        bindings.push(Binding { imported, local });
                    }
                }
                _ => {}
            }
        }
    }
    Some(TsImport {
        module,
        bindings,
        loc: source.locate(node),
    })
}

/// `const NAME = ...`: a function when the value is one, otherwise a
/// constant when the value can contribute to a URL.
fn declarators(source: &Source<'_>, statement: Node<'_>, node: Node<'_>, facts: &mut TsFacts) {
    for declarator in named_children(node)
        .into_iter()
        .filter(|d| d.kind() == "variable_declarator")
    {
        let (Some(name), Some(value)) = (
            declarator.child_by_field_name("name"),
            declarator.child_by_field_name("value"),
        ) else {
            continue;
        };
        if name.kind() != "identifier" {
            continue;
        }
        let name = source.text(name).to_owned();
        match value.kind() {
            "arrow_function" | "function_expression" | "function" => {
                if let Some(f) = function(source, statement, value, Some(name)) {
                    facts.functions.push(f);
                }
            }
            _ => {
                let value = expr(source, value);
                if !matches!(value, Expr::Unknown { .. }) {
                    facts.constants.push(Constant {
                        name,
                        value,
                        loc: source.locate(statement),
                    });
                }
            }
        }
    }
}

fn function(
    source: &Source<'_>,
    header: Node<'_>,
    node: Node<'_>,
    name: Option<String>,
) -> Option<Function> {
    let name = name.or_else(|| {
        node.child_by_field_name("name")
            .map(|n| source.text(n).to_owned())
    })?;
    let type_params = node
        .child_by_field_name("type_parameters")
        .map(|tp| {
            named_children(tp)
                .into_iter()
                .filter_map(|p| p.child_by_field_name("name"))
                .map(|n| source.text(n).to_owned())
                .collect()
        })
        .unwrap_or_default();
    let params = node
        .child_by_field_name("parameters")
        .map(|p| params(source, p))
        .unwrap_or_default();
    let returns = node
        .child_by_field_name("return_type")
        .and_then(|t| named_children(t).into_iter().next())
        .map(|t| lower_type(source, t));
    let header_end = node
        .child_by_field_name("return_type")
        .or_else(|| node.child_by_field_name("parameters"))
        .unwrap_or(node);
    let mut f = Function {
        name,
        params,
        type_params,
        returns,
        requests: vec![],
        calls: vec![],
        loc: source.locate_between(header, header_end),
    };
    if let Some(body) = node.child_by_field_name("body") {
        body_calls(source, body, &mut f);
    }
    Some(f)
}

fn params(source: &Source<'_>, node: Node<'_>) -> Vec<TsParam> {
    named_children(node)
        .into_iter()
        .filter(|p| matches!(p.kind(), "required_parameter" | "optional_parameter"))
        .filter_map(|p| {
            let pattern = p.child_by_field_name("pattern")?;
            let shape = p
                .child_by_field_name("type")
                .and_then(|t| named_children(t).into_iter().next())
                .map(|t| lower_type(source, t));
            Some(TsParam {
                name: source.text(pattern).to_owned(),
                shape,
            })
        })
        .collect()
}

const AXIOS_METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

/// Every request and named call inside `node`, attributed to `f`.
fn body_calls(source: &Source<'_>, node: Node<'_>, f: &mut Function) {
    visit(node, &mut |n| {
        if n.kind() != "call_expression" {
            return;
        }
        let Some(callee) = n.child_by_field_name("function") else {
            return;
        };
        let args: Vec<Node<'_>> = n
            .child_by_field_name("arguments")
            .map(named_children)
            .unwrap_or_default();
        let type_args: Vec<Shape> = n
            .child_by_field_name("type_arguments")
            .map(|t| {
                named_children(t)
                    .into_iter()
                    .map(|a| lower_type(source, a))
                    .collect()
            })
            .unwrap_or_default();
        let callee_text = source.text(callee);
        if matches!(callee_text, "fetch" | "window.fetch" | "globalThis.fetch") {
            if let Some(url) = args.first() {
                let init = args.get(1).filter(|i| i.kind() == "object");
                let method = match init.and_then(|i| property(source, *i, "method")) {
                    None => Some(HttpMethod::Get),
                    Some(m) => syntax::string_value(source, m)
                        .and_then(|m| HttpMethod::from_str(&m.to_ascii_uppercase()).ok()),
                };
                let body = init
                    .and_then(|i| property(source, *i, "body"))
                    .map(|b| body_expr(source, b));
                f.requests.push(Request {
                    client: "fetch".into(),
                    method,
                    url: expr(source, *url),
                    body,
                    response: None,
                    loc: source.locate(n),
                });
            }
            return;
        }
        if callee.kind() == "member_expression" {
            let object = callee
                .child_by_field_name("object")
                .map(|o| source.text(o))
                .unwrap_or_default();
            let verb = callee
                .child_by_field_name("property")
                .map(|p| source.text(p))
                .unwrap_or_default();
            if object == "axios" && AXIOS_METHODS.contains(&verb) {
                if let Some(url) = args.first() {
                    let body = matches!(verb, "post" | "put" | "patch")
                        .then(|| args.get(1).map(|b| body_expr(source, *b)))
                        .flatten();
                    f.requests.push(Request {
                        client: "axios".into(),
                        method: HttpMethod::from_str(&verb.to_ascii_uppercase()).ok(),
                        url: expr(source, *url),
                        body,
                        response: type_args.first().cloned(),
                        loc: source.locate(n),
                    });
                }
                return;
            }
        }
        if matches!(callee.kind(), "identifier" | "member_expression")
            && !callee_text.contains(['(', '\n'])
        {
            f.calls.push(CallSite {
                callee: callee_text.to_owned(),
                type_args,
                args: args.iter().map(|a| expr(source, *a)).collect(),
                loc: source.locate(n),
            });
        }
    });
}

fn property<'t>(source: &Source<'_>, object: Node<'t>, key: &str) -> Option<Node<'t>> {
    named_children(object)
        .into_iter()
        .filter(|p| p.kind() == "pair")
        .find(|p| {
            p.child_by_field_name("key").is_some_and(|k| {
                let text = source.text(k);
                text == key || syntax::string_value(source, k).as_deref() == Some(key)
            })
        })
        .and_then(|p| p.child_by_field_name("value"))
}

/// `JSON.stringify(x)` sends `x`.
fn body_expr(source: &Source<'_>, node: Node<'_>) -> Expr {
    if node.kind() == "call_expression"
        && node
            .child_by_field_name("function")
            .is_some_and(|f| source.text(f) == "JSON.stringify")
        && let Some(inner) = node
            .child_by_field_name("arguments")
            .and_then(|a| named_children(a).into_iter().next())
    {
        return expr(source, inner);
    }
    expr(source, node)
}

pub fn expr(source: &Source<'_>, node: Node<'_>) -> Expr {
    match node.kind() {
        "string" => match syntax::string_value(source, node) {
            Some(value) => Expr::Text { value },
            None => unknown(source, node),
        },
        "template_string" => {
            let mut parts = Vec::new();
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "string_fragment" | "escape_sequence" => parts.push(Expr::Text {
                        value: source.text(child).to_owned(),
                    }),
                    "template_substitution" => {
                        if let Some(inner) = named_children(child).into_iter().next() {
                            parts.push(expr(source, inner));
                        }
                    }
                    _ => {}
                }
            }
            concat(parts)
        }
        "binary_expression" => {
            let operator = node
                .child_by_field_name("operator")
                .map(|o| source.text(o))
                .unwrap_or_default();
            let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ) else {
                return unknown(source, node);
            };
            match operator {
                "+" => concat(vec![expr(source, left), expr(source, right)]),
                "??" | "||" => match expr(source, left) {
                    Expr::Env {
                        name,
                        fallback: None,
                    } => Expr::Env {
                        name,
                        fallback: Some(Box::new(expr(source, right))),
                    },
                    _ => unknown(source, node),
                },
                _ => unknown(source, node),
            }
        }
        "member_expression" => {
            let text = source.text(node);
            for prefix in ["import.meta.env.", "process.env."] {
                if let Some(name) = text.strip_prefix(prefix)
                    && !name.is_empty()
                    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                {
                    return Expr::Env {
                        name: name.to_owned(),
                        fallback: None,
                    };
                }
            }
            unknown(source, node)
        }
        "identifier" => Expr::Ref {
            name: source.text(node).to_owned(),
        },
        "parenthesized_expression"
        | "as_expression"
        | "satisfies_expression"
        | "non_null_expression" => match named_children(node).into_iter().next() {
            Some(inner) => expr(source, inner),
            None => unknown(source, node),
        },
        _ => unknown(source, node),
    }
}

fn unknown(source: &Source<'_>, node: Node<'_>) -> Expr {
    Expr::Unknown {
        text: source.text(node).to_owned(),
    }
}

/// Flattens nested concatenations and merges neighbouring text.
fn concat(parts: Vec<Expr>) -> Expr {
    let mut out: Vec<Expr> = Vec::new();
    for part in parts {
        let pieces = match part {
            Expr::Concat { parts } => parts,
            other => vec![other],
        };
        for piece in pieces {
            match (out.last_mut(), piece) {
                (Some(Expr::Text { value: last }), Expr::Text { value }) => last.push_str(&value),
                (_, piece) => out.push(piece),
            }
        }
    }
    if out.len() == 1 {
        out.remove(0)
    } else {
        Expr::Concat { parts: out }
    }
}

fn interface(source: &Source<'_>, node: Node<'_>) -> TypeDecl {
    let name = node
        .child_by_field_name("name")
        .map(|n| source.text(n).to_owned())
        .unwrap_or_default();
    let extends = named_children(node)
        .into_iter()
        .filter(|n| n.kind() == "extends_type_clause")
        .flat_map(named_children)
        .map(|t| {
            let text = source.text(t);
            text.split('<').next().unwrap_or(text).trim().to_owned()
        })
        .collect();
    let fields = node
        .child_by_field_name("body")
        .map(|b| object_fields(source, b))
        .unwrap_or_default();
    TypeDecl {
        shape: Shape::Object(ObjectShape {
            name: Some(name.clone()),
            fields,
        }),
        name,
        extends,
        loc: source.locate(node),
    }
}

fn type_alias(source: &Source<'_>, node: Node<'_>) -> Option<TypeDecl> {
    let name = source.text(node.child_by_field_name("name")?).to_owned();
    let mut shape = lower_type(source, node.child_by_field_name("value")?);
    if let Shape::Object(object) = &mut shape {
        object.name = Some(name.clone());
    }
    Some(TypeDecl {
        name,
        shape,
        extends: vec![],
        loc: source.locate(node),
    })
}

fn object_fields(source: &Source<'_>, body: Node<'_>) -> Vec<Field> {
    named_children(body)
        .into_iter()
        .filter(|p| p.kind() == "property_signature")
        .filter_map(|p| {
            let key = p.child_by_field_name("name")?;
            let name =
                syntax::string_value(source, key).unwrap_or_else(|| source.text(key).to_owned());
            let mut cursor = p.walk();
            let optional = p.children(&mut cursor).any(|c| c.kind() == "?");
            let declared = p
                .child_by_field_name("type")
                .and_then(|t| named_children(t).into_iter().next())
                .map(|t| lower_type(source, t))
                .unwrap_or(Shape::Any);
            let (without_undefined, had_undefined) = strip_variant(
                declared,
                &Shape::Unknown {
                    reason: UNDEFINED.into(),
                },
            );
            let (shape, nullable) = without_undefined.without_null();
            Some(Field {
                json_name: name.clone(),
                declared_name: name,
                shape,
                required: !optional && !had_undefined,
                nullable,
                span: Some(source.span(p)),
            })
        })
        .collect()
}

/// `undefined` lowers to this marker so a property can tell "may be absent"
/// from "may be null"; it never leaves the adapter.
const UNDEFINED: &str = "undefined";

fn strip_variant(shape: Shape, variant: &Shape) -> (Shape, bool) {
    match shape {
        Shape::Union { variants } if variants.contains(variant) => {
            let mut rest: Vec<Shape> = variants.into_iter().filter(|v| v != variant).collect();
            let shape = match rest.len() {
                0 => Shape::Any,
                1 => rest.remove(0),
                _ => Shape::Union { variants: rest },
            };
            (shape, true)
        }
        other if &other == variant => (Shape::Any, true),
        other => (other, false),
    }
}

/// Lowers a TypeScript type to a shape. References to declared types stay
/// `Named` for the linker to resolve.
pub fn lower_type(source: &Source<'_>, node: Node<'_>) -> Shape {
    match node.kind() {
        "predefined_type" => match source.text(node) {
            "number" => Shape::Number,
            "string" => Shape::String,
            "boolean" => Shape::Boolean,
            "any" | "unknown" | "object" => Shape::Any,
            "null" => Shape::Null,
            "undefined" | "void" => Shape::Unknown {
                reason: UNDEFINED.into(),
            },
            other => Shape::unknown(format!("{other} has no JSON form")),
        },
        "type_identifier" => match source.text(node) {
            "Date" => Shape::unknown("Date is not a JSON type; JSON carries dates as strings"),
            name => Shape::Named {
                name: name.to_owned(),
            },
        },
        "nested_type_identifier" => Shape::Named {
            name: source.text(node).to_owned(),
        },
        "generic_type" => {
            let base = node
                .child_by_field_name("name")
                .map(|n| source.text(n))
                .unwrap_or_default();
            let args: Vec<Shape> = node
                .child_by_field_name("type_arguments")
                .map(|a| {
                    named_children(a)
                        .into_iter()
                        .map(|t| lower_type(source, t))
                        .collect()
                })
                .unwrap_or_default();
            match (base, args.as_slice()) {
                ("Array" | "ReadonlyArray", [item]) => Shape::array(item.clone()),
                ("Promise", [inner]) => inner.clone(),
                ("Record" | "Map", _) => Shape::Any,
                _ => Shape::unknown(format!(
                    "the generic type {} is not resolved",
                    source.text(node)
                )),
            }
        }
        "array_type" => match named_children(node).into_iter().next() {
            Some(item) => Shape::array(lower_type(source, item)),
            None => Shape::unknown("an array type without an element type"),
        },
        "union_type" => {
            let mut variants = Vec::new();
            for part in named_children(node) {
                match lower_type(source, part) {
                    Shape::Union { variants: inner } => variants.extend(inner),
                    other => variants.push(other),
                }
            }
            union(variants)
        }
        "literal_type" => match named_children(node).into_iter().next().map(|n| n.kind()) {
            Some("string" | "template_string") => Shape::String,
            Some("number") => Shape::Number,
            Some("true" | "false") => Shape::Boolean,
            Some("null") => Shape::Null,
            Some("undefined") => Shape::Unknown {
                reason: UNDEFINED.into(),
            },
            _ => match source.text(node) {
                "null" => Shape::Null,
                "undefined" => Shape::Unknown {
                    reason: UNDEFINED.into(),
                },
                "true" | "false" => Shape::Boolean,
                _ => Shape::unknown(format!(
                    "the literal type {} is not read",
                    source.text(node)
                )),
            },
        },
        "object_type" => Shape::Object(ObjectShape {
            name: None,
            fields: object_fields(source, node),
        }),
        "parenthesized_type" | "readonly_type" => match named_children(node).into_iter().next() {
            Some(inner) => lower_type(source, inner),
            None => Shape::unknown("an empty type"),
        },
        _ => Shape::unknown(format!("the type {} is not read", source.text(node))),
    }
}

fn union(variants: Vec<Shape>) -> Shape {
    let mut out: Vec<Shape> = Vec::new();
    for v in variants {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    if out.len() == 1 {
        out.remove(0)
    } else {
        Shape::Union { variants: out }
    }
}

fn tests(source: &Source<'_>, root: Node<'_>) -> Vec<TestCase> {
    let mut out = Vec::new();
    visit(root, &mut |n| {
        if n.kind() != "call_expression" {
            return;
        }
        let callee = n
            .child_by_field_name("function")
            .map(|f| source.text(f))
            .unwrap_or_default();
        if !matches!(callee, "it" | "test" | "it.only" | "test.only") {
            return;
        }
        let args = n
            .child_by_field_name("arguments")
            .map(named_children)
            .unwrap_or_default();
        let (Some(name), Some(body)) = (
            args.first().and_then(|a| syntax::string_value(source, *a)),
            args.get(1),
        ) else {
            return;
        };
        let mut calls = Vec::new();
        visit(*body, &mut |c| {
            if c.kind() != "call_expression" {
                return;
            }
            if let Some(f) = c.child_by_field_name("function")
                && matches!(f.kind(), "identifier" | "member_expression")
            {
                let text = source.text(f).to_owned();
                if !calls.contains(&text) {
                    calls.push(text);
                }
            }
        });
        out.push(TestCase {
            name,
            calls,
            loc: source.locate(n),
        });
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(path: &str, text: &str) -> TsFacts {
        extract(&Source::new(path, text)).expect("the TypeScript grammar loads")
    }

    const HTTP: &str = r#"const BASE_URL: string = import.meta.env.VITE_API_URL ?? "http://localhost:8000";

export async function getJson<T>(path: string): Promise<T> {
  const response = await fetch(`${BASE_URL}${path}`, {
    headers: { Accept: "application/json" },
  });
  return (await response.json()) as T;
}

export async function postJson<T>(path: string, body: unknown): Promise<T> {
  const response = await fetch(`${BASE_URL}${path}`, {
    method: "POST",
    body: JSON.stringify(body),
  });
  return (await response.json()) as T;
}
"#;

    #[test]
    fn reads_base_url_constants_with_their_fallback() {
        let f = facts("web/src/api/http.ts", HTTP);
        assert_eq!(f.constants.len(), 1);
        assert_eq!(
            f.constants[0].value,
            Expr::Env {
                name: "VITE_API_URL".into(),
                fallback: Some(Box::new(Expr::Text {
                    value: "http://localhost:8000".into()
                })),
            }
        );
    }

    #[test]
    fn reads_wrappers_with_their_requests() {
        let f = facts("web/src/api/http.ts", HTTP);
        let get = f
            .functions
            .iter()
            .find(|f| f.name == "getJson")
            .expect("getJson");
        assert_eq!(get.type_params, vec!["T".to_owned()]);
        assert_eq!(get.returns, Some(Shape::Named { name: "T".into() }));
        assert_eq!(get.requests.len(), 1);
        let r = &get.requests[0];
        assert_eq!(r.method, Some(HttpMethod::Get));
        assert_eq!(
            r.url,
            Expr::Concat {
                parts: vec![
                    Expr::Ref {
                        name: "BASE_URL".into()
                    },
                    Expr::Ref {
                        name: "path".into()
                    }
                ]
            }
        );
        assert_eq!(r.loc.span.start.line, 4);
        let post = f
            .functions
            .iter()
            .find(|f| f.name == "postJson")
            .expect("postJson");
        assert_eq!(post.requests[0].method, Some(HttpMethod::Post));
        assert_eq!(
            post.requests[0].body,
            Some(Expr::Ref {
                name: "body".into()
            })
        );
    }

    #[test]
    fn reads_calls_to_wrappers_with_type_arguments_and_url_templates() {
        let f = facts(
            "web/src/api/accounts.ts",
            "import { getJson } from \"./http\";\nimport type { Account } from \"./types\";\n\nexport function fetchAccount(accountId: number): Promise<Account> {\n  return getJson<Account>(`/accounts/${accountId}`);\n}\n",
        );
        assert_eq!(f.imports.len(), 2);
        assert_eq!(f.imports[1].bindings[0].local, "Account");
        let fetch_account = &f.functions[0];
        assert_eq!(fetch_account.params[0].shape, Some(Shape::Number));
        assert_eq!(
            fetch_account.returns,
            Some(Shape::Named {
                name: "Account".into()
            })
        );
        let call = &fetch_account.calls[0];
        assert_eq!(call.callee, "getJson");
        assert_eq!(
            call.type_args,
            vec![Shape::Named {
                name: "Account".into()
            }]
        );
        assert_eq!(
            call.args[0],
            Expr::Concat {
                parts: vec![
                    Expr::Text {
                        value: "/accounts/".into()
                    },
                    Expr::Ref {
                        name: "accountId".into()
                    }
                ]
            }
        );
        assert_eq!(call.loc.span.start.line, 5);
    }

    #[test]
    fn lowers_interfaces_with_optional_nullable_and_literal_fields() {
        let f = facts(
            "web/src/api/types.ts",
            "export interface Receipt extends Base {\n  paymentId: string;\n  status: \"accepted\" | \"rejected\";\n  ledger_entry_id: number | null;\n  note?: string;\n  tags: Array<string>;\n  lines: Line[];\n  maybe: number | undefined;\n}\nexport type Id = string;\n",
        );
        let receipt = &f.types[0];
        assert_eq!(receipt.extends, vec!["Base".to_owned()]);
        let Shape::Object(object) = &receipt.shape else {
            panic!("an object")
        };
        let field = |n: &str| object.field(n).expect(n).clone();
        assert_eq!(field("status").shape, Shape::String);
        assert!(field("ledger_entry_id").nullable && field("ledger_entry_id").required);
        assert_eq!(field("ledger_entry_id").shape, Shape::Number);
        assert!(!field("note").required);
        assert!(!field("maybe").required && !field("maybe").nullable);
        assert_eq!(field("tags").shape, Shape::array(Shape::String));
        assert_eq!(
            field("lines").shape,
            Shape::array(Shape::Named {
                name: "Line".into()
            })
        );
        assert_eq!(
            field("paymentId").span.as_ref().map(|s| s.start.line),
            Some(2)
        );
        assert_eq!(f.types[1].shape, Shape::String);
    }

    #[test]
    fn a_path_built_from_runtime_data_keeps_its_unknown_parts() {
        let f = facts(
            "web/src/api/search.ts",
            "export async function search(collection: string, query: string) {\n  const r = await fetch(`${BASE_URL}/${collection}/search?q=${encodeURIComponent(query)}`);\n  return r.json();\n}\n",
        );
        let r = &f.functions[0].requests[0];
        let Expr::Concat { parts } = &r.url else {
            panic!("a template")
        };
        assert_eq!(
            parts[0],
            Expr::Ref {
                name: "BASE_URL".into()
            }
        );
        assert_eq!(
            parts[2],
            Expr::Ref {
                name: "collection".into()
            }
        );
        assert!(matches!(parts[4], Expr::Unknown { .. }));
    }

    #[test]
    fn reads_axios_and_arrow_functions() {
        let f = facts(
            "web/src/users.js",
            "export const loadUser = async (id) => axios.get(`/users/${id}`);\nexport const save = (u) => axios.post(\"/users\", u);\n",
        );
        assert_eq!(f.functions.len(), 2);
        assert_eq!(f.functions[0].requests[0].method, Some(HttpMethod::Get));
        assert_eq!(
            f.functions[1].requests[0].body,
            Some(Expr::Ref { name: "u".into() })
        );
    }

    #[test]
    fn reads_test_cases_in_test_files_only() {
        let text = "import { fetchAccount } from \"./accounts\";\ndescribe(\"x\", () => {\n  it(\"reads an account\", async () => {\n    const a = await fetchAccount(7);\n    expect(a.name).toBe(\"Ada\");\n  });\n});\n";
        let f = facts("web/src/api/accounts.test.ts", text);
        assert!(f.is_test_file);
        assert_eq!(f.tests.len(), 1);
        assert_eq!(f.tests[0].name, "reads an account");
        assert!(f.tests[0].calls.contains(&"fetchAccount".to_owned()));
        assert!(facts("web/src/api/accounts.ts", text).tests.is_empty());
    }

    #[test]
    fn commented_and_string_urls_are_not_requests() {
        let f = facts(
            "web/src/notes.ts",
            "// fetch(\"/accounts\")\nconst help = \"call fetch('/accounts') to load\";\nexport function noop() { return help; }\n",
        );
        assert!(f.functions.iter().all(|f| f.requests.is_empty()));
    }

    #[test]
    fn malformed_typescript_does_not_panic() {
        let f = facts(
            "web/src/broken.ts",
            "export function (x: { => fetch(`/a/${\ninterface {\n",
        );
        assert!(f.functions.len() <= 2);
    }
}
