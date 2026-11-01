//! Source locations and excerpts.

use std::fmt;

use serde::{Deserialize, Serialize};

/// A 1-based line and a 1-based column counted in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub column: u32,
}

/// A region of one file, addressed by a repository-relative path with `/`
/// separators.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Span {
    pub path: String,
    pub start: Position,
    pub end: Position,
    pub start_byte: usize,
    pub end_byte: usize,
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.path, self.start.line, self.start.column)
    }
}

/// The longest excerpt an evidence record keeps.
pub const MAX_EXCERPT_LINES: usize = 12;

/// The full source lines a span covers, at most [`MAX_EXCERPT_LINES`] of
/// them. Longer spans keep their first lines and end with a line saying how
/// many were left out.
pub fn excerpt(source: &str, span: &Span) -> String {
    let first = span.start.line.max(1) as usize;
    let last = (span.end.line as usize).max(first);
    let wanted = last - first + 1;
    let mut lines: Vec<&str> = source
        .lines()
        .skip(first - 1)
        .take(wanted.min(MAX_EXCERPT_LINES))
        .collect();
    while lines.last().is_some_and(|line| line.trim().is_empty()) && lines.len() > 1 {
        lines.pop();
    }
    let mut text = lines.join("\n");
    if wanted > MAX_EXCERPT_LINES {
        text.push_str(&format!("\n… {} more lines", wanted - MAX_EXCERPT_LINES));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(start: u32, end: u32) -> Span {
        Span {
            path: "a.ts".into(),
            start: Position {
                line: start,
                column: 1,
            },
            end: Position {
                line: end,
                column: 1,
            },
            start_byte: 0,
            end_byte: 0,
        }
    }

    #[test]
    fn excerpt_returns_whole_lines_of_the_span() {
        let source = "one\ntwo\nthree\nfour";
        assert_eq!(excerpt(source, &span(2, 3)), "two\nthree");
    }

    #[test]
    fn long_excerpts_are_cut_and_say_so() {
        let source: String = (1..=30).map(|n| format!("line {n}\n")).collect();
        let text = excerpt(&source, &span(1, 30));
        assert!(text.starts_with("line 1\nline 2"));
        assert!(text.ends_with("… 18 more lines"));
        assert_eq!(text.lines().count(), MAX_EXCERPT_LINES + 1);
    }

    #[test]
    fn spans_past_the_end_do_not_panic() {
        assert_eq!(excerpt("only", &span(5, 9)), "");
    }
}
