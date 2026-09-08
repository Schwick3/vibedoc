pub mod adapter;
pub mod config;
pub mod diagnostic;
pub mod document;
pub mod rules;

pub use adapter::{AdapterClient, AdapterError, AdapterRunOptions, find_adapter};
pub use config::{
    Config, ConfigError, DiscoveredConfig, DocumentProfile, DocumentTarget,
    collect_default_documents, collect_documents, discover_config, discover_project_config,
};
pub use diagnostic::{Diagnostic, OutputReport, Severity};
pub use document::{BindingDirective, Document, ReferenceClaims, parse_document};
pub use rules::{CheckOptions, RuleMetadata, all_rule_metadata, check_documents, rule_metadata};
