//! Everything the linker knows about the repository: each file with its
//! facts, the components, what Docker Compose says about them, and how
//! imports resolve. Also the translation table for the Persian output.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use seam_adapters::compose::{EnvEntry, Service};
use seam_adapters::python::PythonFacts;
use seam_adapters::syntax::Located;
use seam_adapters::typescript::TsFacts;
use seam_adapters::{FileFacts, FileKind};
use seam_core::{ContentHash, Evidence, Method, Role};
use serde::Serialize;

pub struct FileRecord {
    pub path: String,
    pub hash: ContentHash,
    pub kind: FileKind,
    pub bytes: u64,
    pub facts: FileFacts,
    pub diagnostics: Vec<String>,
}

/// An independently built unit: a directory with its own manifest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Component {
    pub name: String,
    /// The directory, relative to the root; empty for the root itself.
    pub root: String,
    /// The manifest that names it, when there is one.
    pub manifest: Option<String>,
}

/// A Docker Compose service, with the component it builds when known.
pub struct ServiceInfo {
    pub service: Service,
    pub component: Option<usize>,
    /// The build context, normalised relative to the repository root.
    pub context: Option<String>,
}

pub struct Workspace {
    pub files: Vec<FileRecord>,
    pub by_path: HashMap<String, usize>,
    pub components: Vec<Component>,
    pub services: Vec<ServiceInfo>,
    /// Dotted Python module name to the files that may hold it.
    py_modules: HashMap<String, Vec<usize>>,
    /// English sentence to its Persian rendering, for every sentence the
    /// linker writes.
    translations: RefCell<BTreeMap<String, String>>,
}

pub fn dir_of(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// Joins `relative` onto `base` and resolves `.` and `..`. Returns `None` when
/// the result would leave the repository.
pub fn normalize(base: &str, relative: &str) -> Option<String> {
    let mut parts: Vec<&str> = if relative.starts_with('/') {
        Vec::new()
    } else {
        base.split('/').filter(|p| !p.is_empty()).collect()
    };
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(parts.join("/"))
}

const TS_EXTENSIONS: [&str; 8] = [".ts", ".tsx", ".mts", ".cts", ".js", ".jsx", ".mjs", ".cjs"];

impl Workspace {
    pub fn new(mut files: Vec<FileRecord>, root_name: &str) -> Workspace {
        files.sort_by(|a, b| a.path.cmp(&b.path));
        let by_path = files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.path.clone(), i))
            .collect();
        let components = components(&files, root_name);
        let mut ws = Workspace {
            files,
            by_path,
            components,
            services: Vec::new(),
            py_modules: HashMap::new(),
            translations: RefCell::new(BTreeMap::new()),
        };
        ws.services = ws.compose_services();
        ws.py_modules = ws.python_modules();
        ws
    }

    pub fn component_of(&self, path: &str) -> usize {
        let mut best = 0;
        let mut best_len = None;
        for (i, c) in self.components.iter().enumerate() {
            let inside =
                c.root.is_empty() || path == c.root || path.starts_with(&format!("{}/", c.root));
            if inside && best_len.is_none_or(|len| c.root.len() > len) {
                best = i;
                best_len = Some(c.root.len());
            }
        }
        best
    }

    pub fn ts(&self, file: usize) -> Option<&TsFacts> {
        match &self.files.get(file)?.facts {
            FileFacts::TypeScript(facts) => Some(facts),
            _ => None,
        }
    }

    pub fn python(&self, file: usize) -> Option<&PythonFacts> {
        match &self.files.get(file)?.facts {
            FileFacts::Python(facts) => Some(facts),
            _ => None,
        }
    }

    /// Records the Persian rendering of `en` and returns `en`.
    pub fn say(&self, en: String, fa: String) -> String {
        self.translations.borrow_mut().insert(en.clone(), fa);
        en
    }

    pub fn translations(&self) -> BTreeMap<String, String> {
        self.translations.borrow().clone()
    }

    pub fn evidence(
        &self,
        role: Role,
        loc: &Located,
        adapter: &str,
        technique: &str,
        note: String,
    ) -> Evidence {
        let file_hash = self
            .by_path
            .get(&loc.span.path)
            .map_or(ContentHash([0; 32]), |&i| self.files[i].hash);
        Evidence {
            role,
            span: loc.span.clone(),
            excerpt: loc.excerpt.clone(),
            file_hash,
            method: Method {
                adapter: adapter.to_owned(),
                technique: technique.to_owned(),
            },
            note,
        }
    }

    fn compose_services(&self) -> Vec<ServiceInfo> {
        let mut out = Vec::new();
        for file in &self.files {
            let FileFacts::Compose(compose) = &file.facts else {
                continue;
            };
            for service in &compose.services {
                let context = service
                    .build_context
                    .as_deref()
                    .and_then(|c| normalize(dir_of(&file.path), c));
                let component = context
                    .as_deref()
                    .and_then(|ctx| self.components.iter().position(|c| c.root == ctx));
                out.push(ServiceInfo {
                    service: service.clone(),
                    component,
                    context,
                });
            }
        }
        out
    }

    /// The services whose build context is `component`.
    pub fn services_building(&self, component: usize) -> impl Iterator<Item = &ServiceInfo> {
        self.services
            .iter()
            .filter(move |s| s.component == Some(component))
    }

    pub fn service_named(&self, name: &str) -> Option<&ServiceInfo> {
        self.services.iter().find(|s| s.service.name == name)
    }

    /// The value Compose gives `name` in a service that builds `component`.
    pub fn env_for(&self, component: usize, name: &str) -> Option<(&ServiceInfo, &EnvEntry)> {
        self.services_building(component).find_map(|s| {
            s.service
                .environment
                .iter()
                .find(|e| e.name == name && e.value.is_some())
                .map(|e| (s, e))
        })
    }

    fn python_modules(&self) -> HashMap<String, Vec<usize>> {
        let mut map: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, file) in self.files.iter().enumerate() {
            if file.kind != FileKind::Python {
                continue;
            }
            let Some(stem) = file.path.strip_suffix(".py") else {
                continue;
            };
            let mut parts: Vec<&str> = stem.split('/').collect();
            if parts.last() == Some(&"__init__") {
                parts.pop();
            }
            for start in 0..parts.len() {
                map.entry(parts[start..].join(".")).or_default().push(i);
            }
        }
        map
    }

    /// The file that holds the Python module `module`, as imported from `from`.
    pub fn resolve_py_module(&self, from: usize, module: &str) -> Option<usize> {
        if module.starts_with('.') {
            let dots = module.chars().take_while(|c| *c == '.').count();
            let mut base = dir_of(&self.files[from].path).to_owned();
            for _ in 1..dots {
                base = dir_of(&base).to_owned();
            }
            let rest = module[dots..].replace('.', "/");
            let stem = if rest.is_empty() {
                base
            } else {
                normalize(&base, &rest)?
            };
            return [format!("{stem}.py"), format!("{stem}/__init__.py")]
                .iter()
                .find_map(|p| self.by_path.get(p).copied());
        }
        let candidates = self.py_modules.get(module)?;
        let component = self.component_of(&self.files[from].path);
        let local: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&c| self.component_of(&self.files[c].path) == component)
            .collect();
        match (local.as_slice(), candidates.as_slice()) {
            ([one], _) => Some(*one),
            ([], [one]) => Some(*one),
            _ => None,
        }
    }

    /// The file a relative TypeScript import points at.
    pub fn resolve_ts_module(&self, from: usize, specifier: &str) -> Option<usize> {
        if !specifier.starts_with("./") && !specifier.starts_with("../") {
            return None;
        }
        let target = normalize(dir_of(&self.files[from].path), specifier)?;
        let mut candidates = vec![target.clone()];
        let stripped = TS_EXTENSIONS
            .iter()
            .find_map(|ext| target.strip_suffix(ext))
            .unwrap_or(&target)
            .to_owned();
        for ext in TS_EXTENSIONS {
            candidates.push(format!("{stripped}{ext}"));
        }
        for ext in TS_EXTENSIONS {
            candidates.push(format!("{target}/index{ext}"));
        }
        candidates
            .iter()
            .find_map(|p| self.by_path.get(p).copied())
            .filter(|&i| self.files[i].kind == FileKind::TypeScript)
    }
}

fn components(files: &[FileRecord], root_name: &str) -> Vec<Component> {
    let mut by_dir: BTreeMap<String, (Option<String>, String)> = BTreeMap::new();
    for file in files {
        let FileFacts::Manifest(manifest) = &file.facts else {
            continue;
        };
        let dir = dir_of(&file.path).to_owned();
        let fallback = if dir.is_empty() {
            root_name.to_owned()
        } else {
            dir.rsplit('/').next().unwrap_or(&dir).to_owned()
        };
        let name = manifest.name.clone().unwrap_or(fallback);
        // The first manifest in a directory, in path order, names it.
        by_dir.entry(dir).or_insert((Some(file.path.clone()), name));
    }
    by_dir
        .entry(String::new())
        .or_insert((None, root_name.to_owned()));
    by_dir
        .into_iter()
        .map(|(root, (manifest, name))| Component {
            name,
            root,
            manifest,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_resolves_dots_and_refuses_to_leave_the_root() {
        assert_eq!(normalize("", "./web").as_deref(), Some("web"));
        assert_eq!(normalize("deploy", "../api").as_deref(), Some("api"));
        assert_eq!(
            normalize("web/src/api", "./http").as_deref(),
            Some("web/src/api/http")
        );
        assert_eq!(
            normalize("web/src/components", "../api/types").as_deref(),
            Some("web/src/api/types")
        );
        assert_eq!(normalize("", "../outside"), None);
    }
}
