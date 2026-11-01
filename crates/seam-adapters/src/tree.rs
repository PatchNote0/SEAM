//! A YAML and JSON reader that keeps the source span of every key and value.
//!
//! Contract and configuration files are read through this tree rather than
//! through serde so that every fact taken from them can cite its line.
//! Anchors are read; aliases are reported and left unresolved.

use seam_core::Span;
use tree_sitter::Node as SyntaxNode;

use crate::syntax::{self, Source, named_children};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Map(Vec<Entry>),
    Seq(Vec<Node>),
    Scalar(String),
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub value: Value,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub key: String,
    pub key_span: Span,
    pub value: Node,
}

/// A parsed document and anything that could not be read.
#[derive(Clone, Debug)]
pub struct Document {
    pub root: Node,
    pub problems: Vec<String>,
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        self.entry(key).map(|e| &e.value)
    }

    pub fn entry(&self, key: &str) -> Option<&Entry> {
        match &self.value {
            Value::Map(entries) => entries.iter().find(|e| e.key == key),
            _ => None,
        }
    }

    pub fn entries(&self) -> &[Entry] {
        match &self.value {
            Value::Map(entries) => entries,
            _ => &[],
        }
    }

    pub fn items(&self) -> &[Node] {
        match &self.value {
            Value::Seq(items) => items,
            _ => &[],
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            Value::Scalar(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self.as_str()? {
            "true" | "True" | "TRUE" => Some(true),
            "false" | "False" | "FALSE" => Some(false),
            _ => None,
        }
    }
}

pub fn read_yaml(source: &Source<'_>) -> Result<Document, syntax::ParseError> {
    let tree = syntax::parse(&tree_sitter_yaml::LANGUAGE.into(), source.text)?;
    let mut reader = Reader {
        source,
        problems: Vec::new(),
    };
    let root = tree.root_node();
    let document = named_children(root)
        .into_iter()
        .find(|n| n.kind() == "document");
    let node = match document {
        Some(doc) => match content(doc) {
            Some(inner) => reader.yaml(inner),
            None => reader.null(doc),
        },
        None => reader.null(root),
    };
    if root.has_error() {
        reader
            .problems
            .push("the file has YAML syntax errors; only the readable parts were used".into());
    }
    Ok(Document {
        root: node,
        problems: reader.problems,
    })
}

pub fn read_json(source: &Source<'_>) -> Result<Document, syntax::ParseError> {
    let tree = syntax::parse(&tree_sitter_json::LANGUAGE.into(), source.text)?;
    let mut reader = Reader {
        source,
        problems: Vec::new(),
    };
    let root = tree.root_node();
    let node = match named_children(root)
        .into_iter()
        .find(|n| n.kind() != "comment")
    {
        Some(value) => reader.json(value),
        None => reader.null(root),
    };
    if root.has_error() {
        reader
            .problems
            .push("the file has JSON syntax errors; only the readable parts were used".into());
    }
    Ok(Document {
        root: node,
        problems: reader.problems,
    })
}

struct Reader<'s, 'a> {
    source: &'s Source<'a>,
    problems: Vec<String>,
}

/// The value inside a `document`, `block_node` or `flow_node`, skipping
/// anchors, tags and comments.
fn content(node: SyntaxNode<'_>) -> Option<SyntaxNode<'_>> {
    named_children(node)
        .into_iter()
        .rfind(|n| !matches!(n.kind(), "anchor" | "tag" | "comment"))
}

impl Reader<'_, '_> {
    fn null(&self, node: SyntaxNode<'_>) -> Node {
        Node {
            value: Value::Null,
            span: self.source.span(node),
        }
    }

    fn yaml(&mut self, node: SyntaxNode<'_>) -> Node {
        let span = self.source.span(node);
        let value = match node.kind() {
            "block_node" | "flow_node" => match content(node) {
                Some(inner) => return self.yaml(inner),
                None => Value::Null,
            },
            "block_mapping" => {
                let pairs = named_children(node)
                    .into_iter()
                    .filter(|n| n.kind() == "block_mapping_pair");
                Value::Map(pairs.filter_map(|pair| self.yaml_pair(pair)).collect())
            }
            "flow_mapping" => {
                let mut entries = Vec::new();
                for child in named_children(node) {
                    match child.kind() {
                        "flow_pair" => entries.extend(self.yaml_pair(child)),
                        "flow_node" => {
                            if let Some(key) = self.key_text(child) {
                                let key_span = self.source.span(child);
                                entries.push(Entry {
                                    key,
                                    key_span: key_span.clone(),
                                    value: Node {
                                        value: Value::Null,
                                        span: key_span,
                                    },
                                });
                            }
                        }
                        _ => {}
                    }
                }
                Value::Map(entries)
            }
            "block_sequence" => Value::Seq(
                named_children(node)
                    .into_iter()
                    .filter(|n| n.kind() == "block_sequence_item")
                    .map(|item| {
                        match named_children(item)
                            .into_iter()
                            .find(|n| n.kind() != "comment")
                        {
                            Some(inner) => self.yaml(inner),
                            None => self.null(item),
                        }
                    })
                    .collect(),
            ),
            "flow_sequence" => Value::Seq(
                named_children(node)
                    .into_iter()
                    .filter(|n| matches!(n.kind(), "flow_node" | "flow_pair"))
                    .map(|item| self.yaml(item))
                    .collect(),
            ),
            "plain_scalar" => match named_children(node).first() {
                Some(inner) if inner.kind() == "null_scalar" => Value::Null,
                _ => Value::Scalar(self.source.text(node).trim().to_owned()),
            },
            "double_quote_scalar" => Value::Scalar(unquote_double(self.source.text(node))),
            "single_quote_scalar" => Value::Scalar(unquote_single(self.source.text(node))),
            "block_scalar" => Value::Scalar(block_scalar(self.source.text(node))),
            "alias" => {
                self.problems.push(format!(
                    "YAML alias `{}` at line {} is not followed",
                    self.source.text(node),
                    span.start.line
                ));
                Value::Null
            }
            "ERROR" => Value::Null,
            other => {
                self.problems.push(format!(
                    "unexpected YAML node `{other}` at line {}",
                    span.start.line
                ));
                Value::Null
            }
        };
        Node { value, span }
    }

    fn yaml_pair(&mut self, pair: SyntaxNode<'_>) -> Option<Entry> {
        let key_node = pair.child_by_field_name("key")?;
        let key = self.key_text(key_node)?;
        let value = match pair.child_by_field_name("value") {
            Some(v) => self.yaml(v),
            None => self.null(pair),
        };
        Some(Entry {
            key,
            key_span: self.source.span(key_node),
            value,
        })
    }

    fn key_text(&mut self, key: SyntaxNode<'_>) -> Option<String> {
        let inner = if matches!(key.kind(), "flow_node" | "block_node") {
            content(key)?
        } else {
            key
        };
        match inner.kind() {
            "plain_scalar" => Some(self.source.text(inner).trim().to_owned()),
            "double_quote_scalar" => Some(unquote_double(self.source.text(inner))),
            "single_quote_scalar" => Some(unquote_single(self.source.text(inner))),
            _ => {
                self.problems.push(format!(
                    "complex YAML key at line {} is not supported",
                    inner.start_position().row + 1
                ));
                None
            }
        }
    }

    fn json(&mut self, node: SyntaxNode<'_>) -> Node {
        let span = self.source.span(node);
        let value = match node.kind() {
            "object" => Value::Map(
                named_children(node)
                    .into_iter()
                    .filter(|n| n.kind() == "pair")
                    .filter_map(|pair| {
                        let key_node = pair.child_by_field_name("key")?;
                        let value = match pair.child_by_field_name("value") {
                            Some(v) => self.json(v),
                            None => self.null(pair),
                        };
                        Some(Entry {
                            key: unquote_double(self.source.text(key_node)),
                            key_span: self.source.span(key_node),
                            value,
                        })
                    })
                    .collect(),
            ),
            "array" => Value::Seq(
                named_children(node)
                    .into_iter()
                    .filter(|n| n.kind() != "comment")
                    .map(|item| self.json(item))
                    .collect(),
            ),
            "string" => Value::Scalar(unquote_double(self.source.text(node))),
            "number" | "true" | "false" => Value::Scalar(self.source.text(node).to_owned()),
            _ => Value::Null,
        };
        Node { value, span }
    }
}

fn unquote_double(raw: &str) -> String {
    let inner = raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(raw);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(c);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

fn unquote_single(raw: &str) -> String {
    let inner = raw
        .strip_prefix('\'')
        .and_then(|s| s.strip_suffix('\''))
        .unwrap_or(raw);
    inner.replace("''", "'")
}

/// The text of a `|` or `>` block scalar, with its common indentation
/// removed. Folding of `>` scalars is approximated by joining lines with
/// spaces, which is enough for descriptions.
fn block_scalar(raw: &str) -> String {
    let mut lines = raw.lines();
    let header = lines.next().unwrap_or_default();
    let body: Vec<&str> = lines.collect();
    let indent = body
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    let dedented: Vec<&str> = body.iter().map(|l| l.get(indent..).unwrap_or("")).collect();
    let separator = if header.trim_start().starts_with('>') {
        " "
    } else {
        "\n"
    };
    dedented.join(separator).trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(text: &str) -> Document {
        read_yaml(&Source::new("t.yaml", text)).expect("the YAML grammar loads")
    }

    #[test]
    fn nested_block_mappings_keep_spans() {
        let doc = yaml("paths:\n  /accounts/{id}:\n    get:\n      operationId: getAccount\n");
        let op = doc
            .root
            .get("paths")
            .and_then(|p| p.get("/accounts/{id}"))
            .and_then(|p| p.get("get"));
        let id = op.and_then(|o| o.get("operationId"));
        assert_eq!(id.and_then(Node::as_str), Some("getAccount"));
        assert_eq!(id.map(|n| n.span.start.line), Some(4));
        let entry = doc
            .root
            .get("paths")
            .and_then(|p| p.entry("/accounts/{id}"));
        assert_eq!(
            entry.map(|e| (e.key_span.start.line, e.key_span.start.column)),
            Some((2, 3))
        );
    }

    #[test]
    fn quoted_keys_flow_collections_and_sequences() {
        let doc = yaml(
            "responses:\n  \"200\": ok\ntags: [a, 'b']\ntype: [integer, \"null\"]\nservers:\n  - url: http://api:8000\n",
        );
        assert_eq!(
            doc.root
                .get("responses")
                .and_then(|r| r.get("200"))
                .and_then(Node::as_str),
            Some("ok")
        );
        let tags: Vec<_> = doc
            .root
            .get("tags")
            .map(|t| t.items().iter().filter_map(Node::as_str).collect())
            .unwrap_or_default();
        assert_eq!(tags, vec!["a", "b"]);
        let types: Vec<_> = doc
            .root
            .get("type")
            .map(|t| t.items().iter().filter_map(Node::as_str).collect())
            .unwrap_or_default();
        assert_eq!(types, vec!["integer", "null"]);
        let url = doc
            .root
            .get("servers")
            .and_then(|s| s.items().first())
            .and_then(|s| s.get("url"));
        assert_eq!(url.and_then(Node::as_str), Some("http://api:8000"));
    }

    #[test]
    fn nulls_anchors_and_block_scalars() {
        let doc = yaml("a: ~\nb:\nc: &x hello\nd: *x\ne: |\n  line one\n  line two\n");
        assert_eq!(doc.root.get("a").map(|n| &n.value), Some(&Value::Null));
        assert_eq!(doc.root.get("c").and_then(Node::as_str), Some("hello"));
        assert_eq!(doc.root.get("d").map(|n| &n.value), Some(&Value::Null));
        assert!(doc.problems.iter().any(|p| p.contains("alias")));
        assert_eq!(
            doc.root.get("e").and_then(Node::as_str),
            Some("line one\nline two")
        );
    }

    #[test]
    fn malformed_yaml_keeps_what_it_can() {
        let doc = yaml("ok: 1\nbroken: [unclosed\nlater: 2\n");
        assert_eq!(doc.root.get("ok").and_then(Node::as_str), Some("1"));
        assert!(!doc.problems.is_empty());
    }

    #[test]
    fn json_objects_and_escapes() {
        let source = Source::new(
            "t.json",
            "{\"openapi\": \"3.0.0\", \"x\": [1, true, null], \"s\": \"a\\\"b\\u00e9\"}",
        );
        let doc = read_json(&source).expect("the JSON grammar loads");
        assert_eq!(
            doc.root.get("openapi").and_then(Node::as_str),
            Some("3.0.0")
        );
        assert_eq!(doc.root.get("x").map(|x| x.items().len()), Some(3));
        assert_eq!(doc.root.get("s").and_then(Node::as_str), Some("a\"bé"));
    }

    #[test]
    fn empty_input_is_null() {
        assert_eq!(yaml("").root.value, Value::Null);
    }
}
