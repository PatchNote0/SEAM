//! Docker Compose files: services, what they build, and their environment.
//!
//! Values of variables whose names suggest a secret are never kept, and the
//! excerpts that cite them show `<redacted>` instead of the value.

use serde::{Deserialize, Serialize};

use crate::syntax::{Located, Source};
use crate::tree::{Node, Value};

pub const ADAPTER: &str = "compose@0.1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComposeFile {
    pub services: Vec<Service>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
    pub name: String,
    /// The build context exactly as written, relative to the Compose file.
    pub build_context: Option<String>,
    pub image: Option<String>,
    pub environment: Vec<EnvEntry>,
    pub loc: Located,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnvEntry {
    pub name: String,
    /// `None` when the variable is declared without a value or when its name
    /// marks it as a secret.
    pub value: Option<String>,
    pub loc: Located,
}

pub fn is_compose_file(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    let Some(stem) = lower
        .strip_suffix(".yml")
        .or_else(|| lower.strip_suffix(".yaml"))
    else {
        return false;
    };
    stem == "docker-compose"
        || stem == "compose"
        || stem.starts_with("docker-compose.")
        || stem.starts_with("compose.")
}

/// Names that conventionally hold secrets.
pub fn looks_secret(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    [
        "PASSWORD",
        "PASSWD",
        "SECRET",
        "TOKEN",
        "PRIVATE",
        "CREDENTIAL",
        "API_KEY",
        "APIKEY",
        "ACCESS_KEY",
    ]
    .iter()
    .any(|marker| upper.contains(marker))
}

pub fn read(source: &Source<'_>, root: &Node) -> ComposeFile {
    let mut services = Vec::new();
    let Some(defs) = root.get("services") else {
        return ComposeFile { services };
    };
    for entry in defs.entries() {
        let service = &entry.value;
        let build_context = match service.get("build").map(|b| &b.value) {
            Some(Value::Scalar(path)) => Some(path.clone()),
            Some(Value::Map(_)) => service
                .get("build")
                .and_then(|b| b.get("context"))
                .and_then(Node::as_str)
                .map(str::to_owned),
            _ => None,
        };
        let environment = service
            .get("environment")
            .map(|env| environment(source, env))
            .unwrap_or_default();
        let span = entry.key_span.clone();
        let excerpt = seam_core::span::excerpt(source.text, &span);
        services.push(Service {
            name: entry.key.clone(),
            build_context,
            image: service
                .get("image")
                .and_then(Node::as_str)
                .map(str::to_owned),
            environment,
            loc: Located { span, excerpt },
        });
    }
    ComposeFile { services }
}

fn environment(source: &Source<'_>, env: &Node) -> Vec<EnvEntry> {
    match &env.value {
        Value::Map(entries) => entries
            .iter()
            .map(|e| {
                let secret = looks_secret(&e.key);
                let value = if secret {
                    None
                } else {
                    e.value.as_str().map(str::to_owned)
                };
                let span = seam_core::Span {
                    path: source.path.to_owned(),
                    start: e.key_span.start,
                    end: e.value.span.end,
                    start_byte: e.key_span.start_byte,
                    end_byte: e.value.span.end_byte,
                };
                entry(source, e.key.clone(), value, span, secret)
            })
            .collect(),
        Value::Seq(items) => items
            .iter()
            .filter_map(|item| {
                let text = item.as_str()?;
                let (name, value) = match text.split_once('=') {
                    Some((n, v)) => (n.to_owned(), Some(v.to_owned())),
                    None => (text.to_owned(), None),
                };
                let secret = looks_secret(&name);
                Some(entry(
                    source,
                    name,
                    if secret { None } else { value },
                    item.span.clone(),
                    secret,
                ))
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn entry(
    source: &Source<'_>,
    name: String,
    value: Option<String>,
    span: seam_core::Span,
    secret: bool,
) -> EnvEntry {
    let mut excerpt = seam_core::span::excerpt(source.text, &span);
    if secret {
        excerpt = redact(&excerpt, &name);
    }
    EnvEntry {
        name,
        value,
        loc: Located { span, excerpt },
    }
}

/// Replaces everything after the variable name and its separator.
fn redact(line: &str, name: &str) -> String {
    match line.find(name) {
        Some(at) => {
            let after = &line[at + name.len()..];
            let separator: String = after
                .chars()
                .take_while(|c| matches!(c, ':' | '=' | ' '))
                .collect();
            format!("{}{name}{separator}<redacted>", &line[..at])
        }
        None => "<redacted>".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::read_yaml;

    const COMPOSE: &str = "services:\n  web:\n    build: ./web\n    environment:\n      VITE_API_URL: http://api:8000\n  api:\n    build:\n      context: ./api\n    environment:\n      - DATABASE_URL=postgresql://db/app\n      - PAYMENTS_WEBHOOK_SECRET=s3cr3t\n      - DEBUG\n  db:\n    image: postgres:16\n    environment:\n      POSTGRES_PASSWORD: ledgerline\n";

    fn read_compose() -> ComposeFile {
        let source = Source::new("docker-compose.yml", COMPOSE);
        let doc = read_yaml(&source).expect("YAML loads");
        read(&source, &doc.root)
    }

    #[test]
    fn recognises_compose_file_names() {
        assert!(is_compose_file("docker-compose.yml"));
        assert!(is_compose_file("compose.yaml"));
        assert!(is_compose_file("docker-compose.override.yml"));
        assert!(!is_compose_file("openapi.yaml"));
    }

    #[test]
    fn reads_build_contexts_and_both_environment_styles() {
        let file = read_compose();
        let names: Vec<_> = file.services.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["web", "api", "db"]);
        assert_eq!(file.services[0].build_context.as_deref(), Some("./web"));
        assert_eq!(file.services[1].build_context.as_deref(), Some("./api"));
        let web_env = &file.services[0].environment[0];
        assert_eq!(
            (web_env.name.as_str(), web_env.value.as_deref()),
            ("VITE_API_URL", Some("http://api:8000"))
        );
        assert_eq!(web_env.loc.span.start.line, 5);
        let debug = file.services[1]
            .environment
            .iter()
            .find(|e| e.name == "DEBUG")
            .expect("DEBUG");
        assert_eq!(debug.value, None);
    }

    #[test]
    fn secret_values_are_never_kept() {
        let file = read_compose();
        let secret = file.services[1]
            .environment
            .iter()
            .find(|e| e.name == "PAYMENTS_WEBHOOK_SECRET")
            .expect("secret entry");
        assert_eq!(secret.value, None);
        assert!(!secret.loc.excerpt.contains("s3cr3t"));
        let password = &file.services[2].environment[0];
        assert_eq!(password.value, None);
        assert_eq!(password.loc.excerpt.trim(), "POSTGRES_PASSWORD: <redacted>");
    }
}
