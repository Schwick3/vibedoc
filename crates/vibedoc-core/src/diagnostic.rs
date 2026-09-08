use serde::{Deserialize, Serialize};
use vibedoc_protocol::SourceLocation;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub document: SourceLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<SourceLocation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub experimental: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct VerificationSummary {
    pub verified_structural_claims: usize,
    pub contradicted_structural_claims: usize,
    pub unverified_structural_claims: usize,
    pub free_form_prose_evaluated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OutputReport {
    pub schema_version: u32,
    pub tool: ToolInfo,
    pub status: ReportStatus,
    pub diagnostics: Vec<Diagnostic>,
    pub verification: VerificationSummary,
    pub summary: ReportSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReportSummary {
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub documents: usize,
}

impl OutputReport {
    pub fn new(
        version: impl Into<String>,
        mut diagnostics: Vec<Diagnostic>,
        verification: VerificationSummary,
        documents: usize,
        deny_warnings: bool,
    ) -> Self {
        diagnostics.sort_by(|left, right| {
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
        let summary = ReportSummary {
            errors: diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .count(),
            warnings: diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Warning)
                .count(),
            info: diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Info)
                .count(),
            documents,
        };
        let failed = summary.errors > 0 || (deny_warnings && summary.warnings > 0);
        Self {
            schema_version: 1,
            tool: ToolInfo {
                name: "vibedoc".into(),
                version: version.into(),
            },
            status: if failed {
                ReportStatus::Fail
            } else {
                ReportStatus::Pass
            },
            diagnostics,
            verification,
            summary,
        }
    }

    pub fn failed(&self) -> bool {
        self.status == ReportStatus::Fail
    }
}
