use crate::config::{Config, DocumentProfile};
use crate::diagnostic::{Diagnostic, Severity, VerificationSummary};
use crate::document::{BindingDirective, BindingScope, Document, ReferenceClaims};
use regex::Regex;
use serde::Serialize;
use std::collections::BTreeSet;
use std::ops::Range;
use vibedoc_protocol::{Confidence, FactGraph, RelationshipKind, SourceLocation, Symbol};

#[derive(Debug, Clone, Copy, Default)]
pub struct CheckOptions {
    pub experimental: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    pub verification: VerificationSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleMetadata {
    pub id: &'static str,
    pub title: &'static str,
    pub default_severity: Severity,
    pub experimental: bool,
    pub rationale: &'static str,
    pub bad_example: &'static str,
    pub good_example: &'static str,
    pub remediation: &'static str,
}

pub fn all_rule_metadata() -> Vec<RuleMetadata> {
    vec![
        metadata(
            "VDOC-L001",
            "Long sentence",
            Severity::Warning,
            false,
            "Long sentences make technical claims harder to scan and verify.",
            "The service validates the request and creates a user and then sends a message after it stores all of the supplied account values in the database.",
            "The service validates the request. The service creates and stores the user.",
            "Split the sentence so that each sentence states one primary fact.",
        ),
        metadata(
            "VDOC-L002",
            "Vague wording",
            Severity::Warning,
            false,
            "Vague verbs and nouns conceal the operation performed by software.",
            "The service handles authentication stuff.",
            "`AuthenticationService` validates the credentials.",
            "Replace the flagged wording with the observable operation or technical noun.",
        ),
        metadata(
            "VDOC-L003",
            "Qualitative claim",
            Severity::Warning,
            false,
            "Qualitative claims require evidence and are often not established by source code.",
            "The function securely and efficiently stores the token.",
            "The function stores the token in `TokenRepository`.",
            "Remove the qualifier or cite evidence that establishes the property.",
        ),
        metadata(
            "VDOC-T001",
            "Forbidden terminology",
            Severity::Warning,
            false,
            "One canonical term per concept prevents terminology drift.",
            "The auth token expires.",
            "The access token expires.",
            "Use the configured preferred term.",
        ),
        metadata(
            "VDOC-D001",
            "Skipped heading level",
            Severity::Warning,
            false,
            "A consistent heading hierarchy makes information easier to find.",
            "# API followed by ### Parameters",
            "# API followed by ## Parameters",
            "Change the heading to the next level in the hierarchy.",
        ),
        metadata(
            "VDOC-D002",
            "Vague link text",
            Severity::Warning,
            false,
            "A link label must identify its destination without surrounding context.",
            "Read more [here](configuration.md).",
            "Read the [configuration guide](configuration.md).",
            "Replace the generic label with a description of the destination.",
        ),
        metadata(
            "VDOC-G001",
            "Invalid source binding",
            Severity::Error,
            false,
            "An explicit source binding must resolve to exactly one source symbol.",
            "<!-- vibedoc:source adapter=\"typescript\" -->",
            "<!-- vibedoc:source adapter=\"typescript\" path=\"src/auth.ts\" symbol=\"login\" -->",
            "Correct the directive attributes, path, or qualified symbol name.",
        ),
        metadata(
            "VDOC-G002",
            "Ambiguous automatic binding",
            Severity::Warning,
            false,
            "An automatic binding cannot establish evidence when several symbols have the same name.",
            "## `parse`",
            "Use an explicit source directive before the heading.",
            "Add an explicit source directive with the source path and qualified symbol.",
        ),
        metadata(
            "VDOC-G003",
            "Unknown documented symbol",
            Severity::Error,
            false,
            "A structural code identifier must exist in the bound source symbol.",
            "- `userName`: The name.",
            "- `username`: The name.",
            "Use the identifier exactly as it appears in the source.",
        ),
        metadata(
            "VDOC-G004",
            "Missing parameter documentation",
            Severity::Warning,
            false,
            "Reference documentation should identify each inspectable parameter.",
            "A Parameters section that omits `role`.",
            "- `role` (`UserRole`): The role assigned to the user.",
            "Add a parameter item or disable this rule for intentionally partial documentation.",
        ),
        metadata(
            "VDOC-G005",
            "Parameter type mismatch",
            Severity::Error,
            false,
            "A documented parameter type must match the compiler fact when the fact is exact.",
            "- `email` (`number`): The email address.",
            "- `email` (`string`): The email address.",
            "Use the type displayed by `vibedoc inspect`.",
        ),
        metadata(
            "VDOC-G006",
            "Return type mismatch",
            Severity::Error,
            false,
            "A documented return type must match the compiler fact when the fact is exact.",
            "Returns `User` for a function typed as `Promise<User>`.",
            "Returns `Promise<User>`.",
            "Use the return type displayed by `vibedoc inspect`.",
        ),
        metadata(
            "VDOC-G007",
            "Unverified documented error",
            Severity::Warning,
            false,
            "Static analysis did not find a direct throw that supports the documented error.",
            "Lists `NetworkError` without a direct throw.",
            "Lists an error that is directly thrown, or explains external evidence.",
            "Review the implementation or treat the claim as externally grounded.",
        ),
        metadata(
            "VDOC-G008",
            "Insufficient compiler evidence",
            Severity::Warning,
            false,
            "Overloads, dynamic JavaScript, destructuring, and incomplete types can prevent safe comparison.",
            "Treating an inferred `any` type as verified.",
            "The checker reports the structural claim as unverified.",
            "Add explicit source types or accept that this claim is not statically verified.",
        ),
        metadata(
            "VDOC-G009",
            "Unknown automatic source symbol",
            Severity::Error,
            false,
            "A reference heading that names a source symbol must resolve to compiler evidence.",
            "## `missingSymbol` when no source symbol has that name.",
            "Use an existing symbol name or add an explicit source directive.",
            "Correct the symbol name, select the correct project, or add an explicit source directive.",
        ),
        metadata(
            "VDOC-X001",
            "Experimental return or throw claim",
            Severity::Warning,
            true,
            "A prose return or throw claim conflicts with direct compiler evidence.",
            "`login` returns `User` when it returns `Promise<User>`.",
            "`login` returns `Promise<User>`.",
            "Revise the controlled prose pattern or inspect the source fact.",
        ),
        metadata(
            "VDOC-X002",
            "Experimental operation claim",
            Severity::Warning,
            true,
            "A prose operation claim has no matching direct relationship in the fact graph.",
            "`login` calls `audit` when no direct call is present.",
            "Describe only the directly observed call or side effect.",
            "Revise the claim or keep it outside the experimental controlled pattern.",
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn metadata(
    id: &'static str,
    title: &'static str,
    default_severity: Severity,
    experimental: bool,
    rationale: &'static str,
    bad_example: &'static str,
    good_example: &'static str,
    remediation: &'static str,
) -> RuleMetadata {
    RuleMetadata {
        id,
        title,
        default_severity,
        experimental,
        rationale,
        bad_example,
        good_example,
        remediation,
    }
}

pub fn rule_metadata(id: &str) -> Option<RuleMetadata> {
    all_rule_metadata().into_iter().find(|rule| rule.id == id)
}

pub fn check_documents(
    documents: &[Document],
    graph: &FactGraph,
    config: &Config,
    options: CheckOptions,
) -> CheckResult {
    let mut result = CheckResult::default();
    for document in documents {
        check_language(document, config, &mut result.diagnostics);
        check_structure(document, config, &mut result.diagnostics);
        check_grounding(document, graph, config, options, &mut result);
    }
    result.diagnostics.sort_by(|left, right| {
        (
            &left.document.path,
            left.document.range.start.line,
            left.document.range.start.column,
            &left.rule_id,
        )
            .cmp(&(
                &right.document.path,
                right.document.range.start.line,
                right.document.range.start.column,
                &right.rule_id,
            ))
    });
    result
}

fn check_language(document: &Document, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    let sentence_re = Regex::new(r#"[^.!?]+(?:[.!?]+|$)"#).unwrap();
    let word_re = Regex::new(r#"[\p{L}\p{N}][\p{L}\p{N}'_-]*"#).unwrap();
    for span in &document.prose {
        for sentence in sentence_re.find_iter(&span.text) {
            let count = word_re.find_iter(sentence.as_str()).count();
            if count > 25 {
                push(
                    diagnostics,
                    config,
                    "VDOC-L001",
                    format!("Sentence has {count} words; the maximum is 25."),
                    document,
                    (span.bytes.start + sentence.start())..(span.bytes.start + sentence.end()),
                    Vec::new(),
                    Some("Split the sentence into shorter factual statements.".into()),
                    false,
                );
            }
        }
    }

    for (word, replacement) in [
        ("handles", "state the exact operation"),
        ("manages", "state the exact operation"),
        ("deals with", "state the exact operation"),
        ("stuff", "use a technical noun"),
        ("things", "use a technical noun"),
    ] {
        check_phrase(
            document,
            config,
            diagnostics,
            "VDOC-L002",
            word,
            format!("Vague wording: `{word}`."),
            Some(format!("Replace `{word}`; {replacement}.")),
        );
    }
    for word in [
        "easy",
        "simple",
        "secure",
        "securely",
        "robust",
        "fast",
        "efficient",
        "efficiently",
        "optimized",
        "seamless",
    ] {
        check_phrase(
            document,
            config,
            diagnostics,
            "VDOC-L003",
            word,
            format!("Qualitative claim requires evidence: `{word}`."),
            Some(format!(
                "Remove `{word}` or cite evidence for the property."
            )),
        );
    }
    for (preferred, term) in &config.terms {
        for forbidden in &term.forbidden {
            check_phrase(
                document,
                config,
                diagnostics,
                "VDOC-T001",
                forbidden,
                format!("Use `{preferred}` instead of `{forbidden}`."),
                Some(format!("Replace `{forbidden}` with `{preferred}`.")),
            );
        }
    }
}

fn check_phrase(
    document: &Document,
    config: &Config,
    diagnostics: &mut Vec<Diagnostic>,
    rule_id: &str,
    phrase: &str,
    message: String,
    suggestion: Option<String>,
) {
    let pattern = format!(
        r"(?i)(?P<before>^|[^\p{{L}}\p{{N}}_])(?P<term>{})(?P<after>$|[^\p{{L}}\p{{N}}_])",
        regex::escape(phrase)
    );
    let expression = Regex::new(&pattern).unwrap();
    for span in &document.prose {
        for captures in expression.captures_iter(&span.text) {
            let found = captures.name("term").unwrap();
            push(
                diagnostics,
                config,
                rule_id,
                message.clone(),
                document,
                (span.bytes.start + found.start())..(span.bytes.start + found.end()),
                Vec::new(),
                suggestion.clone(),
                false,
            );
        }
    }
}

fn check_structure(document: &Document, config: &Config, diagnostics: &mut Vec<Diagnostic>) {
    for pair in document.headings.windows(2) {
        if pair[1].level > pair[0].level + 1 {
            push(
                diagnostics,
                config,
                "VDOC-D001",
                format!(
                    "Heading level {} follows heading level {}.",
                    pair[1].level, pair[0].level
                ),
                document,
                pair[1].bytes.clone(),
                Vec::new(),
                Some(format!("Use heading level {}.", pair[0].level + 1)),
                false,
            );
        }
    }
    for link in &document.links {
        if matches!(
            link.text.trim().to_ascii_lowercase().as_str(),
            "here" | "click here" | "this link" | "more" | "learn more"
        ) {
            push(
                diagnostics,
                config,
                "VDOC-D002",
                format!(
                    "Link text `{}` does not identify its destination.",
                    link.text.trim()
                ),
                document,
                link.bytes.clone(),
                Vec::new(),
                Some("Describe the destination in the link text.".into()),
                false,
            );
        }
    }
}

fn check_grounding(
    document: &Document,
    graph: &FactGraph,
    config: &Config,
    options: CheckOptions,
    result: &mut CheckResult,
) {
    let mut covered_headings = BTreeSet::new();
    for scope in document.binding_scopes() {
        for (index, heading) in document.headings.iter().enumerate() {
            if heading.bytes.start >= scope.bytes.start && heading.bytes.end <= scope.bytes.end {
                covered_headings.insert(index);
            }
        }
        match resolve_explicit(&scope.directive, graph) {
            Ok(symbol) => validate_scope(document, &scope, symbol, graph, config, options, result),
            Err(message) => push(
                &mut result.diagnostics,
                config,
                "VDOC-G001",
                message,
                document,
                scope.directive.bytes.clone(),
                Vec::new(),
                Some("Correct the source directive or inspect available symbols.".into()),
                false,
            ),
        }
    }

    if document.profile != DocumentProfile::Reference {
        return;
    }
    for (index, heading) in document.headings.iter().enumerate() {
        if covered_headings.contains(&index)
            || heading.code_spans.len() != 1
            || matches!(
                heading.text.trim().to_ascii_lowercase().as_str(),
                "parameters" | "returns" | "errors"
            )
        {
            continue;
        }
        let name = &heading.code_spans[0].text;
        let matches = graph
            .symbols
            .iter()
            .filter(|symbol| symbol.name == *name || symbol.qualified_name == *name)
            .collect::<Vec<_>>();
        if matches.len() > 1 {
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G002",
                format!("`{name}` matches {} source symbols.", matches.len()),
                document,
                heading.code_spans[0].bytes.clone(),
                matches
                    .iter()
                    .map(|symbol| symbol.declaration.clone())
                    .collect(),
                Some("Add an explicit `vibedoc:source` directive.".into()),
                false,
            );
        } else if let Some(symbol) = matches.first() {
            let end = document
                .headings
                .iter()
                .skip(index + 1)
                .find(|candidate| candidate.level <= heading.level)
                .map_or(document.source.len(), |candidate| candidate.bytes.start);
            let scope = BindingScope {
                directive: BindingDirective {
                    adapter: Some(symbol.adapter.clone()),
                    path: Some(symbol.declaration.path.clone()),
                    symbol: Some(symbol.qualified_name.clone()),
                    bytes: heading.code_spans[0].bytes.clone(),
                    error: None,
                },
                bytes: heading.bytes.start..end,
                heading_level: heading.level,
            };
            validate_scope(document, &scope, symbol, graph, config, options, result);
        } else {
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G009",
                format!("`{name}` does not match a source symbol."),
                document,
                heading.code_spans[0].bytes.clone(),
                Vec::new(),
                Some(
                    "Correct the symbol name or add an explicit `vibedoc:source` directive.".into(),
                ),
                false,
            );
        }
    }
}

fn resolve_explicit<'a>(
    directive: &BindingDirective,
    graph: &'a FactGraph,
) -> Result<&'a Symbol, String> {
    if let Some(error) = &directive.error {
        return Err(format!("Invalid source directive: {error}."));
    }
    let adapter = directive.adapter.as_deref().unwrap();
    let path = normalize_source_path(directive.path.as_deref().unwrap());
    let name = directive.symbol.as_deref().unwrap();
    let matches = graph
        .symbols
        .iter()
        .filter(|symbol| {
            symbol.adapter == adapter
                && normalize_source_path(&symbol.declaration.path) == path
                && (symbol.name == name || symbol.qualified_name == name)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [symbol] => Ok(*symbol),
        [] => Err(format!(
            "Source binding `{adapter}:{path}#{name}` does not match a source symbol."
        )),
        _ => Err(format!(
            "Source binding `{adapter}:{path}#{name}` matches {} symbols.",
            matches.len()
        )),
    }
}

fn validate_scope(
    document: &Document,
    scope: &BindingScope,
    symbol: &Symbol,
    graph: &FactGraph,
    config: &Config,
    options: CheckOptions,
    result: &mut CheckResult,
) {
    let claims = document.reference_claims(scope);
    if document.profile == DocumentProfile::Reference {
        validate_reference_claims(document, symbol, &claims, config, result);
    }
    if options.experimental || config.experimental.prose_grounding {
        check_experimental(document, scope, symbol, graph, config, result);
        result.verification.free_form_prose_evaluated = true;
    }
}

fn validate_reference_claims(
    document: &Document,
    symbol: &Symbol,
    claims: &ReferenceClaims,
    config: &Config,
    result: &mut CheckResult,
) {
    if symbol.signatures.len() != 1 {
        let count =
            claims.parameters.len() + claims.return_type.iter().count() + claims.errors.len();
        result.verification.unverified_structural_claims += count;
        if count > 0 {
            let location = claims
                .parameters
                .first()
                .map(|claim| claim.bytes.clone())
                .or_else(|| claims.return_type.as_ref().map(|claim| claim.bytes.clone()))
                .unwrap_or(0..0);
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G008",
                format!(
                    "`{}` has {} signatures; structural claims were not compared.",
                    symbol.qualified_name,
                    symbol.signatures.len()
                ),
                document,
                location,
                vec![symbol.declaration.clone()],
                None,
                false,
            );
        }
        return;
    }
    let signature = &symbol.signatures[0];
    let documented_names = claims
        .parameters
        .iter()
        .map(|parameter| parameter.name.as_str())
        .collect::<BTreeSet<_>>();
    for claim in &claims.parameters {
        let Some(parameter) = signature
            .parameters
            .iter()
            .find(|parameter| parameter.name == claim.name)
        else {
            if signature
                .parameters
                .iter()
                .any(|parameter| parameter.destructured)
            {
                result.verification.unverified_structural_claims += 1;
                push(
                    &mut result.diagnostics,
                    config,
                    "VDOC-G008",
                    format!(
                        "`{}` was not compared because the source signature contains a destructured parameter.",
                        claim.name
                    ),
                    document,
                    claim.bytes.clone(),
                    vec![signature.declaration.clone()],
                    None,
                    false,
                );
                continue;
            }
            result.verification.contradicted_structural_claims += 1;
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G003",
                format!(
                    "`{}` is not a parameter of `{}`.",
                    claim.name, symbol.qualified_name
                ),
                document,
                claim.bytes.clone(),
                vec![signature.declaration.clone()],
                Some("Use a parameter name from the source signature.".into()),
                false,
            );
            continue;
        };
        result.verification.verified_structural_claims += 1;
        if let Some(documented_type) = &claim.type_name {
            if parameter.destructured
                || parameter.type_fact.confidence == Confidence::Incomplete
                || matches!(parameter.type_fact.normalized.as_str(), "any" | "unknown")
            {
                result.verification.unverified_structural_claims += 1;
                push(
                    &mut result.diagnostics,
                    config,
                    "VDOC-G008",
                    format!(
                        "The type of `{}` is not precise enough to compare.",
                        claim.name
                    ),
                    document,
                    claim.bytes.clone(),
                    vec![parameter.location.clone()],
                    None,
                    false,
                );
            } else if normalize_type(documented_type)
                != normalize_type(&parameter.type_fact.normalized)
                && normalize_type(documented_type) != normalize_type(&parameter.type_fact.display)
            {
                result.verification.contradicted_structural_claims += 1;
                push(
                    &mut result.diagnostics,
                    config,
                    "VDOC-G005",
                    format!(
                        "Parameter `{}` is documented as `{documented_type}` but the source type is `{}`.",
                        claim.name, parameter.type_fact.display
                    ),
                    document,
                    claim.bytes.clone(),
                    vec![parameter.location.clone()],
                    Some(format!("Use `{}`.", parameter.type_fact.display)),
                    false,
                );
            } else {
                result.verification.verified_structural_claims += 1;
            }
        }
    }
    for parameter in &signature.parameters {
        if !parameter.destructured && !documented_names.contains(parameter.name.as_str()) {
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G004",
                format!("Parameter `{}` is not documented.", parameter.name),
                document,
                document
                    .headings
                    .iter()
                    .find(|heading| heading.text.eq_ignore_ascii_case("parameters"))
                    .map_or(0..0, |heading| heading.bytes.clone()),
                vec![parameter.location.clone()],
                Some(format!("Add a Parameters item for `{}`.", parameter.name)),
                false,
            );
        }
    }

    if let Some(claim) = &claims.return_type {
        if signature.return_type.confidence == Confidence::Incomplete
            || matches!(signature.return_type.normalized.as_str(), "any" | "unknown")
        {
            result.verification.unverified_structural_claims += 1;
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G008",
                "The return type is not precise enough to compare.".into(),
                document,
                claim.bytes.clone(),
                vec![signature.declaration.clone()],
                None,
                false,
            );
        } else if normalize_type(&claim.value) != normalize_type(&signature.return_type.normalized)
            && normalize_type(&claim.value) != normalize_type(&signature.return_type.display)
        {
            result.verification.contradicted_structural_claims += 1;
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G006",
                format!(
                    "Return type is documented as `{}` but the source type is `{}`.",
                    claim.value, signature.return_type.display
                ),
                document,
                claim.bytes.clone(),
                vec![signature.declaration.clone()],
                Some(format!("Use `{}`.", signature.return_type.display)),
                false,
            );
        } else {
            result.verification.verified_structural_claims += 1;
        }
    }

    for claim in &claims.errors {
        if symbol.throws.iter().any(|observed| {
            observed.type_name == claim.value && observed.confidence != Confidence::Incomplete
        }) {
            result.verification.verified_structural_claims += 1;
        } else {
            result.verification.unverified_structural_claims += 1;
            push(
                &mut result.diagnostics,
                config,
                "VDOC-G007",
                format!(
                    "No direct throw of `{}` was observed in `{}`.",
                    claim.value, symbol.qualified_name
                ),
                document,
                claim.bytes.clone(),
                vec![symbol.declaration.clone()],
                None,
                false,
            );
        }
    }
}

fn check_experimental(
    document: &Document,
    scope: &BindingScope,
    symbol: &Symbol,
    graph: &FactGraph,
    config: &Config,
    result: &mut CheckResult,
) {
    let text = &document.source[scope.bytes.clone()];
    let return_or_throw = Regex::new(r#"(?i)`([^`]+)`\s+(returns|throws)\s+`([^`]+)`"#).unwrap();
    for captures in return_or_throw.captures_iter(text) {
        let full = captures.get(0).unwrap();
        let absolute = (scope.bytes.start + full.start())..(scope.bytes.start + full.end());
        if in_fenced_code(&document.source, absolute.start) {
            continue;
        }
        if captures.get(1).unwrap().as_str() != symbol.name
            && captures.get(1).unwrap().as_str() != symbol.qualified_name
        {
            continue;
        }
        let predicate = captures.get(2).unwrap().as_str().to_ascii_lowercase();
        let object = captures.get(3).unwrap().as_str();
        let supported = if predicate == "returns" {
            symbol.signatures.iter().any(|signature| {
                signature.return_type.confidence != Confidence::Incomplete
                    && (normalize_type(object) == normalize_type(&signature.return_type.normalized)
                        || normalize_type(object) == normalize_type(&signature.return_type.display))
            })
        } else {
            symbol.throws.iter().any(|error| {
                error.type_name == object && error.confidence != Confidence::Incomplete
            })
        };
        if !supported {
            push(
                &mut result.diagnostics,
                config,
                "VDOC-X001",
                format!(
                    "Experimental check found no direct evidence that `{}` {predicate} `{object}`.",
                    symbol.qualified_name
                ),
                document,
                absolute,
                vec![symbol.declaration.clone()],
                None,
                true,
            );
        }
    }

    let operation =
        Regex::new(r#"(?i)`([^`]+)`\s+(calls|reads|writes|stores|updates|deletes)\s+`([^`]+)`"#)
            .unwrap();
    for captures in operation.captures_iter(text) {
        let full = captures.get(0).unwrap();
        let absolute = (scope.bytes.start + full.start())..(scope.bytes.start + full.end());
        if in_fenced_code(&document.source, absolute.start) {
            continue;
        }
        if captures.get(1).unwrap().as_str() != symbol.name
            && captures.get(1).unwrap().as_str() != symbol.qualified_name
        {
            continue;
        }
        let predicate = captures.get(2).unwrap().as_str().to_ascii_lowercase();
        let object = captures.get(3).unwrap().as_str();
        let expected_kind = if predicate == "reads" {
            RelationshipKind::Reads
        } else if predicate == "calls" {
            RelationshipKind::Calls
        } else {
            RelationshipKind::Writes
        };
        let supported = graph.relationships.iter().any(|relationship| {
            relationship.from == symbol.id
                && relationship.kind == expected_kind
                && (relationship.display == object
                    || relationship
                        .to
                        .as_deref()
                        .is_some_and(|target| target.ends_with(object)))
        });
        if !supported {
            push(
                &mut result.diagnostics,
                config,
                "VDOC-X002",
                format!(
                    "Experimental check found no direct evidence that `{}` {predicate} `{object}`.",
                    symbol.qualified_name
                ),
                document,
                absolute,
                vec![symbol.declaration.clone()],
                None,
                true,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn push(
    diagnostics: &mut Vec<Diagnostic>,
    config: &Config,
    rule_id: &str,
    message: String,
    document: &Document,
    bytes: Range<usize>,
    evidence: Vec<SourceLocation>,
    suggestion: Option<String>,
    experimental: bool,
) {
    let Some(severity) = configured_severity(config, rule_id) else {
        return;
    };
    diagnostics.push(Diagnostic {
        rule_id: rule_id.to_string(),
        severity,
        message,
        document: document.location(bytes),
        evidence,
        suggestion,
        experimental,
    });
}

fn configured_severity(config: &Config, rule_id: &str) -> Option<Severity> {
    let configured = if let Some(level) = config.rules.get(rule_id) {
        level.severity()
    } else {
        rule_metadata(rule_id).map(|metadata| metadata.default_severity)
    };
    if rule_id.starts_with("VDOC-X") && configured == Some(Severity::Error) {
        Some(Severity::Warning)
    } else {
        configured
    }
}

fn normalize_source_path(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

fn normalize_type(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn in_fenced_code(source: &str, byte: usize) -> bool {
    let mut fence: Option<&str> = None;
    let mut cursor = 0usize;
    for line in source.split_inclusive('\n') {
        if cursor > byte {
            break;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            fence = if fence == Some("```") {
                None
            } else if fence.is_none() {
                Some("```")
            } else {
                fence
            };
        } else if trimmed.starts_with("~~~") {
            fence = if fence == Some("~~~") {
                None
            } else if fence.is_none() {
                Some("~~~")
            } else {
                fence
            };
        }
        if byte < cursor + line.len() {
            return fence.is_some();
        }
        cursor += line.len();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{RuleLevel, TermConfig};
    use crate::document::parse_document;
    use vibedoc_protocol::{Position, Signature, SourceRange, SymbolKind, TypeFact};

    fn location(path: &str) -> SourceLocation {
        SourceLocation {
            path: path.into(),
            range: SourceRange {
                start: Position { line: 1, column: 1 },
                end: Position { line: 1, column: 5 },
            },
        }
    }

    fn symbol(
        id: &str,
        path: &str,
        name: &str,
        qualified_name: &str,
        signatures: Vec<Signature>,
    ) -> Symbol {
        Symbol {
            id: id.into(),
            adapter: "typescript".into(),
            language: "typescript".into(),
            name: name.into(),
            qualified_name: qualified_name.into(),
            kind: SymbolKind::Function,
            exported: true,
            declaration: location(path),
            signatures,
            throws: Vec::new(),
            confidence: Confidence::Exact,
        }
    }

    #[test]
    fn reports_language_and_structural_contradictions() {
        let source = r#"<!-- vibedoc:source adapter="typescript" path="src/auth.ts" symbol="login" -->
# `login`

This simple function handles all of the authentication stuff for every user in the application and it efficiently performs every required operation without additional configuration for all callers today.

## Parameters

- `email` (`number`): The email.
- `missing` (`string`): Missing.

## Returns

`User`
"#;
        let document = parse_document(
            "/tmp/auth.md",
            "docs/auth.md",
            DocumentProfile::Reference,
            source.into(),
        );
        let declaration = location("src/auth.ts");
        let graph = FactGraph {
            files: vec![],
            symbols: vec![Symbol {
                id: "typescript:src/auth.ts#login".into(),
                adapter: "typescript".into(),
                language: "typescript".into(),
                name: "login".into(),
                qualified_name: "login".into(),
                kind: SymbolKind::Function,
                exported: true,
                declaration: declaration.clone(),
                signatures: vec![Signature {
                    parameters: vec![vibedoc_protocol::Parameter {
                        name: "email".into(),
                        type_fact: TypeFact {
                            display: "string".into(),
                            normalized: "string".into(),
                            confidence: Confidence::Exact,
                        },
                        optional: false,
                        rest: false,
                        destructured: false,
                        location: declaration.clone(),
                    }],
                    return_type: TypeFact {
                        display: "Promise<User>".into(),
                        normalized: "Promise<User>".into(),
                        confidence: Confidence::Exact,
                    },
                    declaration,
                }],
                throws: vec![],
                confidence: Confidence::Exact,
            }],
            relationships: vec![],
        };
        let result = check_documents(
            &[document],
            &graph,
            &Config::default(),
            CheckOptions::default(),
        );
        let ids = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rule_id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(ids.contains("VDOC-L001"));
        assert!(ids.contains("VDOC-L002"));
        assert!(ids.contains("VDOC-L003"));
        assert!(ids.contains("VDOC-G003"));
        assert!(ids.contains("VDOC-G005"));
        assert!(ids.contains("VDOC-G006"));
    }

    #[test]
    fn severity_can_be_disabled() {
        let mut config = Config::default();
        config.rules.insert("VDOC-L002".into(), RuleLevel::Off);
        let document = parse_document(
            "/tmp/a.md",
            "a.md",
            DocumentProfile::Guide,
            "It handles things.".into(),
        );
        let result = check_documents(
            &[document],
            &FactGraph::default(),
            &config,
            CheckOptions::default(),
        );
        assert!(
            result
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.rule_id != "VDOC-L002")
        );
    }

    #[test]
    fn reports_terminology_and_document_structure_rules() {
        let mut config = Config::default();
        config.terms.insert(
            "access token".into(),
            TermConfig {
                forbidden: vec!["auth token".into()],
            },
        );
        let document = parse_document(
            "/tmp/a.md",
            "a.md",
            DocumentProfile::Guide,
            "# One\n\n### Three\n\nThe auth token expires. Read [here](target.md).\n".into(),
        );
        let result = check_documents(
            &[document],
            &FactGraph::default(),
            &config,
            CheckOptions::default(),
        );
        let ids = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rule_id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(ids.contains("VDOC-T001"));
        assert!(ids.contains("VDOC-D001"));
        assert!(ids.contains("VDOC-D002"));
    }

    #[test]
    fn reports_invalid_ambiguous_and_unknown_bindings() {
        let invalid = parse_document(
            "/tmp/invalid.md",
            "invalid.md",
            DocumentProfile::Reference,
            "<!-- vibedoc:source adapter=\"typescript\" -->\n# `login`\n".into(),
        );
        let invalid_result = check_documents(
            &[invalid],
            &FactGraph::default(),
            &Config::default(),
            CheckOptions::default(),
        );
        assert!(
            invalid_result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule_id == "VDOC-G001")
        );

        let ambiguous = parse_document(
            "/tmp/ambiguous.md",
            "ambiguous.md",
            DocumentProfile::Reference,
            "# `parse`\n".into(),
        );
        let graph = FactGraph {
            files: Vec::new(),
            symbols: vec![
                symbol(
                    "typescript:src/a.ts#parse",
                    "src/a.ts",
                    "parse",
                    "parse",
                    Vec::new(),
                ),
                symbol(
                    "typescript:src/b.ts#parse",
                    "src/b.ts",
                    "parse",
                    "parse",
                    Vec::new(),
                ),
            ],
            relationships: Vec::new(),
        };
        let ambiguous_result = check_documents(
            &[ambiguous],
            &graph,
            &Config::default(),
            CheckOptions::default(),
        );
        assert!(
            ambiguous_result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule_id == "VDOC-G002")
        );

        let unknown = parse_document(
            "/tmp/unknown.md",
            "unknown.md",
            DocumentProfile::Reference,
            "# `notPresent`\n".into(),
        );
        let unknown_result = check_documents(
            &[unknown],
            &FactGraph::default(),
            &Config::default(),
            CheckOptions::default(),
        );
        assert!(
            unknown_result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule_id == "VDOC-G009")
        );
    }

    #[test]
    fn automatically_binds_one_matching_reference_symbol() {
        let document = parse_document(
            "/tmp/login.md",
            "login.md",
            DocumentProfile::Reference,
            "# `login`\n\n## Returns\n\n`string`\n".into(),
        );
        let declaration = location("src/auth.ts");
        let graph = FactGraph {
            files: Vec::new(),
            symbols: vec![symbol(
                "typescript:src/auth.ts#login",
                "src/auth.ts",
                "login",
                "login",
                vec![Signature {
                    parameters: Vec::new(),
                    return_type: TypeFact {
                        display: "string".into(),
                        normalized: "string".into(),
                        confidence: Confidence::Exact,
                    },
                    declaration,
                }],
            )],
            relationships: Vec::new(),
        };
        let result = check_documents(
            &[document],
            &graph,
            &Config::default(),
            CheckOptions::default(),
        );
        assert!(result.diagnostics.is_empty());
        assert_eq!(result.verification.verified_structural_claims, 1);
    }

    #[test]
    fn reports_completeness_and_insufficient_evidence() {
        let document = parse_document(
            "/tmp/login.md",
            "login.md",
            DocumentProfile::Reference,
            r#"<!-- vibedoc:source adapter="typescript" path="src/auth.ts" symbol="login" -->
# `login`

## Parameters

- `email` (`string`): Email.

## Returns

`User`

## Errors

- `NetworkError`: The request failed.
"#
            .into(),
        );
        let declaration = location("src/auth.ts");
        let signature = Signature {
            parameters: vec![
                vibedoc_protocol::Parameter {
                    name: "email".into(),
                    type_fact: TypeFact {
                        display: "string".into(),
                        normalized: "string".into(),
                        confidence: Confidence::Exact,
                    },
                    optional: false,
                    rest: false,
                    destructured: false,
                    location: declaration.clone(),
                },
                vibedoc_protocol::Parameter {
                    name: "role".into(),
                    type_fact: TypeFact {
                        display: "any".into(),
                        normalized: "any".into(),
                        confidence: Confidence::Incomplete,
                    },
                    optional: false,
                    rest: false,
                    destructured: false,
                    location: declaration.clone(),
                },
            ],
            return_type: TypeFact {
                display: "any".into(),
                normalized: "any".into(),
                confidence: Confidence::Incomplete,
            },
            declaration,
        };
        let graph = FactGraph {
            files: Vec::new(),
            symbols: vec![symbol(
                "typescript:src/auth.ts#login",
                "src/auth.ts",
                "login",
                "login",
                vec![signature],
            )],
            relationships: Vec::new(),
        };
        let result = check_documents(
            &[document],
            &graph,
            &Config::default(),
            CheckOptions::default(),
        );
        let ids = result
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.rule_id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(ids.contains("VDOC-G004"));
        assert!(ids.contains("VDOC-G007"));
        assert!(ids.contains("VDOC-G008"));
        assert!(result.verification.unverified_structural_claims >= 2);
    }

    #[test]
    fn experimental_rules_are_warning_only() {
        let document = parse_document(
            "/tmp/login.md",
            "login.md",
            DocumentProfile::Reference,
            r#"<!-- vibedoc:source adapter="typescript" path="src/auth.ts" symbol="login" -->
# `login`

`login` returns `User`. `login` calls `missingOperation`.
"#
            .into(),
        );
        let declaration = location("src/auth.ts");
        let graph = FactGraph {
            files: Vec::new(),
            symbols: vec![symbol(
                "typescript:src/auth.ts#login",
                "src/auth.ts",
                "login",
                "login",
                vec![Signature {
                    parameters: Vec::new(),
                    return_type: TypeFact {
                        display: "string".into(),
                        normalized: "string".into(),
                        confidence: Confidence::Exact,
                    },
                    declaration,
                }],
            )],
            relationships: Vec::new(),
        };
        let mut config = Config::default();
        config.rules.insert("VDOC-X001".into(), RuleLevel::Error);
        config.rules.insert("VDOC-X002".into(), RuleLevel::Error);
        let result = check_documents(
            &[document],
            &graph,
            &config,
            CheckOptions { experimental: true },
        );
        for id in ["VDOC-X001", "VDOC-X002"] {
            let diagnostic = result
                .diagnostics
                .iter()
                .find(|diagnostic| diagnostic.rule_id == id)
                .expect("experimental diagnostic");
            assert_eq!(diagnostic.severity, Severity::Warning);
            assert!(diagnostic.experimental);
        }
        assert!(result.verification.free_form_prose_evaluated);
    }

    #[test]
    fn incomplete_named_types_and_throws_do_not_verify_or_contradict_claims() {
        let document = parse_document(
            "/tmp/broken.md",
            "broken.md",
            DocumentProfile::Reference,
            "# `broken`\n\n`broken` returns `MissingType`. `broken` throws `MissingError`.\n\n## Parameters\n\n- `value` (`string`): The input.\n\n## Returns\n\n`number`\n\n## Errors\n\n- `MissingError`: The operation failed.\n".into(),
        );
        let declaration = location("src/broken.ts");
        let incomplete = TypeFact {
            display: "MissingType".into(),
            normalized: "MissingType".into(),
            confidence: Confidence::Incomplete,
        };
        let mut broken = symbol(
            "typescript:src/broken.ts#broken",
            "src/broken.ts",
            "broken",
            "broken",
            vec![Signature {
                parameters: vec![vibedoc_protocol::Parameter {
                    name: "value".into(),
                    type_fact: incomplete.clone(),
                    optional: false,
                    rest: false,
                    destructured: false,
                    location: declaration.clone(),
                }],
                return_type: incomplete,
                declaration: declaration.clone(),
            }],
        );
        broken.throws.push(vibedoc_protocol::ThrowFact {
            type_name: "MissingError".into(),
            location: declaration,
            confidence: Confidence::Incomplete,
        });
        let result = check_documents(
            &[document],
            &FactGraph {
                symbols: vec![broken],
                ..FactGraph::default()
            },
            &Config::default(),
            CheckOptions { experimental: true },
        );
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|d| d.rule_id == "VDOC-X001")
                .count(),
            2
        );
        assert_eq!(result.verification.verified_structural_claims, 1); // Parameter name only.
        assert_eq!(result.verification.unverified_structural_claims, 3);
        assert_eq!(result.verification.contradicted_structural_claims, 0);
        assert_eq!(
            result
                .diagnostics
                .iter()
                .filter(|d| d.rule_id == "VDOC-G008")
                .count(),
            2
        );
        assert!(result.diagnostics.iter().any(|d| d.rule_id == "VDOC-G007"));
    }

    #[test]
    fn destructured_parameters_are_unverified_not_contradicted() {
        let document = parse_document(
            "/tmp/greeting.md",
            "greeting.md",
            DocumentProfile::Reference,
            r#"<!-- vibedoc:source adapter="typescript" path="src/component.tsx" symbol="Greeting" -->
# `Greeting`

## Parameters

- `name` (`string`): The displayed name.
"#
            .into(),
        );
        let declaration = location("src/component.tsx");
        let graph = FactGraph {
            files: Vec::new(),
            symbols: vec![symbol(
                "typescript:src/component.tsx#Greeting",
                "src/component.tsx",
                "Greeting",
                "Greeting",
                vec![Signature {
                    parameters: vec![vibedoc_protocol::Parameter {
                        name: "{ name }".into(),
                        type_fact: TypeFact {
                            display: "GreetingProps".into(),
                            normalized: "GreetingProps".into(),
                            confidence: Confidence::Exact,
                        },
                        optional: false,
                        rest: false,
                        destructured: true,
                        location: declaration.clone(),
                    }],
                    return_type: TypeFact {
                        display: "Element".into(),
                        normalized: "Element".into(),
                        confidence: Confidence::Inferred,
                    },
                    declaration,
                }],
            )],
            relationships: Vec::new(),
        };
        let result = check_documents(
            &[document],
            &graph,
            &Config::default(),
            CheckOptions::default(),
        );
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.rule_id == "VDOC-G008")
        );
        assert!(
            result
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.rule_id != "VDOC-G003")
        );
        assert_eq!(result.verification.contradicted_structural_claims, 0);
        assert_eq!(result.verification.unverified_structural_claims, 1);
    }
}
