//! HTTP methods and route templates, and the rule that decides whether a
//! consumer's request can reach a producer's route.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl HttpMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }
}

impl fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("`{0}` is not an HTTP method")]
pub struct UnknownMethod(pub String);

impl FromStr for HttpMethod {
    type Err = UnknownMethod;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let method = match s.to_ascii_uppercase().as_str() {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "PATCH" => HttpMethod::Patch,
            "DELETE" => HttpMethod::Delete,
            "HEAD" => HttpMethod::Head,
            "OPTIONS" => HttpMethod::Options,
            _ => return Err(UnknownMethod(s.to_owned())),
        };
        Ok(method)
    }
}

/// One path segment. A parameter's name is kept when the source names it
/// (`{account_id}`, `${accountId}`) so evidence can show it, but names never
/// take part in matching.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Segment {
    Literal(String),
    Param(Option<String>),
    /// A parameter that swallows the rest of the path, such as FastAPI's
    /// `{path:path}` or Axum's `{*rest}`.
    Rest(Option<String>),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RouteTemplate {
    pub segments: Vec<Segment>,
}

/// How a consumer's route met a producer's route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteMatch {
    /// Every segment lines up kind for kind.
    Exact,
    /// The consumer sends a literal where the producer takes a parameter,
    /// as in `/accounts/me` against `/accounts/{id}`. The number is how many
    /// literal segments matched literally, used to prefer the most specific
    /// route.
    Literal {
        specific: usize,
    },
    NoMatch,
}

impl RouteTemplate {
    /// Parses a server-side route declaration: FastAPI, OpenAPI and Axum 0.8
    /// write parameters as `{name}`, Axum 0.7 and Express as `:name`.
    pub fn parse(path: &str) -> RouteTemplate {
        let path = path.split(['?', '#']).next().unwrap_or_default();
        let segments = path
            .split('/')
            .filter(|raw| !raw.is_empty())
            .map(parse_segment)
            .collect();
        RouteTemplate { segments }
    }

    /// The key that identifies a route regardless of parameter names:
    /// `/accounts/{}/invoices`.
    pub fn key(&self) -> String {
        if self.segments.is_empty() {
            return "/".to_owned();
        }
        let mut key = String::new();
        for segment in &self.segments {
            key.push('/');
            match segment {
                Segment::Literal(text) => key.push_str(text),
                Segment::Param(_) => key.push_str("{}"),
                Segment::Rest(_) => key.push_str("{*}"),
            }
        }
        key
    }

    /// True when the first segment is computed at runtime, which leaves the
    /// target route unknowable from source.
    pub fn starts_dynamic(&self) -> bool {
        matches!(
            self.segments.first(),
            Some(Segment::Param(_) | Segment::Rest(_))
        )
    }

    pub fn joined(&self, tail: &RouteTemplate) -> RouteTemplate {
        let mut segments = self.segments.clone();
        segments.extend(tail.segments.iter().cloned());
        RouteTemplate { segments }
    }

    /// Can a request built from `self` (the consumer side) reach `producer`?
    ///
    /// A consumer literal may fill a producer parameter. A consumer parameter
    /// never matches a producer literal: the value is not known statically,
    /// so claiming it would be a guess.
    pub fn reaches(&self, producer: &RouteTemplate) -> RouteMatch {
        let mut specific = 0;
        let mut filled_parameter = false;
        let mut consumer = self.segments.iter();
        for wanted in &producer.segments {
            match wanted {
                Segment::Rest(_) => {
                    // A rest parameter needs at least one segment to absorb.
                    if consumer.next().is_none() {
                        return RouteMatch::NoMatch;
                    }
                    return RouteMatch::Literal { specific };
                }
                Segment::Literal(expected) => match consumer.next() {
                    Some(Segment::Literal(sent)) if sent == expected => specific += 1,
                    _ => return RouteMatch::NoMatch,
                },
                Segment::Param(_) => match consumer.next() {
                    Some(Segment::Literal(_)) => filled_parameter = true,
                    Some(Segment::Param(_)) => {}
                    Some(Segment::Rest(_)) | None => return RouteMatch::NoMatch,
                },
            }
        }
        if consumer.next().is_some() {
            return RouteMatch::NoMatch;
        }
        if filled_parameter {
            RouteMatch::Literal { specific }
        } else {
            RouteMatch::Exact
        }
    }
}

fn parse_segment(raw: &str) -> Segment {
    if let Some(name) = raw.strip_prefix(':') {
        return Segment::Param(Some(name.to_owned()));
    }
    if let Some(inner) = raw.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
        if let Some(rest) = inner.strip_prefix('*') {
            return Segment::Rest(Some(rest.to_owned()).filter(|n| !n.is_empty()));
        }
        // FastAPI and Starlette converters: `{id:int}`, `{path:path}`.
        return match inner.split_once(':') {
            Some((name, "path")) => Segment::Rest(Some(name.to_owned())),
            Some((name, _converter)) => Segment::Param(Some(name.to_owned())),
            None => Segment::Param(Some(inner.to_owned()).filter(|n| !n.is_empty())),
        };
    }
    Segment::Literal(raw.to_owned())
}

impl fmt::Display for RouteTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.segments.is_empty() {
            return f.write_str("/");
        }
        for segment in &self.segments {
            match segment {
                Segment::Literal(text) => write!(f, "/{text}")?,
                Segment::Param(Some(name)) => write!(f, "/{{{name}}}")?,
                Segment::Param(None) => f.write_str("/{}")?,
                Segment::Rest(Some(name)) => write!(f, "/{{*{name}}}")?,
                Segment::Rest(None) => f.write_str("/{*}")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(s: &str) -> Segment {
        Segment::Literal(s.to_owned())
    }

    fn param(s: &str) -> Segment {
        Segment::Param(Some(s.to_owned()))
    }

    #[test]
    fn method_parsing_ignores_case_and_rejects_unknown_verbs() {
        assert_eq!("get".parse(), Ok(HttpMethod::Get));
        assert_eq!("PATCH".parse(), Ok(HttpMethod::Patch));
        assert_eq!(
            "fetch".parse::<HttpMethod>(),
            Err(UnknownMethod("fetch".to_owned()))
        );
    }

    #[test]
    fn parses_brace_and_colon_parameters() {
        let fastapi = RouteTemplate::parse("/accounts/{account_id}/invoices");
        assert_eq!(
            fastapi.segments,
            vec![lit("accounts"), param("account_id"), lit("invoices")]
        );
        let axum07 = RouteTemplate::parse("/ledger/accounts/:id/balance");
        assert_eq!(axum07.key(), "/ledger/accounts/{}/balance");
        assert_eq!(fastapi.key(), "/accounts/{}/invoices");
    }

    #[test]
    fn converters_and_rest_parameters_are_understood() {
        let typed = RouteTemplate::parse("/files/{file_id:int}");
        assert_eq!(typed.segments, vec![lit("files"), param("file_id")]);
        let rest = RouteTemplate::parse("/static/{path:path}");
        assert_eq!(
            rest.segments,
            vec![lit("static"), Segment::Rest(Some("path".to_owned()))]
        );
        let axum_rest = RouteTemplate::parse("/assets/{*rest}");
        assert_eq!(
            axum_rest.segments[1],
            Segment::Rest(Some("rest".to_owned()))
        );
    }

    #[test]
    fn slashes_and_query_strings_do_not_create_segments() {
        assert_eq!(RouteTemplate::parse("payments/").key(), "/payments");
        assert_eq!(RouteTemplate::parse("//a//b").key(), "/a/b");
        assert_eq!(RouteTemplate::parse("/search?q=1").key(), "/search");
        assert_eq!(RouteTemplate::parse("/").key(), "/");
    }

    #[test]
    fn parameter_names_do_not_affect_matching() {
        let consumer = RouteTemplate {
            segments: vec![lit("accounts"), Segment::Param(None)],
        };
        let producer = RouteTemplate::parse("/accounts/{account_id}");
        assert_eq!(consumer.reaches(&producer), RouteMatch::Exact);
    }

    #[test]
    fn a_literal_can_fill_a_parameter_but_not_the_reverse() {
        let me = RouteTemplate::parse("/accounts/me");
        let by_id = RouteTemplate::parse("/accounts/{id}");
        assert_eq!(me.reaches(&by_id), RouteMatch::Literal { specific: 1 });
        assert_eq!(by_id.reaches(&me), RouteMatch::NoMatch);
    }

    #[test]
    fn different_lengths_and_literals_do_not_match() {
        let a = RouteTemplate::parse("/accounts/{id}");
        assert_eq!(
            a.reaches(&RouteTemplate::parse("/accounts/{id}/invoices")),
            RouteMatch::NoMatch
        );
        assert_eq!(
            a.reaches(&RouteTemplate::parse("/users/{id}")),
            RouteMatch::NoMatch
        );
        assert_eq!(
            RouteTemplate::parse("/Accounts").reaches(&RouteTemplate::parse("/accounts")),
            RouteMatch::NoMatch
        );
    }

    #[test]
    fn a_rest_parameter_absorbs_the_remaining_segments() {
        let producer = RouteTemplate::parse("/static/{path:path}");
        let consumer = RouteTemplate::parse("/static/css/site.css");
        assert_eq!(
            consumer.reaches(&producer),
            RouteMatch::Literal { specific: 1 }
        );
        assert_eq!(
            RouteTemplate::parse("/static").reaches(&producer),
            RouteMatch::NoMatch
        );
    }

    #[test]
    fn dynamic_first_segments_are_flagged() {
        let dynamic = RouteTemplate {
            segments: vec![Segment::Param(Some("collection".to_owned())), lit("search")],
        };
        assert!(dynamic.starts_dynamic());
        assert!(!RouteTemplate::parse("/accounts/{id}").starts_dynamic());
    }

    #[test]
    fn joining_composes_prefixes() {
        let prefix = RouteTemplate::parse("/accounts");
        let joined = prefix.joined(&RouteTemplate::parse("/{account_id}/invoices"));
        assert_eq!(joined.to_string(), "/accounts/{account_id}/invoices");
        assert_eq!(
            RouteTemplate::default()
                .joined(&RouteTemplate::parse("/health"))
                .key(),
            "/health"
        );
    }
}
