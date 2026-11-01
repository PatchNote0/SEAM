//! Helpers shared by the tree-sitter based adapters.

use seam_core::{Position, Span};
use serde::{Deserialize, Serialize};
use tree_sitter::{Language, Node, Parser, Tree};

/// A fact's location: the span and the source lines it covers, captured
/// while the file is in hand so evidence never needs to re-read it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Located {
    pub span: Span,
    pub excerpt: String,
}

/// One file being read by an adapter.
#[derive(Clone, Copy)]
pub struct Source<'a> {
    pub path: &'a str,
    pub text: &'a str,
}

impl<'a> Source<'a> {
    pub fn new(path: &'a str, text: &'a str) -> Source<'a> {
        Source { path, text }
    }

    pub fn span(&self, node: Node<'_>) -> Span {
        self.span_between(node, node)
    }

    /// A span from the start of `first` to the end of `last`.
    pub fn span_between(&self, first: Node<'_>, last: Node<'_>) -> Span {
        let start = first.start_position();
        let end = last.end_position();
        Span {
            path: self.path.to_owned(),
            start: Position {
                line: start.row as u32 + 1,
                column: start.column as u32 + 1,
            },
            end: Position {
                line: end.row as u32 + 1,
                column: end.column as u32 + 1,
            },
            start_byte: first.start_byte(),
            end_byte: last.end_byte(),
        }
    }

    pub fn locate(&self, node: Node<'_>) -> Located {
        self.locate_between(node, node)
    }

    pub fn locate_between(&self, first: Node<'_>, last: Node<'_>) -> Located {
        let span = self.span_between(first, last);
        let excerpt = seam_core::span::excerpt(self.text, &span);
        Located { span, excerpt }
    }

    pub fn text(&self, node: Node<'_>) -> &'a str {
        self.text
            .get(node.start_byte()..node.end_byte())
            .unwrap_or_default()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("the grammar could not be loaded: {0}")]
    Grammar(String),
    #[error("the parser returned no tree")]
    NoTree,
}

pub fn parse(language: &Language, text: &str) -> Result<Tree, ParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(language)
        .map_err(|e| ParseError::Grammar(e.to_string()))?;
    parser.parse(text, None).ok_or(ParseError::NoTree)
}

/// Named children of a node, in order.
pub fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Visits `node` and every named descendant, depth first, in source order.
pub fn visit<'t>(node: Node<'t>, f: &mut impl FnMut(Node<'t>)) {
    f(node);
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        visit(child, f);
    }
}

/// The text of a string literal without its quotes, or `None` when the node
/// is not a plain string (template literals with substitutions, f-strings).
pub fn string_value(source: &Source<'_>, node: Node<'_>) -> Option<String> {
    let raw = source.text(node);
    let quote = raw.chars().next()?;
    if !matches!(quote, '"' | '\'' | '`') || raw.len() < 2 || !raw.ends_with(quote) {
        return None;
    }
    let inner = &raw[1..raw.len() - 1];
    if quote == '`' && inner.contains("${") {
        return None;
    }
    Some(unescape(inner))
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
