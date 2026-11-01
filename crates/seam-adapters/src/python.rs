//! Python: FastAPI applications, routers and routes, class fields (Pydantic
//! models are recognised by the linker from their bases), imports, and the
//! requests tests make through a test client.

use std::str::FromStr;

use seam_core::{HttpMethod, Span};
use serde::{Deserialize, Serialize};
use tree_sitter::Node;

use crate::syntax::{self, Located, Source, named_children, visit};

pub const ADAPTER: &str = "python@0.1";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PythonFacts {
    pub imports: Vec<Import>,
    pub objects: Vec<AppObject>,
    pub routes: Vec<Route>,
    pub includes: Vec<Include>,
    pub classes: Vec<Class>,
    pub test_requests: Vec<TestRequest>,
    pub is_test_file: bool,
    /// The parser recovered from syntax errors; facts come from the parts that parsed.
    pub syntax_errors: bool,
}

/// `from module import name as local`, or `import module as local` (then
/// `name` is `None`). Relative modules keep their leading dots.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    pub module: String,
    pub name: Option<String>,
    pub local: String,
    pub loc: Located,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    App,
    Router,
}

/// A module-level `name = FastAPI(...)` or `name = APIRouter(...)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppObject {
    pub name: String,
    pub kind: AppKind,
    pub prefix: Option<String>,
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Route {
    /// The object the decorator hangs off: `router` in `@router.get(...)`.
    pub object: String,
    pub method: HttpMethod,
    /// `None` when the path is not a string literal.
    pub path: Option<String>,
    pub response_model: Option<TypeExpr>,
    pub status_code: Option<u16>,
    pub handler: String,
    pub params: Vec<Param>,
    pub returns: Option<TypeExpr>,
    /// The decorator and the `def` line.
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    pub annotation: Option<TypeExpr>,
    /// The FastAPI marker in the default, such as `Header` or `Query`.
    pub marker: Option<String>,
}

/// `app.include_router(router, prefix="...")`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Include {
    pub app: String,
    pub router: Reference,
    pub prefix: Option<String>,
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reference {
    Name { name: String },
    Attribute { module: String, name: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Class {
    pub name: String,
    pub bases: Vec<String>,
    pub fields: Vec<ClassField>,
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassField {
    pub name: String,
    pub annotation: TypeExpr,
    pub has_default: bool,
    pub alias: Option<String>,
    pub serialization_alias: Option<String>,
    pub validation_alias: Option<String>,
    pub span: Span,
}

/// A type annotation or a type used as a value (`response_model=list[X]`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum TypeExpr {
    Name(String),
    Generic { base: String, args: Vec<TypeExpr> },
    Union(Vec<TypeExpr>),
    None,
    Other(String),
}

/// `client.get("/accounts/7")` inside a `test_*` function.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRequest {
    pub test: String,
    pub method: HttpMethod,
    /// The literal path; interpolated parts become `{}`.
    pub path: String,
    pub loc: Located,
}

pub fn is_test_path(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    file.starts_with("test_")
        || file.ends_with("_test.py")
        || path.contains("/tests/")
        || path.starts_with("tests/")
}

pub fn extract(source: &Source<'_>) -> Result<PythonFacts, syntax::ParseError> {
    let tree = syntax::parse(&tree_sitter_python::LANGUAGE.into(), source.text)?;
    let root = tree.root_node();
    let mut facts = PythonFacts {
        is_test_file: is_test_path(source.path),
        syntax_errors: root.has_error(),
        ..PythonFacts::default()
    };
    for statement in named_children(root) {
        match statement.kind() {
            "import_from_statement" | "import_statement" => {
                imports(source, statement, &mut facts.imports)
            }
            "expression_statement" => module_statement(source, statement, &mut facts),
            "decorated_definition" => decorated(source, statement, &mut facts),
            "class_definition" => facts.classes.push(class(source, statement)),
            "function_definition" if facts.is_test_file => {
                test_function(source, statement, &mut facts.test_requests)
            }
            _ => {}
        }
    }
    Ok(facts)
}

fn imports(source: &Source<'_>, node: Node<'_>, out: &mut Vec<Import>) {
    let loc = source.locate(node);
    if node.kind() == "import_statement" {
        let mut cursor = node.walk();
        for name in node.children_by_field_name("name", &mut cursor) {
            let (module, local) = match name.kind() {
                "aliased_import" => {
                    let module = name
                        .child_by_field_name("name")
                        .map(|n| source.text(n).to_owned())
                        .unwrap_or_default();
                    let local = name
                        .child_by_field_name("alias")
                        .map(|n| source.text(n).to_owned())
                        .unwrap_or_else(|| module.clone());
                    (module, local)
                }
                _ => {
                    let module = source.text(name).to_owned();
                    let local = module.split('.').next().unwrap_or(&module).to_owned();
                    (module, local)
                }
            };
            out.push(Import {
                module,
                name: None,
                local,
                loc: loc.clone(),
            });
        }
        return;
    }
    let Some(module_node) = node.child_by_field_name("module_name") else {
        return;
    };
    let module = source.text(module_node).to_owned();
    let mut cursor = node.walk();
    for name in node.children_by_field_name("name", &mut cursor) {
        let (imported, local) = match name.kind() {
            "aliased_import" => {
                let imported = name
                    .child_by_field_name("name")
                    .map(|n| source.text(n).to_owned())
                    .unwrap_or_default();
                let local = name
                    .child_by_field_name("alias")
                    .map(|n| source.text(n).to_owned())
                    .unwrap_or_else(|| imported.clone());
                (imported, local)
            }
            _ => (source.text(name).to_owned(), source.text(name).to_owned()),
        };
        out.push(Import {
            module: module.clone(),
            name: Some(imported),
            local,
            loc: loc.clone(),
        });
    }
}

fn module_statement(source: &Source<'_>, statement: Node<'_>, facts: &mut PythonFacts) {
    let Some(expr) = named_children(statement).into_iter().next() else {
        return;
    };
    match expr.kind() {
        "assignment" => {
            let (Some(left), Some(right)) = (
                expr.child_by_field_name("left"),
                expr.child_by_field_name("right"),
            ) else {
                return;
            };
            if left.kind() != "identifier" || right.kind() != "call" {
                return;
            }
            let callee = right
                .child_by_field_name("function")
                .map(|f| source.text(f))
                .unwrap_or_default();
            let kind = match callee.rsplit('.').next() {
                Some("FastAPI") => AppKind::App,
                Some("APIRouter") => AppKind::Router,
                _ => return,
            };
            let args = right.child_by_field_name("arguments");
            facts.objects.push(AppObject {
                name: source.text(left).to_owned(),
                kind,
                prefix: args.and_then(|a| keyword_string(source, a, "prefix")),
                loc: source.locate(statement),
            });
        }
        "call" => {
            let Some(function) = expr.child_by_field_name("function") else {
                return;
            };
            if function.kind() != "attribute" {
                return;
            }
            let attr = function
                .child_by_field_name("attribute")
                .map(|a| source.text(a))
                .unwrap_or_default();
            if attr != "include_router" {
                return;
            }
            let app = function
                .child_by_field_name("object")
                .map(|o| source.text(o).to_owned())
                .unwrap_or_default();
            let Some(args) = expr.child_by_field_name("arguments") else {
                return;
            };
            let Some(first) = named_children(args)
                .into_iter()
                .find(|a| a.kind() != "keyword_argument")
            else {
                return;
            };
            let router = match first.kind() {
                "identifier" => Reference::Name {
                    name: source.text(first).to_owned(),
                },
                "attribute" => Reference::Attribute {
                    module: first
                        .child_by_field_name("object")
                        .map(|o| source.text(o).to_owned())
                        .unwrap_or_default(),
                    name: first
                        .child_by_field_name("attribute")
                        .map(|a| source.text(a).to_owned())
                        .unwrap_or_default(),
                },
                _ => return,
            };
            facts.includes.push(Include {
                app,
                router,
                prefix: keyword_string(source, args, "prefix"),
                loc: source.locate(statement),
            });
        }
        _ => {}
    }
}

fn decorated(source: &Source<'_>, node: Node<'_>, facts: &mut PythonFacts) {
    let Some(definition) = node.child_by_field_name("definition") else {
        return;
    };
    match definition.kind() {
        "class_definition" => facts.classes.push(class(source, definition)),
        "function_definition" => {
            let decorators: Vec<Node<'_>> = named_children(node)
                .into_iter()
                .filter(|n| n.kind() == "decorator")
                .collect();
            for decorator in &decorators {
                facts.routes.extend(route(source, *decorator, definition));
            }
            if facts.is_test_file {
                test_function(source, definition, &mut facts.test_requests);
            }
        }
        _ => {}
    }
}

const ROUTE_METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

fn route(source: &Source<'_>, decorator: Node<'_>, def: Node<'_>) -> Vec<Route> {
    let Some(call) = named_children(decorator)
        .into_iter()
        .find(|n| n.kind() == "call")
    else {
        return vec![];
    };
    let Some(function) = call.child_by_field_name("function") else {
        return vec![];
    };
    if function.kind() != "attribute" {
        return vec![];
    }
    let object = function
        .child_by_field_name("object")
        .map(|o| source.text(o).to_owned())
        .unwrap_or_default();
    let verb = function
        .child_by_field_name("attribute")
        .map(|a| source.text(a))
        .unwrap_or_default();
    let Some(args) = call.child_by_field_name("arguments") else {
        return vec![];
    };
    let methods: Vec<HttpMethod> = if ROUTE_METHODS.contains(&verb) {
        HttpMethod::from_str(verb).into_iter().collect()
    } else if verb == "api_route" {
        keyword(args, source, "methods")
            .map(|list| {
                named_children(list)
                    .into_iter()
                    .filter_map(|m| syntax::string_value(source, m))
                    .filter_map(|m| HttpMethod::from_str(&m).ok())
                    .collect()
            })
            .unwrap_or_else(|| vec![HttpMethod::Get])
    } else {
        return vec![];
    };
    let path = named_children(args)
        .into_iter()
        .find(|a| a.kind() != "keyword_argument")
        .and_then(|p| syntax::string_value(source, p))
        .or_else(|| keyword_string(source, args, "path"));
    let response_model = keyword(args, source, "response_model").map(|t| type_expr(source, t));
    let status_code =
        keyword(args, source, "status_code").and_then(|n| source.text(n).parse().ok());
    let handler = def
        .child_by_field_name("name")
        .map(|n| source.text(n).to_owned())
        .unwrap_or_default();
    let params = def
        .child_by_field_name("parameters")
        .map(|p| params(source, p))
        .unwrap_or_default();
    let returns = def
        .child_by_field_name("return_type")
        .map(|t| type_expr(source, t));
    let header_end = def
        .child_by_field_name("return_type")
        .or_else(|| def.child_by_field_name("parameters"))
        .unwrap_or(def);
    let loc = source.locate_between(decorator, header_end);
    methods
        .into_iter()
        .map(|method| Route {
            object: object.clone(),
            method,
            path: path.clone(),
            response_model: response_model.clone(),
            status_code,
            handler: handler.clone(),
            params: params.clone(),
            returns: returns.clone(),
            loc: loc.clone(),
        })
        .collect()
}

fn params(source: &Source<'_>, parameters: Node<'_>) -> Vec<Param> {
    named_children(parameters)
        .into_iter()
        .filter_map(|p| {
            let (name, annotation, default) = match p.kind() {
                "identifier" => (source.text(p).to_owned(), None, None),
                "typed_parameter" => {
                    let name = named_children(p)
                        .into_iter()
                        .find(|n| n.kind() == "identifier")
                        .map(|n| source.text(n).to_owned())?;
                    (name, p.child_by_field_name("type"), None)
                }
                "typed_default_parameter" | "default_parameter" => {
                    let name = p
                        .child_by_field_name("name")
                        .map(|n| source.text(n).to_owned())?;
                    (
                        name,
                        p.child_by_field_name("type"),
                        p.child_by_field_name("value"),
                    )
                }
                _ => return None,
            };
            let marker = default
                .filter(|d| d.kind() == "call")
                .and_then(|d| d.child_by_field_name("function"))
                .map(|f| {
                    source
                        .text(f)
                        .rsplit('.')
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                });
            Some(Param {
                name,
                annotation: annotation.map(|a| type_expr(source, a)),
                marker,
            })
        })
        .collect()
}

fn class(source: &Source<'_>, def: Node<'_>) -> Class {
    let name = def
        .child_by_field_name("name")
        .map(|n| source.text(n).to_owned())
        .unwrap_or_default();
    let bases = def
        .child_by_field_name("superclasses")
        .map(|s| {
            named_children(s)
                .into_iter()
                .filter(|b| b.kind() != "keyword_argument")
                .map(|b| source.text(b).to_owned())
                .collect()
        })
        .unwrap_or_default();
    let mut fields = Vec::new();
    if let Some(body) = def.child_by_field_name("body") {
        for statement in named_children(body) {
            let Some(assignment) = named_children(statement)
                .into_iter()
                .find(|n| n.kind() == "assignment")
            else {
                continue;
            };
            let (Some(left), Some(annotation)) = (
                assignment.child_by_field_name("left"),
                assignment.child_by_field_name("type"),
            ) else {
                continue;
            };
            if left.kind() != "identifier" {
                continue;
            }
            let annotation = type_expr(source, annotation);
            if matches!(&annotation, TypeExpr::Generic { base, .. } if base.ends_with("ClassVar")) {
                continue;
            }
            let right = assignment.child_by_field_name("right");
            let field_call = right.filter(|r| {
                r.kind() == "call"
                    && r.child_by_field_name("function")
                        .is_some_and(|f| source.text(f).rsplit('.').next() == Some("Field"))
            });
            let args = field_call.and_then(|c| c.child_by_field_name("arguments"));
            let has_default = match (right, args) {
                (None, _) => false,
                (Some(_), None) => true,
                (Some(_), Some(args)) => {
                    let first = named_children(args)
                        .into_iter()
                        .find(|a| a.kind() != "keyword_argument");
                    keyword(args, source, "default").is_some()
                        || keyword(args, source, "default_factory").is_some()
                        || first.is_some_and(|f| f.kind() != "ellipsis")
                }
            };
            fields.push(ClassField {
                name: source.text(left).to_owned(),
                annotation,
                has_default,
                alias: args.and_then(|a| keyword_string(source, a, "alias")),
                serialization_alias: args
                    .and_then(|a| keyword_string(source, a, "serialization_alias")),
                validation_alias: args.and_then(|a| keyword_string(source, a, "validation_alias")),
                span: source.span(statement),
            });
        }
    }
    let header_end = def
        .child_by_field_name("superclasses")
        .or_else(|| def.child_by_field_name("name"))
        .unwrap_or(def);
    Class {
        name,
        bases,
        fields,
        loc: source.locate_between(def, header_end),
    }
}

fn test_function(source: &Source<'_>, def: Node<'_>, out: &mut Vec<TestRequest>) {
    let name = def
        .child_by_field_name("name")
        .map(|n| source.text(n))
        .unwrap_or_default();
    if !name.starts_with("test") {
        return;
    }
    let Some(body) = def.child_by_field_name("body") else {
        return;
    };
    visit(body, &mut |node| {
        if node.kind() != "call" {
            return;
        }
        let Some(function) = node
            .child_by_field_name("function")
            .filter(|f| f.kind() == "attribute")
        else {
            return;
        };
        let verb = function
            .child_by_field_name("attribute")
            .map(|a| source.text(a))
            .unwrap_or_default();
        if !ROUTE_METHODS.contains(&verb) {
            return;
        }
        let Some(method) = HttpMethod::from_str(verb).ok() else {
            return;
        };
        let Some(first) = node
            .child_by_field_name("arguments")
            .and_then(|a| named_children(a).into_iter().next())
        else {
            return;
        };
        let Some(path) = literal_path(source, first) else {
            return;
        };
        out.push(TestRequest {
            test: name.to_owned(),
            method,
            path,
            loc: source.locate(node),
        });
    });
}

/// A path from a string or f-string; interpolations become `{}` and a scheme
/// and host, if present, are dropped.
fn literal_path(source: &Source<'_>, node: Node<'_>) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let mut text = String::new();
    for part in named_children(node) {
        match part.kind() {
            "string_content" => text.push_str(source.text(part)),
            "interpolation" => text.push_str("{}"),
            _ => {}
        }
    }
    let path = match text.find("://") {
        Some(scheme) => text[scheme + 3..]
            .find('/')
            .map(|slash| text[scheme + 3 + slash..].to_owned())?,
        None => text,
    };
    path.starts_with('/').then_some(path)
}

fn keyword<'t>(args: Node<'t>, source: &Source<'_>, name: &str) -> Option<Node<'t>> {
    named_children(args)
        .into_iter()
        .filter(|a| a.kind() == "keyword_argument")
        .find(|a| {
            a.child_by_field_name("name")
                .is_some_and(|n| source.text(n) == name)
        })
        .and_then(|a| a.child_by_field_name("value"))
}

fn keyword_string(source: &Source<'_>, args: Node<'_>, name: &str) -> Option<String> {
    keyword(args, source, name).and_then(|v| syntax::string_value(source, v))
}

pub fn type_expr(source: &Source<'_>, node: Node<'_>) -> TypeExpr {
    match node.kind() {
        "type" => match named_children(node).into_iter().next() {
            Some(inner) => type_expr(source, inner),
            None => TypeExpr::Other(source.text(node).to_owned()),
        },
        "identifier" | "attribute" => TypeExpr::Name(source.text(node).to_owned()),
        "none" => TypeExpr::None,
        "string" => match syntax::string_value(source, node) {
            Some(forward) => TypeExpr::Name(forward),
            None => TypeExpr::Other(source.text(node).to_owned()),
        },
        "generic_type" => {
            let children = named_children(node);
            let base = children
                .first()
                .map(|b| source.text(*b).to_owned())
                .unwrap_or_default();
            let args = children
                .iter()
                .filter(|c| c.kind() == "type_parameter")
                .flat_map(|tp| named_children(*tp))
                .map(|t| type_expr(source, t))
                .collect();
            TypeExpr::Generic { base, args }
        }
        "subscript" => {
            let base = node
                .child_by_field_name("value")
                .map(|v| source.text(v).to_owned())
                .unwrap_or_default();
            let mut cursor = node.walk();
            let args = node
                .children_by_field_name("subscript", &mut cursor)
                .flat_map(|s| {
                    if s.kind() == "tuple" {
                        named_children(s)
                    } else {
                        vec![s]
                    }
                });
            TypeExpr::Generic {
                base,
                args: args.map(|a| type_expr(source, a)).collect(),
            }
        }
        "binary_operator" => {
            let mut variants = Vec::new();
            for side in [
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            ]
            .into_iter()
            .flatten()
            {
                match type_expr(source, side) {
                    TypeExpr::Union(inner) => variants.extend(inner),
                    other => variants.push(other),
                }
            }
            TypeExpr::Union(variants)
        }
        _ => TypeExpr::Other(source.text(node).to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(path: &str, text: &str) -> PythonFacts {
        extract(&Source::new(path, text)).expect("the Python grammar loads")
    }

    const ROUTER: &str = r#"from fastapi import APIRouter, Header

from app import db
from app.models import AccountOut, InvoiceOut as Invoice

router = APIRouter(prefix="/accounts", tags=["accounts"])


@router.get("/{account_id}", response_model=AccountOut)
def get_account(account_id: int, x_trace: str = Header(default="")) -> AccountOut:
    return db.fetch_account(account_id)


@router.post("/{account_id}/invoices", response_model=list[Invoice], status_code=201)
async def add_invoice(account_id: int, invoice: Invoice) -> list[Invoice]:
    return []
"#;

    #[test]
    fn reads_routers_routes_and_imports() {
        let f = facts("api/app/routers/accounts.py", ROUTER);
        assert_eq!(f.objects.len(), 1);
        assert_eq!(
            (
                f.objects[0].name.as_str(),
                f.objects[0].kind,
                f.objects[0].prefix.as_deref()
            ),
            ("router", AppKind::Router, Some("/accounts"))
        );
        assert_eq!(f.routes.len(), 2);
        let get = &f.routes[0];
        assert_eq!(
            (
                get.object.as_str(),
                get.method,
                get.path.as_deref(),
                get.handler.as_str()
            ),
            (
                "router",
                HttpMethod::Get,
                Some("/{account_id}"),
                "get_account"
            )
        );
        assert_eq!(
            get.response_model,
            Some(TypeExpr::Name("AccountOut".into()))
        );
        assert_eq!(get.loc.span.start.line, 9);
        assert!(get.loc.excerpt.contains("def get_account"));
        assert_eq!(get.params[1].marker.as_deref(), Some("Header"));
        let post = &f.routes[1];
        assert_eq!(post.status_code, Some(201));
        assert_eq!(
            post.response_model,
            Some(TypeExpr::Generic {
                base: "list".into(),
                args: vec![TypeExpr::Name("Invoice".into())]
            })
        );
        let alias = f
            .imports
            .iter()
            .find(|i| i.local == "Invoice")
            .expect("aliased import");
        assert_eq!(
            (alias.module.as_str(), alias.name.as_deref()),
            ("app.models", Some("InvoiceOut"))
        );
    }

    #[test]
    fn reads_pydantic_style_fields_with_aliases_and_defaults() {
        let f = facts(
            "api/app/models.py",
            "class Receipt(BaseModel):\n    payment_id: str = Field(serialization_alias=\"paymentId\")\n    status: str\n    ledger_entry_id: Optional[int] = None\n    total: int | None = Field(...)\n    kind: ClassVar[str] = \"x\"\n",
        );
        let receipt = &f.classes[0];
        assert_eq!(receipt.bases, vec!["BaseModel".to_owned()]);
        let names: Vec<_> = receipt.fields.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["payment_id", "status", "ledger_entry_id", "total"]
        );
        assert_eq!(
            receipt.fields[0].serialization_alias.as_deref(),
            Some("paymentId")
        );
        assert!(
            !receipt.fields[0].has_default,
            "Field(serialization_alias=...) has no default"
        );
        assert!(!receipt.fields[1].has_default);
        assert!(receipt.fields[2].has_default);
        assert!(!receipt.fields[3].has_default, "Field(...) is required");
        assert_eq!(
            receipt.fields[3].annotation,
            TypeExpr::Union(vec![TypeExpr::Name("int".into()), TypeExpr::None])
        );
    }

    #[test]
    fn reads_apps_and_included_routers() {
        let f = facts(
            "api/app/main.py",
            "from fastapi import FastAPI\nfrom app.routers import accounts, payments\n\napp = FastAPI(title=\"x\")\napp.include_router(accounts.router)\napp.include_router(payments.router, prefix=\"/v2\")\n",
        );
        assert_eq!(f.objects[0].kind, AppKind::App);
        assert_eq!(f.includes.len(), 2);
        assert_eq!(
            f.includes[1].router,
            Reference::Attribute {
                module: "payments".into(),
                name: "router".into()
            }
        );
        assert_eq!(f.includes[1].prefix.as_deref(), Some("/v2"));
    }

    #[test]
    fn test_requests_are_read_only_from_test_functions_in_test_files() {
        let text = "def test_reads_account(client):\n    client.get(f\"/accounts/{7}\")\n    client.post(\"http://testserver/payments\", json={})\n\ndef helper(client):\n    client.get(\"/ignored\")\n";
        let f = facts("api/tests/test_accounts.py", text);
        let paths: Vec<_> = f
            .test_requests
            .iter()
            .map(|r| (r.method, r.path.as_str()))
            .collect();
        assert_eq!(
            paths,
            vec![
                (HttpMethod::Get, "/accounts/{}"),
                (HttpMethod::Post, "/payments")
            ]
        );
        let not_test = facts("api/app/client.py", text);
        assert!(not_test.test_requests.is_empty());
    }

    #[test]
    fn route_like_strings_outside_decorators_are_not_routes() {
        let f = facts(
            "api/app/notes.py",
            "# @router.get(\"/accounts\")\nPATH = \"/accounts/{id}\"\nlog.info(\"GET /accounts\")\n",
        );
        assert!(f.routes.is_empty());
        assert!(f.objects.is_empty());
    }

    #[test]
    fn malformed_python_does_not_panic() {
        let f = facts(
            "api/app/broken.py",
            "@router.get(\"/x\"\ndef broken(:\n    return\nclass\n",
        );
        assert!(f.classes.len() <= 1);
    }
}
