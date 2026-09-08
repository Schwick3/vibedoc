use crate::diagnostic::Severity;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use walkdir::{DirEntry, WalkDir};

pub const CONFIG_FILE: &str = "vibedoc.toml";

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("configuration file does not exist: {0}")]
    Missing(PathBuf),
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid configuration in {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("unsupported configuration version {0}; expected version 1")]
    Version(u32),
    #[error("invalid glob `{pattern}`: {message}")]
    Glob { pattern: String, message: String },
    #[error("document {path} matches conflicting profiles: {profiles}")]
    ConflictingProfiles { path: PathBuf, profiles: String },
    #[error("document path does not exist: {0}")]
    MissingDocument(PathBuf),
    #[error("no Markdown documents matched")]
    NoDocuments,
}

#[derive(Debug, Clone)]
pub struct DiscoveredConfig {
    pub path: PathBuf,
    pub root: PathBuf,
    pub config: Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    #[serde(default)]
    pub documents: Vec<DocumentSet>,
    #[serde(default)]
    pub adapters: BTreeMap<String, AdapterConfig>,
    #[serde(default)]
    pub rules: BTreeMap<String, RuleLevel>,
    #[serde(default)]
    pub terms: BTreeMap<String, TermConfig>,
    #[serde(default)]
    pub experimental: ExperimentalConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            documents: Vec::new(),
            adapters: BTreeMap::new(),
            rules: BTreeMap::new(),
            terms: BTreeMap::new(),
            experimental: ExperimentalConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentSet {
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    pub profile: DocumentProfile,
    #[serde(default)]
    pub adapters: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum DocumentProfile {
    Guide,
    Reference,
}

impl std::fmt::Display for DocumentProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Guide => "guide",
            Self::Reference => "reference",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AdapterConfig {
    #[serde(default)]
    pub projects: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TermConfig {
    #[serde(default)]
    pub forbidden: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ExperimentalConfig {
    #[serde(default)]
    pub prose_grounding: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleLevel {
    Off,
    Info,
    Warning,
    Error,
}

impl RuleLevel {
    pub fn severity(self) -> Option<Severity> {
        match self {
            Self::Off => None,
            Self::Info => Some(Severity::Info),
            Self::Warning => Some(Severity::Warning),
            Self::Error => Some(Severity::Error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentTarget {
    pub path: PathBuf,
    pub profile: DocumentProfile,
    pub adapters: Vec<String>,
}

pub fn discover_config(
    start: &Path,
    explicit: Option<&Path>,
) -> Result<Option<DiscoveredConfig>, ConfigError> {
    if let Some(path) = explicit {
        let path = absolutize(start, path);
        if !path.is_file() {
            return Err(ConfigError::Missing(path));
        }
        return load_discovered(path).map(Some);
    }

    let mut cursor = if start.is_file() {
        start.parent().unwrap_or(start).to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        let candidate = cursor.join(CONFIG_FILE);
        if candidate.is_file() {
            return load_discovered(candidate).map(Some);
        }
        if !cursor.pop() {
            return Ok(None);
        }
    }
}

fn load_discovered(path: PathBuf) -> Result<DiscoveredConfig, ConfigError> {
    let text = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source,
    })?;
    let config: Config = toml::from_str(&text).map_err(|error| ConfigError::Parse {
        path: path.clone(),
        message: error.to_string(),
    })?;
    if config.version != 1 {
        return Err(ConfigError::Version(config.version));
    }
    let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok(DiscoveredConfig { path, root, config })
}

pub fn collect_documents(root: &Path, config: &Config) -> Result<Vec<DocumentTarget>, ConfigError> {
    let sets = config
        .documents
        .iter()
        .map(CompiledDocumentSet::new)
        .collect::<Result<Vec<_>, _>>()?;
    let mut matches: BTreeMap<PathBuf, Vec<&CompiledDocumentSet<'_>>> = BTreeMap::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(walk_entry)
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        let relative = path.strip_prefix(root).unwrap_or(path);
        for set in &sets {
            if set.includes.is_match(relative) && !set.excludes.is_match(relative) {
                matches.entry(path.to_path_buf()).or_default().push(set);
            }
        }
    }

    let mut targets = Vec::new();
    for (path, matching_sets) in matches {
        let profiles = matching_sets
            .iter()
            .map(|set| set.source.profile)
            .collect::<BTreeSet<_>>();
        if profiles.len() > 1 {
            return Err(ConfigError::ConflictingProfiles {
                path,
                profiles: profiles
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        }
        let adapters = matching_sets
            .iter()
            .flat_map(|set| set.source.adapters.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        targets.push(DocumentTarget {
            path,
            profile: *profiles
                .iter()
                .next()
                .expect("a matching set has a profile"),
            adapters,
        });
    }
    if targets.is_empty() {
        return Err(ConfigError::NoDocuments);
    }
    Ok(targets)
}

pub fn collect_explicit_documents(
    root: &Path,
    paths: &[PathBuf],
    profile: DocumentProfile,
    adapters: Vec<String>,
) -> Result<Vec<DocumentTarget>, ConfigError> {
    let mut output = BTreeSet::new();
    for requested in paths {
        let path = absolutize(root, requested);
        if !path.exists() {
            return Err(ConfigError::MissingDocument(path));
        }
        if path.is_file() {
            if path.extension().and_then(|value| value.to_str()) == Some("md") {
                output.insert(path);
            }
            continue;
        }
        for entry in WalkDir::new(&path)
            .follow_links(false)
            .into_iter()
            .filter_entry(walk_entry)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if entry.path().extension().and_then(|value| value.to_str()) == Some("md") {
                output.insert(entry.path().to_path_buf());
            }
        }
    }
    if output.is_empty() {
        return Err(ConfigError::NoDocuments);
    }
    Ok(output
        .into_iter()
        .map(|path| DocumentTarget {
            path,
            profile,
            adapters: adapters.clone(),
        })
        .collect())
}

pub fn collect_default_documents(root: &Path) -> Result<Vec<DocumentTarget>, ConfigError> {
    let mut paths = Vec::new();
    let readme = root.join("README.md");
    if readme.is_file() {
        paths.push(readme);
    }
    let docs = root.join("docs");
    if docs.is_dir() {
        for entry in WalkDir::new(docs)
            .follow_links(false)
            .into_iter()
            .filter_entry(walk_entry)
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if entry.path().extension().and_then(|value| value.to_str()) == Some("md") {
                paths.push(entry.path().to_path_buf());
            }
        }
    }
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Err(ConfigError::NoDocuments);
    }
    Ok(paths
        .into_iter()
        .map(|path| DocumentTarget {
            path,
            profile: DocumentProfile::Guide,
            adapters: Vec::new(),
        })
        .collect())
}

pub fn discover_project_config(start: &Path, root: &Path) -> Option<PathBuf> {
    let mut cursor = if start.is_file() {
        start.parent()?.to_path_buf()
    } else {
        start.to_path_buf()
    };
    loop {
        for name in ["tsconfig.json", "jsconfig.json"] {
            let candidate = cursor.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        if cursor == root || !cursor.starts_with(root) || !cursor.pop() {
            return None;
        }
    }
}

pub fn project_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn absolutize(root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    }
}

struct CompiledDocumentSet<'a> {
    source: &'a DocumentSet,
    includes: GlobSet,
    excludes: GlobSet,
}

impl<'a> CompiledDocumentSet<'a> {
    fn new(source: &'a DocumentSet) -> Result<Self, ConfigError> {
        Ok(Self {
            source,
            includes: compile_globs(&source.include)?,
            excludes: compile_globs(&source.exclude)?,
        })
    }
}

fn compile_globs(patterns: &[String]) -> Result<GlobSet, ConfigError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| ConfigError::Glob {
            pattern: pattern.clone(),
            message: error.to_string(),
        })?;
        builder.add(glob);
    }
    builder.build().map_err(|error| ConfigError::Glob {
        pattern: patterns.join(", "),
        message: error.to_string(),
    })
}

fn walk_entry(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    !matches!(
        entry.file_name().to_str(),
        Some(".git" | "node_modules" | "target" | "dist" | "build")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn discovers_nearest_config_and_rejects_unknown_keys() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("a/b")).unwrap();
        fs::write(temp.path().join(CONFIG_FILE), "version = 1\n").unwrap();
        let found = discover_config(&temp.path().join("a/b"), None)
            .unwrap()
            .unwrap();
        assert_eq!(found.root, temp.path());

        fs::write(
            temp.path().join(CONFIG_FILE),
            "version = 1\nunknown = true\n",
        )
        .unwrap();
        assert!(discover_config(&temp.path().join("a"), None).is_err());
    }

    #[test]
    fn rejects_conflicting_document_profiles() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("docs")).unwrap();
        let mut file = fs::File::create(temp.path().join("docs/api.md")).unwrap();
        writeln!(file, "# API").unwrap();
        let config: Config = toml::from_str(
            r#"
version = 1
[[documents]]
include = ["docs/*.md"]
profile = "guide"
[[documents]]
include = ["docs/api.md"]
profile = "reference"
"#,
        )
        .unwrap();
        assert!(matches!(
            collect_documents(temp.path(), &config),
            Err(ConfigError::ConflictingProfiles { .. })
        ));
    }

    #[test]
    fn assigns_profiles_and_honors_excludes() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir_all(temp.path().join("docs/archive")).unwrap();
        fs::write(temp.path().join("docs/api.md"), "# API\n").unwrap();
        fs::write(temp.path().join("docs/archive/old.md"), "# Old\n").unwrap();
        let config: Config = toml::from_str(
            r#"
version = 1
[[documents]]
include = ["docs/**/*.md"]
exclude = ["docs/archive/**"]
profile = "reference"
adapters = ["typescript"]
"#,
        )
        .unwrap();
        let targets = collect_documents(temp.path(), &config).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].profile, DocumentProfile::Reference);
        assert_eq!(targets[0].adapters, ["typescript"]);
        assert!(targets[0].path.ends_with("docs/api.md"));
    }

    #[test]
    fn explicit_configuration_and_nearest_project_have_precedence() {
        let temp = tempfile::tempdir().unwrap();
        let nested = temp.path().join("packages/web/src");
        fs::create_dir_all(&nested).unwrap();
        fs::write(temp.path().join(CONFIG_FILE), "version = 1\n").unwrap();
        fs::write(temp.path().join("tsconfig.json"), "{}\n").unwrap();
        fs::write(
            temp.path().join("packages/web/vibedoc.toml"),
            "version = 1\n",
        )
        .unwrap();
        fs::write(temp.path().join("packages/web/jsconfig.json"), "{}\n").unwrap();

        let nearest = discover_config(&nested, None).unwrap().unwrap();
        assert!(nearest.path.ends_with("packages/web/vibedoc.toml"));

        let explicit = discover_config(&nested, Some(temp.path().join(CONFIG_FILE).as_path()))
            .unwrap()
            .unwrap();
        assert_eq!(explicit.root, temp.path());

        let project = discover_project_config(&nested, temp.path()).unwrap();
        assert!(project.ends_with("packages/web/jsconfig.json"));
    }
}
