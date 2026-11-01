//! Per-file extraction.
//!
//! An adapter reads one file and returns the facts found in it, each with
//! the span that proves it. Adapters never see other files; anything that
//! needs two files happens in `seam-analysis`. Malformed input yields the
//! facts that could be read plus a diagnostic, never a panic.

pub mod compose;
pub mod manifest;
pub mod openapi;
pub mod python;
pub mod syntax;
pub mod tree;
pub mod typescript;

use serde::{Deserialize, Serialize};

pub use manifest::ManifestKind;

/// How a file is read, decided from its path alone.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileKind {
    /// TypeScript, JavaScript and their JSX forms.
    TypeScript,
    Python,
    /// YAML: a Compose file by name, otherwise OpenAPI when its content says so.
    Yaml,
    /// JSON: OpenAPI when its content says so.
    Json,
    Manifest(ManifestKind),
}

const SCRIPT_EXTENSIONS: [&str; 8] = ["ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs"];

/// The kind of a repository-relative path, or `None` when SEAM does not read it.
pub fn kind_for_path(path: &str) -> Option<FileKind> {
    let file = path.rsplit('/').next().unwrap_or(path);
    if let Some(kind) = manifest::kind_for_file_name(file) {
        return Some(FileKind::Manifest(kind));
    }
    if file.ends_with(".d.ts") || file.ends_with(".min.js") || file.ends_with(".bundle.js") {
        return None;
    }
    let extension = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase())?;
    match extension.as_str() {
        e if SCRIPT_EXTENSIONS.contains(&e) => Some(FileKind::TypeScript),
        "py" => Some(FileKind::Python),
        "yaml" | "yml" => Some(FileKind::Yaml),
        "json" => Some(FileKind::Json),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "facts", rename_all = "snake_case")]
pub enum FileFacts {
    TypeScript(typescript::TsFacts),
    Python(python::PythonFacts),
    OpenApi(openapi::OpenApiDoc),
    Compose(compose::ComposeFile),
    Manifest(manifest::Manifest),
    /// A YAML or JSON file that is neither OpenAPI nor Compose.
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extraction {
    pub facts: FileFacts,
    /// Problems worth showing, each a sentence. They never stop a scan.
    pub diagnostics: Vec<String>,
}

/// Reads one file. `path` is repository-relative with `/` separators.
pub fn extract(path: &str, kind: FileKind, text: &str) -> Extraction {
    let source = syntax::Source::new(path, text);
    let mut diagnostics = Vec::new();
    let facts = match kind {
        FileKind::TypeScript => match typescript::extract(&source) {
            Ok(facts) => {
                if facts.syntax_errors {
                    diagnostics.push(RECOVERED.to_owned());
                }
                FileFacts::TypeScript(facts)
            }
            Err(e) => failed(&mut diagnostics, e),
        },
        FileKind::Python => match python::extract(&source) {
            Ok(facts) => {
                if facts.syntax_errors {
                    diagnostics.push(RECOVERED.to_owned());
                }
                FileFacts::Python(facts)
            }
            Err(e) => failed(&mut diagnostics, e),
        },
        FileKind::Yaml | FileKind::Json => {
            let read = if kind == FileKind::Yaml {
                tree::read_yaml(&source)
            } else {
                tree::read_json(&source)
            };
            match read {
                Ok(document) => {
                    let file = path.rsplit('/').next().unwrap_or(path);
                    let facts = if kind == FileKind::Yaml && compose::is_compose_file(file) {
                        FileFacts::Compose(compose::read(&source, &document.root))
                    } else if openapi::is_openapi(&document.root) {
                        FileFacts::OpenApi(openapi::read(&source, &document.root))
                    } else {
                        FileFacts::Other
                    };
                    if !matches!(facts, FileFacts::Other) {
                        diagnostics.extend(document.problems);
                    }
                    facts
                }
                Err(e) => failed(&mut diagnostics, e),
            }
        }
        FileKind::Manifest(manifest_kind) => {
            FileFacts::Manifest(manifest::read(&source, manifest_kind))
        }
    };
    Extraction { facts, diagnostics }
}

const RECOVERED: &str = "The file has syntax errors; facts were read from the parts that parsed.";

fn failed(diagnostics: &mut Vec<String>, error: syntax::ParseError) -> FileFacts {
    diagnostics.push(format!("The file could not be parsed: {error}."));
    FileFacts::Other
}

/// What an adapter does with one feature of its language.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Support {
    /// Read from the file itself.
    Read,
    /// Followed across files by the linker.
    Resolved,
    /// Not handled; anything that depends on it becomes an assumption or UNKNOWN.
    NotRead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Capability {
    pub language: &'static str,
    pub feature: &'static str,
    pub support: Support,
}

/// Every adapter's declared capabilities, printed by `seam languages` and
/// quoted when a missing capability forces an assumption.
pub fn capabilities() -> Vec<Capability> {
    use Support::*;
    const TS: &str = "TypeScript and JavaScript";
    let table: [(&str, &str, Support); 40] = [
        (
            TS,
            "fetch calls, with the method from a literal init object",
            Read,
        ),
        (
            TS,
            "axios.get, post, put, patch, delete, head and options",
            Read,
        ),
        (TS, "axios instances made with axios.create", NotRead),
        (
            TS,
            "functions that wrap a request and take the path as a parameter",
            Resolved,
        ),
        (TS, "template literals and + concatenation in URLs", Read),
        (TS, "module constants, local or imported", Resolved),
        (
            TS,
            "import.meta.env and process.env, with a ?? or || fallback",
            Read,
        ),
        (TS, "environment values set in Docker Compose", Resolved),
        (TS, "relative imports", Resolved),
        (TS, "tsconfig path aliases and package imports", NotRead),
        (
            TS,
            "interfaces and type aliases, including extends",
            Resolved,
        ),
        (TS, "generic type arguments at call sites", Read),
        (
            TS,
            "generics other than Array, ReadonlyArray and Promise",
            NotRead,
        ),
        (TS, "intersection, mapped and conditional types", NotRead),
        (TS, "inferred types without an annotation", NotRead),
        (TS, "class methods and object-literal methods", NotRead),
        (
            TS,
            "it and test blocks in *.test.* and *.spec.* files",
            Read,
        ),
        ("Python", "FastAPI routes on an app or an APIRouter", Read),
        ("Python", "api_route with a methods list", Read),
        (
            "Python",
            "router prefixes and include_router across modules",
            Resolved,
        ),
        ("Python", "response_model and return annotations", Read),
        (
            "Python",
            "request bodies from parameters typed with a model",
            Read,
        ),
        (
            "Python",
            "Pydantic models, including inheritance between models",
            Resolved,
        ),
        (
            "Python",
            "alias, serialization_alias and validation_alias",
            Read,
        ),
        ("Python", "alias generators in model_config", NotRead),
        (
            "Python",
            "Optional, Union, X | None, list, dict, Literal and Annotated",
            Read,
        ),
        ("Python", "TestClient requests in test functions", Read),
        ("Python", "Flask, Django and other frameworks", NotRead),
        (
            "Python",
            "outgoing HTTP calls with httpx or requests",
            NotRead,
        ),
        (
            "OpenAPI",
            "OpenAPI 3.0 and 3.1 operations in YAML or JSON",
            Read,
        ),
        ("OpenAPI", "local $ref", Resolved),
        ("OpenAPI", "references to other files", NotRead),
        ("OpenAPI", "allOf, oneOf, anyOf and nullable", Read),
        ("OpenAPI", "the first 2xx response of an operation", Read),
        (
            "Docker Compose",
            "services, build contexts and images",
            Read,
        ),
        ("Docker Compose", "environment as a map or a list", Read),
        (
            "Docker Compose",
            "env_file and ${VAR} interpolation",
            NotRead,
        ),
        (
            "Manifests",
            "package.json, pyproject.toml, Cargo.toml and go.mod names",
            Read,
        ),
        ("Rust, Go, Java, SQL, GraphQL", "any extraction", NotRead),
        ("All", "running or importing the analysed code", NotRead),
    ];
    table
        .into_iter()
        .map(|(language, feature, support)| Capability {
            language,
            feature,
            support,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_grammar_loads() {
        for (name, language) in [
            (
                "typescript",
                tree_sitter::Language::from(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            ),
            ("tsx", tree_sitter_typescript::LANGUAGE_TSX.into()),
            ("python", tree_sitter_python::LANGUAGE.into()),
            ("yaml", tree_sitter_yaml::LANGUAGE.into()),
            ("json", tree_sitter_json::LANGUAGE.into()),
        ] {
            assert!(
                syntax::parse(&language, "").is_ok(),
                "{name} grammar failed to load"
            );
        }
    }

    #[test]
    fn kinds_come_from_paths() {
        assert_eq!(
            kind_for_path("web/src/api/http.ts"),
            Some(FileKind::TypeScript)
        );
        assert_eq!(kind_for_path("web/src/App.jsx"), Some(FileKind::TypeScript));
        assert_eq!(kind_for_path("api/app/main.py"), Some(FileKind::Python));
        assert_eq!(
            kind_for_path("web/package.json"),
            Some(FileKind::Manifest(ManifestKind::Npm))
        );
        assert_eq!(kind_for_path("api/openapi.yaml"), Some(FileKind::Yaml));
        assert_eq!(kind_for_path("web/src/env.d.ts"), None);
        assert_eq!(kind_for_path("web/dist/app.min.js"), None);
        assert_eq!(kind_for_path("README.md"), None);
        assert_eq!(kind_for_path("Makefile"), None);
    }

    #[test]
    fn yaml_is_compose_by_name_and_openapi_by_content() {
        let compose = extract(
            "docker-compose.yml",
            FileKind::Yaml,
            "services:\n  api:\n    build: ./api\n",
        );
        assert!(matches!(compose.facts, FileFacts::Compose(_)));
        let spec = extract(
            "api/spec.yaml",
            FileKind::Yaml,
            "openapi: 3.1.0\npaths: {}\n",
        );
        assert!(matches!(spec.facts, FileFacts::OpenApi(_)));
        let other = extract(
            ".github/workflows/ci.yml",
            FileKind::Yaml,
            "on: push\njobs: {}\n",
        );
        assert_eq!(other.facts, FileFacts::Other);
    }

    #[test]
    fn broken_files_give_diagnostics_not_panics() {
        let ts = extract("web/src/x.ts", FileKind::TypeScript, "export function ( {");
        assert!(matches!(ts.facts, FileFacts::TypeScript(_)));
        assert_eq!(ts.diagnostics.len(), 1);
        let yaml = extract(
            "api/openapi.yaml",
            FileKind::Yaml,
            "openapi: [unclosed\n  paths: {",
        );
        assert!(!matches!(yaml.facts, FileFacts::Compose(_)));
    }

    #[test]
    fn capabilities_are_declared_for_every_adapter() {
        let caps = capabilities();
        for language in [
            "TypeScript and JavaScript",
            "Python",
            "OpenAPI",
            "Docker Compose",
            "Manifests",
        ] {
            assert!(
                caps.iter().any(|c| c.language == language),
                "{language} declares nothing"
            );
        }
    }
}
