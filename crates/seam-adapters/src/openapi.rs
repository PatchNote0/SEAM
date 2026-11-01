//! OpenAPI 3.x documents: operations, request and response schemas, with
//! local `$ref`s resolved and every property's line kept.

use std::str::FromStr;

use seam_core::{Field, HttpMethod, ObjectShape, Shape};
use serde::{Deserialize, Serialize};

use crate::syntax::{Located, Source};
use crate::tree::{Node, Value};

pub const ADAPTER: &str = "openapi@0.1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenApiDoc {
    pub title: Option<String>,
    pub servers: Vec<String>,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub method: HttpMethod,
    pub path: String,
    pub operation_id: Option<String>,
    pub loc: Located,
    pub request: Option<Shape>,
    pub response: Option<OperationResponse>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationResponse {
    pub status: String,
    pub shape: Option<Shape>,
    pub loc: Located,
}

/// True when a parsed YAML or JSON document is an OpenAPI or Swagger file.
pub fn is_openapi(root: &Node) -> bool {
    root.get("openapi").is_some() || root.get("swagger").is_some()
}

pub fn read(source: &Source<'_>, root: &Node) -> OpenApiDoc {
    let lowering = Lowering { root, source };
    let title = root
        .get("info")
        .and_then(|i| i.get("title"))
        .and_then(Node::as_str)
        .map(str::to_owned);
    let servers = root
        .get("servers")
        .map(|s| {
            s.items()
                .iter()
                .filter_map(|s| s.get("url").and_then(Node::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut operations = Vec::new();
    if let Some(paths) = root.get("paths") {
        for path_entry in paths.entries() {
            for op_entry in path_entry.value.entries() {
                let Ok(method) = HttpMethod::from_str(&op_entry.key) else {
                    continue;
                };
                let op = &op_entry.value;
                let span = seam_core::Span {
                    path: source.path.to_owned(),
                    start: op_entry.key_span.start,
                    end: op.span.end,
                    start_byte: op_entry.key_span.start_byte,
                    end_byte: op.span.end_byte,
                };
                let excerpt = seam_core::span::excerpt(source.text, &span);
                operations.push(Operation {
                    method,
                    path: path_entry.key.clone(),
                    operation_id: op
                        .get("operationId")
                        .and_then(Node::as_str)
                        .map(str::to_owned),
                    loc: Located { span, excerpt },
                    request: lowering.request(op),
                    response: lowering.response(op),
                });
            }
        }
    }
    OpenApiDoc {
        title,
        servers,
        operations,
    }
}

struct Lowering<'a> {
    root: &'a Node,
    source: &'a Source<'a>,
}

const MAX_DEPTH: usize = 8;

impl Lowering<'_> {
    fn request(&self, op: &Node) -> Option<Shape> {
        let body = self.deref(op.get("requestBody")?, 0)?;
        let schema = json_content(body.get("content")?)?.get("schema")?;
        Some(self.lower(schema, 0, &mut Vec::new()).0)
    }

    fn response(&self, op: &Node) -> Option<OperationResponse> {
        let mut statuses: Vec<_> = op
            .get("responses")?
            .entries()
            .iter()
            .filter(|e| e.key.starts_with('2'))
            .collect();
        statuses.sort_by(|a, b| a.key.cmp(&b.key));
        let entry = statuses.first()?;
        let response = self.deref(&entry.value, 0)?;
        let shape = response
            .get("content")
            .and_then(json_content)
            .and_then(|c| c.get("schema"))
            .map(|schema| self.lower(schema, 0, &mut Vec::new()).0);
        let span = seam_core::Span {
            path: self.source.path.to_owned(),
            start: entry.key_span.start,
            end: entry.value.span.end,
            start_byte: entry.key_span.start_byte,
            end_byte: entry.value.span.end_byte,
        };
        let excerpt = seam_core::span::excerpt(self.source.text, &span);
        Some(OperationResponse {
            status: entry.key.clone(),
            shape,
            loc: Located { span, excerpt },
        })
    }

    /// Follows a local `$ref` such as `#/components/schemas/Account`.
    fn deref<'n>(&'n self, node: &'n Node, depth: usize) -> Option<&'n Node> {
        match node.get("$ref").and_then(Node::as_str) {
            Some(pointer) if depth < MAX_DEPTH => {
                let target = self.pointer(pointer)?;
                self.deref(target, depth + 1)
            }
            Some(_) => None,
            None => Some(node),
        }
    }

    fn pointer(&self, pointer: &str) -> Option<&Node> {
        let path = pointer.strip_prefix("#/")?;
        let mut node = self.root;
        for part in path.split('/') {
            let key = part.replace("~1", "/").replace("~0", "~");
            node = node.get(&key)?;
        }
        Some(node)
    }

    /// Lowers a schema to a shape and says whether it admits `null`.
    fn lower(&self, schema: &Node, depth: usize, seen: &mut Vec<String>) -> (Shape, bool) {
        if depth > MAX_DEPTH {
            return (Shape::unknown("schema nesting is too deep"), false);
        }
        if let Some(pointer) = schema.get("$ref").and_then(Node::as_str) {
            let name = pointer.rsplit('/').next().unwrap_or(pointer).to_owned();
            if seen.contains(&name) {
                return (Shape::Named { name }, false);
            }
            let Some(target) = self.pointer(pointer) else {
                return (
                    Shape::unknown(format!(
                        "`{pointer}` is not a local reference that resolves"
                    )),
                    false,
                );
            };
            seen.push(name.clone());
            let (mut shape, nullable) = self.lower(target, depth + 1, seen);
            seen.pop();
            if let Shape::Object(object) = &mut shape {
                object.name.get_or_insert(name);
            }
            return (shape, nullable);
        }
        let mut nullable = schema
            .get("nullable")
            .and_then(Node::as_bool)
            .unwrap_or(false);
        if let Some(parts) = schema.get("allOf") {
            let mut merged = ObjectShape::default();
            for part in parts.items() {
                let (shape, part_nullable) = self.lower(part, depth + 1, seen);
                nullable |= part_nullable;
                if let Shape::Object(object) = shape {
                    merged.name = merged.name.or(object.name);
                    merged.fields.extend(object.fields);
                }
            }
            return (Shape::Object(merged), nullable);
        }
        for key in ["oneOf", "anyOf"] {
            if let Some(options) = schema.get(key) {
                let mut variants = Vec::new();
                for option in options.items() {
                    let (shape, option_nullable) = self.lower(option, depth + 1, seen);
                    if shape == Shape::Null {
                        nullable = true;
                    } else {
                        variants.push(shape);
                    }
                    nullable |= option_nullable;
                }
                let shape = if variants.len() == 1 {
                    variants.remove(0)
                } else {
                    Shape::Union { variants }
                };
                return (shape, nullable);
            }
        }
        let mut types: Vec<&str> = match schema.get("type").map(|t| &t.value) {
            Some(Value::Scalar(t)) => vec![t.as_str()],
            Some(Value::Seq(items)) => items.iter().filter_map(Node::as_str).collect(),
            _ => Vec::new(),
        };
        if types.contains(&"null") {
            nullable = true;
            types.retain(|t| *t != "null");
        }
        if types.is_empty() && schema.get("properties").is_some() {
            types.push("object");
        }
        let mut shapes: Vec<Shape> = types
            .iter()
            .map(|t| self.lower_type(t, schema, depth, seen))
            .collect();
        let shape = match shapes.len() {
            0 if nullable => Shape::Null,
            0 => Shape::Any,
            1 => shapes.remove(0),
            _ => Shape::Union { variants: shapes },
        };
        (shape, nullable)
    }

    fn lower_type(&self, kind: &str, schema: &Node, depth: usize, seen: &mut Vec<String>) -> Shape {
        match kind {
            "string" => Shape::String,
            "integer" => Shape::Integer,
            "number" => Shape::Number,
            "boolean" => Shape::Boolean,
            "array" => match schema.get("items") {
                Some(items) => {
                    let (shape, nullable) = self.lower(items, depth + 1, seen);
                    let items = if nullable {
                        Shape::Union {
                            variants: vec![shape, Shape::Null],
                        }
                    } else {
                        shape
                    };
                    Shape::array(items)
                }
                None => Shape::array(Shape::Any),
            },
            "object" => {
                let required: Vec<&str> = schema
                    .get("required")
                    .map(|r| r.items().iter().filter_map(Node::as_str).collect())
                    .unwrap_or_default();
                let fields = schema
                    .get("properties")
                    .map(|props| {
                        props
                            .entries()
                            .iter()
                            .map(|entry| {
                                let (shape, nullable) = self.lower(&entry.value, depth + 1, seen);
                                Field {
                                    json_name: entry.key.clone(),
                                    declared_name: entry.key.clone(),
                                    shape,
                                    required: required.contains(&entry.key.as_str()),
                                    nullable,
                                    span: Some(entry.key_span.clone()),
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Shape::Object(ObjectShape { name: None, fields })
            }
            other => Shape::unknown(format!("schema type `{other}` is not recognised")),
        }
    }
}

fn json_content(content: &Node) -> Option<&Node> {
    content.get("application/json").or_else(|| {
        content
            .entries()
            .iter()
            .find(|e| e.key.contains("json"))
            .map(|e| &e.value)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::read_yaml;

    const DOC: &str = r##"openapi: 3.1.0
info:
  title: Ledgerline API
paths:
  /accounts/{account_id}:
    get:
      operationId: getAccount
      responses:
        "404":
          description: missing
        "200":
          description: ok
          content:
            application/json:
              schema:
                $ref: "#/components/schemas/Account"
  /payments:
    post:
      requestBody:
        content:
          application/json:
            schema:
              $ref: "#/components/schemas/Payment"
      responses:
        "201":
          description: created
components:
  schemas:
    Account:
      type: object
      required: [id, display_name]
      properties:
        id:
          type: integer
        display_name:
          type: string
        manager:
          $ref: "#/components/schemas/Account"
        ledger_entry_id:
          type: [integer, "null"]
        tags:
          type: array
          items:
            type: string
    Payment:
      type: object
      properties:
        amount:
          type: number
          nullable: true
"##;

    fn doc() -> OpenApiDoc {
        let source = Source::new("api/openapi.yaml", DOC);
        let tree = read_yaml(&source).expect("YAML loads");
        assert!(is_openapi(&tree.root));
        read(&source, &tree.root)
    }

    #[test]
    fn reads_operations_with_their_lines() {
        let doc = doc();
        assert_eq!(doc.title.as_deref(), Some("Ledgerline API"));
        let get = &doc.operations[0];
        assert_eq!(
            (get.method, get.path.as_str(), get.operation_id.as_deref()),
            (
                HttpMethod::Get,
                "/accounts/{account_id}",
                Some("getAccount")
            )
        );
        assert_eq!(get.loc.span.start.line, 6);
        assert!(get.loc.excerpt.starts_with("    get:"));
        assert_eq!(doc.operations[1].method, HttpMethod::Post);
    }

    #[test]
    fn picks_the_first_success_response_and_resolves_references() {
        let doc = doc();
        let response = doc.operations[0].response.as_ref().expect("a 2xx response");
        assert_eq!(response.status, "200");
        let Some(Shape::Object(account)) = &response.shape else {
            panic!("an object shape")
        };
        assert_eq!(account.name.as_deref(), Some("Account"));
        let id = account.field("id").expect("id");
        assert!(id.required && !id.nullable && id.shape == Shape::Integer);
        assert_eq!(id.span.as_ref().map(|s| s.start.line), Some(33));
        let ledger = account.field("ledger_entry_id").expect("ledger_entry_id");
        assert!(ledger.nullable && !ledger.required && ledger.shape == Shape::Integer);
        assert_eq!(
            account.field("tags").map(|f| f.shape.clone()),
            Some(Shape::array(Shape::String))
        );
    }

    #[test]
    fn recursive_references_stop_at_a_name() {
        let doc = doc();
        let Some(Shape::Object(account)) = doc.operations[0]
            .response
            .as_ref()
            .and_then(|r| r.shape.clone())
        else {
            panic!()
        };
        assert_eq!(
            account.field("manager").map(|f| f.shape.clone()),
            Some(Shape::Named {
                name: "Account".into()
            })
        );
    }

    #[test]
    fn request_bodies_and_nullable_flags() {
        let doc = doc();
        let Some(Shape::Object(payment)) = &doc.operations[1].request else {
            panic!("an object")
        };
        let amount = payment.field("amount").expect("amount");
        assert!(amount.nullable);
        assert_eq!(
            doc.operations[1]
                .response
                .as_ref()
                .and_then(|r| r.shape.clone()),
            None
        );
    }

    #[test]
    fn broken_references_become_unknown_shapes() {
        let text = "openapi: 3.0.0\npaths:\n  /x:\n    get:\n      responses:\n        '200':\n          content:\n            application/json:\n              schema:\n                $ref: '#/components/schemas/Missing'\n";
        let source = Source::new("o.yaml", text);
        let tree = read_yaml(&source).expect("YAML loads");
        let doc = read(&source, &tree.root);
        assert!(matches!(
            doc.operations[0]
                .response
                .as_ref()
                .and_then(|r| r.shape.clone()),
            Some(Shape::Unknown { .. })
        ));
    }
}
