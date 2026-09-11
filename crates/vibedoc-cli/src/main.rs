use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;
use vibedoc_core::config::{absolutize, collect_explicit_documents, project_relative};
use vibedoc_core::{
    AdapterClient, AdapterRunOptions, CheckOptions, Config, Diagnostic, DocumentProfile,
    OutputReport, Severity, collect_default_documents, collect_documents, discover_config,
    discover_project_config, find_adapter, parse_document, rule_metadata,
};
use vibedoc_protocol::{AdapterSeverity, AnalyzeParams, FactGraph};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(
    name = "vibedoc",
    version,
    about = "Evidence-aware software documentation checker"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a minimal vibedoc.toml in the current directory.
    Init(InitArgs),
    /// Check Markdown documentation.
    Check(CheckArgs),
    /// Validate configuration, projects, and adapter availability.
    Doctor(DoctorArgs),
    /// Inspect facts returned by a source adapter.
    Inspect(InspectArgs),
    /// Explain a Vibedoc rule.
    Explain(ExplainArgs),
}

#[derive(Args)]
struct InitArgs {
    /// Replace an existing vibedoc.toml.
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct CheckArgs {
    /// Markdown files or directories. Configured document sets are used when omitted.
    paths: Vec<PathBuf>,
    /// Use this configuration instead of discovering vibedoc.toml.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Override the profile for explicitly supplied paths.
    #[arg(long, value_enum)]
    profile: Option<ProfileArg>,
    /// Diagnostic output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    /// Override a project as ADAPTER=PATH. May be repeated.
    #[arg(long = "project", value_name = "ADAPTER=PATH")]
    projects: Vec<String>,
    /// Override an adapter executable as ADAPTER=/ABSOLUTE/PATH. May be repeated.
    #[arg(long = "adapter-command", value_name = "ADAPTER=PATH")]
    adapter_commands: Vec<String>,
    /// Enable warning-only experimental prose grounding.
    #[arg(long)]
    experimental: bool,
    /// Return exit code 1 when warnings are present.
    #[arg(long)]
    deny_warnings: bool,
}

#[derive(Args)]
struct DoctorArgs {
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args)]
struct InspectArgs {
    /// Adapter to inspect.
    #[arg(long, default_value = "typescript")]
    adapter: String,
    /// Filter symbols by name, qualified name, or ID.
    #[arg(long)]
    symbol: Option<String>,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Args)]
struct ExplainArgs {
    rule_id: String,
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ProfileArg {
    Guide,
    Reference,
}

impl From<ProfileArg> for DocumentProfile {
    fn from(value: ProfileArg) -> Self {
        match value {
            ProfileArg::Guide => Self::Guide,
            ProfileArg::Reference => Self::Reference,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug)]
struct CliError(String);

impl From<std::io::Error> for CliError {
    fn from(value: std::io::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<serde_json::Error> for CliError {
    fn from(value: serde_json::Error) -> Self {
        Self(value.to_string())
    }
}

impl From<vibedoc_core::ConfigError> for CliError {
    fn from(value: vibedoc_core::ConfigError) -> Self {
        Self(value.to_string())
    }
}

impl From<vibedoc_core::AdapterError> for CliError {
    fn from(value: vibedoc_core::AdapterError) -> Self {
        Self(value.to_string())
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = command_format(&cli.command);
    match run(cli.command) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            if format == OutputFormat::Json {
                let output = serde_json::json!({
                    "schemaVersion": 1,
                    "tool": { "name": "vibedoc", "version": VERSION },
                    "status": "error",
                    "error": { "message": error.0 }
                });
                println!("{}", serde_json::to_string_pretty(&output).unwrap());
            } else {
                eprintln!("vibedoc: {}", error.0);
            }
            ExitCode::from(2)
        }
    }
}

fn command_format(command: &Command) -> OutputFormat {
    match command {
        Command::Init(_) => OutputFormat::Text,
        Command::Check(args) => args.format,
        Command::Doctor(args) => args.format,
        Command::Inspect(args) => args.format,
        Command::Explain(args) => args.format,
    }
}

fn run(command: Command) -> Result<u8, CliError> {
    match command {
        Command::Init(args) => init(args),
        Command::Check(args) => check(args),
        Command::Doctor(args) => doctor(args),
        Command::Inspect(args) => inspect(args),
        Command::Explain(args) => explain(args),
    }
}

fn init(args: InitArgs) -> Result<u8, CliError> {
    let root = env::current_dir()?;
    let path = root.join("vibedoc.toml");
    if path.exists() && !args.force {
        return Err(CliError(format!(
            "{} already exists; use --force to replace it",
            path.display()
        )));
    }

    let has_readme = root.join("README.md").is_file();
    let has_docs = root.join("docs").is_dir();
    let project = ["tsconfig.json", "jsconfig.json"]
        .into_iter()
        .find(|name| root.join(name).is_file());
    let mut output = String::from("version = 1\n\n");
    if has_readme || has_docs {
        output.push_str("[[documents]]\ninclude = [");
        let mut includes = Vec::new();
        if has_readme {
            includes.push("\"README.md\"");
        }
        if has_docs {
            includes.push("\"docs/**/*.md\"");
        }
        output.push_str(&includes.join(", "));
        output.push_str("]\nprofile = \"guide\"\n\n");
    } else {
        output.push_str(
            "[[documents]]\ninclude = [\"README.md\", \"docs/**/*.md\"]\nprofile = \"guide\"\n\n",
        );
    }
    if let Some(project) = project {
        output.push_str("[adapters.typescript]\n");
        output.push_str(&format!("projects = [\"{project}\"]\n"));
    } else {
        output.push_str(
            "# [adapters.typescript]\n# projects = [\"tsconfig.json\"]\n# sources = [\"src/**/*.ts\", \"src/**/*.tsx\", \"src/**/*.js\", \"src/**/*.jsx\"]\n",
        );
    }
    output.push_str(
        "\n[experimental]\nprose_grounding = false\n\n# [terms.\"access token\"]\n# forbidden = [\"auth token\", \"login token\"]\n",
    );
    fs::write(&path, output)?;
    println!("Created {}", path.display());
    Ok(0)
}

fn check(args: CheckArgs) -> Result<u8, CliError> {
    let cwd = env::current_dir()?;
    let discovered = discover_config(&cwd, args.config.as_deref())?;
    let root = discovered
        .as_ref()
        .map_or_else(|| cwd.clone(), |found| found.root.clone());
    let config = discovered
        .as_ref()
        .map_or_else(Config::default, |found| found.config.clone());
    let profile = args
        .profile
        .map(DocumentProfile::from)
        .unwrap_or(DocumentProfile::Guide);
    let targets = if args.paths.is_empty() {
        if discovered.is_some() {
            collect_documents(&root, &config)?
        } else {
            collect_default_documents(&root)?
        }
    } else {
        collect_explicit_documents(&cwd, &args.paths, profile, Vec::new())?
    };

    let mut documents = Vec::new();
    let mut adapters = BTreeSet::new();
    for target in targets {
        let target_has_adapter = !target.adapters.is_empty();
        let source = fs::read_to_string(&target.path)?;
        let document = parse_document(
            &target.path,
            project_relative(&root, &target.path),
            target.profile,
            source,
        );
        adapters.extend(target.adapters);
        let directive_adapters = document
            .directives
            .iter()
            .filter_map(|directive| directive.adapter.clone())
            .collect::<Vec<_>>();
        let has_directive_adapter = !directive_adapters.is_empty();
        adapters.extend(directive_adapters);
        if target.profile == DocumentProfile::Reference
            && !target_has_adapter
            && !has_directive_adapter
        {
            adapters.insert("typescript".into());
        }
        documents.push(document);
    }

    let project_overrides = parse_assignments(&args.projects, false)?;
    let command_overrides = parse_assignments(&args.adapter_commands, true)?;
    let mut graph = FactGraph::default();
    for adapter in adapters {
        let analysis = analyze_adapter(
            &adapter,
            &root,
            &cwd,
            &config,
            &project_overrides,
            &command_overrides,
        )?;
        graph.files.extend(analysis.graph.files);
        graph.symbols.extend(analysis.graph.symbols);
        graph.relationships.extend(analysis.graph.relationships);
    }
    graph
        .files
        .sort_by(|left, right| left.path.cmp(&right.path));
    graph.symbols.sort_by(|left, right| left.id.cmp(&right.id));

    let checked = vibedoc_core::check_documents(
        &documents,
        &graph,
        &config,
        CheckOptions {
            experimental: args.experimental,
        },
    );
    let report = OutputReport::new(
        VERSION,
        checked.diagnostics,
        checked.verification,
        documents.len(),
        args.deny_warnings,
    );
    render_report(&report, args.format)?;
    Ok(if report.failed() { 1 } else { 0 })
}

fn analyze_adapter(
    name: &str,
    root: &Path,
    cwd: &Path,
    config: &Config,
    project_overrides: &BTreeMap<String, PathBuf>,
    command_overrides: &BTreeMap<String, PathBuf>,
) -> Result<vibedoc_protocol::AnalyzeResult, CliError> {
    let command = find_adapter(name, command_overrides)?;
    let adapter_config = config.adapters.get(name).cloned().unwrap_or_default();
    let projects = if let Some(project) = project_overrides.get(name) {
        vec![normalize_path(&absolutize(cwd, project))]
    } else if !adapter_config.projects.is_empty() {
        adapter_config
            .projects
            .iter()
            .map(|project| normalize_path(&absolutize(root, Path::new(project))))
            .collect()
    } else if name == "typescript" {
        discover_project_config(cwd, root)
            .map(|project| vec![normalize_path(&project)])
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let mut client = AdapterClient::start(AdapterRunOptions {
        name: name.into(),
        command,
        workspace_root: root.to_path_buf(),
        timeout: Duration::from_secs(120),
    })?;
    let result = client.analyze(AnalyzeParams {
        workspace_root: normalize_path(root),
        projects,
        source_globs: adapter_config.sources,
    })?;
    client.shutdown()?;
    for diagnostic in &result.diagnostics {
        match diagnostic.severity {
            AdapterSeverity::Error => {
                return Err(CliError(format!(
                    "adapter `{name}` reported {}: {}",
                    diagnostic.code, diagnostic.message
                )));
            }
            AdapterSeverity::Warning | AdapterSeverity::Info => {
                eprintln!(
                    "adapter {name}: {:?} {}: {}",
                    diagnostic.severity, diagnostic.code, diagnostic.message
                );
            }
        }
    }
    Ok(result)
}

fn doctor(args: DoctorArgs) -> Result<u8, CliError> {
    let cwd = env::current_dir()?;
    let discovered = discover_config(&cwd, None)?;
    let root = discovered
        .as_ref()
        .map_or_else(|| cwd.clone(), |found| found.root.clone());
    let config = discovered
        .as_ref()
        .map_or_else(Config::default, |found| found.config.clone());
    let mut checks = Vec::new();
    checks.push(DoctorCheck::pass(
        "configuration",
        discovered.as_ref().map_or_else(
            || "No vibedoc.toml; configuration-free defaults are active.".into(),
            |found| format!("Loaded {}.", found.path.display()),
        ),
    ));
    match if discovered.is_some() {
        collect_documents(&root, &config)
    } else {
        collect_default_documents(&root)
    } {
        Ok(documents) => checks.push(DoctorCheck::pass(
            "documents",
            format!("Matched {} Markdown document(s).", documents.len()),
        )),
        Err(error) => checks.push(DoctorCheck::fail("documents", error.to_string())),
    }

    let mut adapters = config.adapters.keys().cloned().collect::<BTreeSet<_>>();
    for set in &config.documents {
        adapters.extend(set.adapters.iter().cloned());
    }
    if adapters.is_empty()
        && ["tsconfig.json", "jsconfig.json"]
            .iter()
            .any(|name| root.join(name).is_file())
    {
        adapters.insert("typescript".into());
    }
    for adapter in adapters {
        let empty = BTreeMap::new();
        match analyze_adapter(&adapter, &root, &cwd, &config, &empty, &empty) {
            Ok(result) => checks.push(DoctorCheck::pass(
                format!("adapter:{adapter}"),
                format!(
                    "Loaded {} source file(s) and {} symbol(s).",
                    result.graph.files.len(),
                    result.graph.symbols.len()
                ),
            )),
            Err(error) => checks.push(DoctorCheck::fail(format!("adapter:{adapter}"), error.0)),
        }
    }
    let failed = checks.iter().any(|check| !check.ok);
    let report = DoctorReport {
        schema_version: 1,
        tool: tool_info(),
        status: if failed { "fail" } else { "pass" },
        checks,
    };
    match args.format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        OutputFormat::Text => {
            for check in &report.checks {
                println!(
                    "{} {}: {}",
                    if check.ok { "ok" } else { "error" },
                    check.name,
                    check.message
                );
            }
        }
    }
    Ok(if failed { 2 } else { 0 })
}

fn inspect(args: InspectArgs) -> Result<u8, CliError> {
    let cwd = env::current_dir()?;
    let discovered = discover_config(&cwd, None)?;
    let root = discovered
        .as_ref()
        .map_or_else(|| cwd.clone(), |found| found.root.clone());
    let config = discovered
        .as_ref()
        .map_or_else(Config::default, |found| found.config.clone());
    let empty = BTreeMap::new();
    let mut result = analyze_adapter(&args.adapter, &root, &cwd, &config, &empty, &empty)?;
    if let Some(query) = args.symbol {
        result.graph.symbols.retain(|symbol| {
            symbol.id.contains(&query)
                || symbol.name.contains(&query)
                || symbol.qualified_name.contains(&query)
        });
        let ids = result
            .graph
            .symbols
            .iter()
            .map(|symbol| symbol.id.as_str())
            .collect::<BTreeSet<_>>();
        result.graph.relationships.retain(|relationship| {
            ids.contains(relationship.from.as_str())
                || relationship
                    .to
                    .as_deref()
                    .is_some_and(|id| ids.contains(id))
        });
    }
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&InspectReport {
                schema_version: 1,
                tool: tool_info(),
                status: "pass",
                analysis: result,
            })?
        ),
        OutputFormat::Text => {
            for symbol in &result.graph.symbols {
                println!(
                    "{} {:?} {} {}:{}:{}",
                    if symbol.exported { "export" } else { "local" },
                    symbol.kind,
                    symbol.qualified_name,
                    symbol.declaration.path,
                    symbol.declaration.range.start.line,
                    symbol.declaration.range.start.column
                );
                for signature in &symbol.signatures {
                    let parameters = signature
                        .parameters
                        .iter()
                        .map(|parameter| {
                            format!("{}: {}", parameter.name, parameter.type_fact.display)
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    println!("  ({parameters}) -> {}", signature.return_type.display);
                }
            }
            println!(
                "{} file(s), {} symbol(s), {} relationship(s)",
                result.graph.files.len(),
                result.graph.symbols.len(),
                result.graph.relationships.len()
            );
        }
    }
    Ok(0)
}

fn explain(args: ExplainArgs) -> Result<u8, CliError> {
    let id = args.rule_id.to_ascii_uppercase();
    let rule = rule_metadata(&id).ok_or_else(|| CliError(format!("unknown rule `{id}`")))?;
    match args.format {
        OutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&ExplainReport {
                schema_version: 1,
                tool: tool_info(),
                status: "pass",
                rule,
            })?
        ),
        OutputFormat::Text => {
            println!("{} — {}", rule.id, rule.title);
            println!("Severity: {:?}", rule.default_severity);
            println!("Experimental: {}", rule.experimental);
            println!("\n{}", rule.rationale);
            println!("\nBad: {}", rule.bad_example);
            println!("Good: {}", rule.good_example);
            println!("Fix: {}", rule.remediation);
        }
    }
    Ok(0)
}

fn parse_assignments(
    values: &[String],
    require_absolute: bool,
) -> Result<BTreeMap<String, PathBuf>, CliError> {
    let mut output = BTreeMap::new();
    for value in values {
        let (name, path) = value
            .split_once('=')
            .ok_or_else(|| CliError(format!("expected ADAPTER=PATH, got `{value}`")))?;
        if name.is_empty() || path.is_empty() {
            return Err(CliError(format!("expected ADAPTER=PATH, got `{value}`")));
        }
        let path = PathBuf::from(path);
        if require_absolute && !path.is_absolute() {
            return Err(CliError(format!(
                "adapter command for `{name}` must be an absolute path"
            )));
        }
        output.insert(name.to_string(), path);
    }
    Ok(output)
}

fn render_report(report: &OutputReport, format: OutputFormat) -> Result<(), CliError> {
    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(report)?),
        OutputFormat::Text => {
            for diagnostic in &report.diagnostics {
                render_diagnostic(diagnostic);
            }
            println!(
                "{} error(s), {} warning(s), {} document(s)",
                report.summary.errors, report.summary.warnings, report.summary.documents
            );
            println!(
                "Structural verification: {} verified, {} contradicted, {} unverified.",
                report.verification.verified_structural_claims,
                report.verification.contradicted_structural_claims,
                report.verification.unverified_structural_claims
            );
            if !report.verification.free_form_prose_evaluated {
                println!("Free-form prose was not factually evaluated.");
            }
        }
    }
    Ok(())
}

fn render_diagnostic(diagnostic: &Diagnostic) {
    println!(
        "{}:{}:{}: {} {}: {}",
        diagnostic.document.path,
        diagnostic.document.range.start.line,
        diagnostic.document.range.start.column,
        match diagnostic.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        },
        diagnostic.rule_id,
        diagnostic.message
    );
    for evidence in &diagnostic.evidence {
        println!(
            "  evidence: {}:{}:{}",
            evidence.path, evidence.range.start.line, evidence.range.start.column
        );
    }
    if let Some(suggestion) = &diagnostic.suggestion {
        println!("  help: {suggestion}");
    }
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorReport {
    schema_version: u32,
    tool: CliToolInfo,
    status: &'static str,
    checks: Vec<DoctorCheck>,
}

#[derive(Serialize)]
struct CliToolInfo {
    name: &'static str,
    version: &'static str,
}

fn tool_info() -> CliToolInfo {
    CliToolInfo {
        name: "vibedoc",
        version: VERSION,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectReport {
    schema_version: u32,
    tool: CliToolInfo,
    status: &'static str,
    analysis: vibedoc_protocol::AnalyzeResult,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExplainReport {
    schema_version: u32,
    tool: CliToolInfo,
    status: &'static str,
    rule: vibedoc_core::RuleMetadata,
}

#[derive(Serialize)]
struct DoctorCheck {
    name: String,
    ok: bool,
    message: String,
}

impl DoctorCheck {
    fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ok: true,
            message: message.into(),
        }
    }

    fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ok: false,
            message: message.into(),
        }
    }
}
