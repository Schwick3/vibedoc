use crate::config::DocumentProfile;
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::Regex;
use std::ops::Range;
use std::path::PathBuf;
use vibedoc_protocol::{Position, SourceLocation, SourceRange};

#[derive(Debug, Clone)]
pub struct Document {
    pub path: PathBuf,
    pub display_path: String,
    pub profile: DocumentProfile,
    pub source: String,
    pub prose: Vec<TextSpan>,
    pub code_spans: Vec<TextSpan>,
    pub typed_code_blocks: Vec<TextSpan>,
    pub headings: Vec<Heading>,
    pub links: Vec<TextSpan>,
    pub directives: Vec<BindingDirective>,
    line_starts: Vec<usize>,
}

#[derive(Debug, Clone)]
pub struct TextSpan {
    pub text: String,
    pub bytes: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub bytes: Range<usize>,
    pub code_spans: Vec<TextSpan>,
}

#[derive(Debug, Clone)]
pub struct BindingDirective {
    pub adapter: Option<String>,
    pub path: Option<String>,
    pub symbol: Option<String>,
    pub bytes: Range<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BindingScope {
    pub directive: BindingDirective,
    pub bytes: Range<usize>,
    pub heading_level: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ReferenceClaims {
    pub parameters: Vec<DocumentedParameter>,
    pub return_type: Option<DocumentedValue>,
    pub errors: Vec<DocumentedValue>,
}

#[derive(Debug, Clone)]
pub struct DocumentedParameter {
    pub name: String,
    pub type_name: Option<String>,
    pub bytes: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct DocumentedValue {
    pub value: String,
    pub bytes: Range<usize>,
}

pub fn parse_document(
    path: impl Into<PathBuf>,
    display_path: impl Into<String>,
    profile: DocumentProfile,
    source: String,
) -> Document {
    let path = path.into();
    let display_path = display_path.into();
    let line_starts = line_starts(&source);
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(&source, options).into_offset_iter();

    let mut prose = Vec::new();
    let mut code_spans = Vec::new();
    let mut headings = Vec::new();
    let mut links = Vec::new();
    let mut directives = Vec::new();
    let mut typed_code_blocks = Vec::new();
    let mut current_code_block: Option<TextSpan> = None;
    let mut code_block_depth = 0usize;
    let mut current_heading: Option<Heading> = None;
    let mut current_link: Option<TextSpan> = None;

    for (event, range) in parser {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                code_block_depth += 1;
                if matches!(kind, CodeBlockKind::Fenced(ref language) if matches!(language.as_ref(), "ts" | "typescript"))
                {
                    current_code_block = Some(TextSpan {
                        text: String::new(),
                        bytes: range.clone(),
                    });
                }
            }
            Event::Text(text) if code_block_depth > 0 => {
                if let Some(block) = current_code_block.as_mut() {
                    block.text.push_str(&text);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                code_block_depth = code_block_depth.saturating_sub(1);
                if let Some(mut block) = current_code_block.take() {
                    block.bytes.end = range.end;
                    typed_code_blocks.push(block);
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                current_heading = Some(Heading {
                    level: heading_level(level),
                    text: String::new(),
                    bytes: range.start..range.end,
                    code_spans: Vec::new(),
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(mut heading) = current_heading.take() {
                    heading.bytes.end = range.end;
                    heading.text = heading.text.trim().to_string();
                    headings.push(heading);
                }
            }
            Event::Start(Tag::Link { .. }) => {
                current_link = Some(TextSpan {
                    text: String::new(),
                    bytes: range.start..range.end,
                });
            }
            Event::End(TagEnd::Link) => {
                if let Some(mut link) = current_link.take() {
                    link.bytes.end = range.end;
                    links.push(link);
                }
            }
            Event::Text(text) if code_block_depth == 0 => {
                let span = TextSpan {
                    text: text.to_string(),
                    bytes: range.clone(),
                };
                if let Some(heading) = current_heading.as_mut() {
                    heading.text.push_str(&text);
                } else {
                    prose.push(span);
                }
                if let Some(link) = current_link.as_mut() {
                    link.text.push_str(&text);
                }
            }
            Event::Code(code) if code_block_depth == 0 => {
                let span = TextSpan {
                    text: code.to_string(),
                    bytes: range.clone(),
                };
                if let Some(heading) = current_heading.as_mut() {
                    heading.text.push_str(&code);
                    heading.code_spans.push(span.clone());
                }
                if let Some(link) = current_link.as_mut() {
                    link.text.push_str(&code);
                }
                code_spans.push(span);
            }
            Event::Html(html) | Event::InlineHtml(html) if code_block_depth == 0 => {
                directives.extend(parse_directives(&html, range.start));
            }
            _ => {}
        }
    }

    Document {
        path,
        display_path,
        profile,
        source,
        prose,
        code_spans,
        typed_code_blocks,
        headings,
        links,
        directives,
        line_starts,
    }
}

impl Document {
    pub fn location(&self, bytes: Range<usize>) -> SourceLocation {
        SourceLocation {
            path: self.display_path.clone(),
            range: SourceRange {
                start: self.position(bytes.start),
                end: self.position(bytes.end),
            },
        }
    }

    pub fn position(&self, byte: usize) -> Position {
        let byte = byte.min(self.source.len());
        let line_index = self.line_starts.partition_point(|start| *start <= byte) - 1;
        Position {
            line: (line_index + 1) as u32,
            column: (byte - self.line_starts[line_index] + 1) as u32,
        }
    }

    pub fn binding_scopes(&self) -> Vec<BindingScope> {
        let mut scopes = Vec::new();
        for directive in &self.directives {
            let first_heading = self.headings.first();
            if first_heading.is_some_and(|heading| directive.bytes.end <= heading.bytes.start) {
                scopes.push(BindingScope {
                    directive: directive.clone(),
                    bytes: 0..self.source.len(),
                    heading_level: 0,
                });
                continue;
            }
            if let Some((index, heading)) = self
                .headings
                .iter()
                .enumerate()
                .find(|(_, heading)| heading.bytes.start >= directive.bytes.end)
            {
                let end = self
                    .headings
                    .iter()
                    .skip(index + 1)
                    .find(|candidate| candidate.level <= heading.level)
                    .map_or(self.source.len(), |candidate| candidate.bytes.start);
                scopes.push(BindingScope {
                    directive: directive.clone(),
                    bytes: heading.bytes.start..end,
                    heading_level: heading.level,
                });
            } else {
                scopes.push(BindingScope {
                    directive: directive.clone(),
                    bytes: 0..self.source.len(),
                    heading_level: 0,
                });
            }
        }
        scopes
    }

    /// Recognize TypeDoc method/function sections only when a TS signature and
    /// a local source-path label accompany them. Link destinations are never fetched.
    pub fn typedoc_scopes(&self) -> Vec<BindingScope> {
        let method = Regex::new(r"^([A-Za-z_$][\w$]*(?:\.[A-Za-z_$][\w$]*)*)\(\)$").unwrap();
        let function =
            Regex::new(r"^\s*(?:declare\s+)?(?:async\s+)?function\s+([A-Za-z_$][\w$]*)\s*(?:<|\()")
                .unwrap();
        let mut scopes = Vec::new();
        for (index, heading) in self.headings.iter().enumerate() {
            let Some(name) = method.captures(&heading.text).map(|c| c[1].to_string()) else {
                continue;
            };
            let end = self
                .headings
                .iter()
                .skip(index + 1)
                .find(|h| h.level <= heading.level)
                .map_or(self.source.len(), |h| h.bytes.start);
            let preamble_end = self
                .headings
                .get(index + 1)
                .map_or(end, |h| h.bytes.start)
                .min(end);
            let Some(block) = self
                .typed_code_blocks
                .iter()
                .find(|b| b.bytes.start >= heading.bytes.end && b.bytes.end <= preamble_end)
            else {
                continue;
            };
            // Reject examples or signatures naming a different callable.
            let signature_name =
                Regex::new(&format!(r"^\s*{}\s*(?:<|\()", regex::escape(&name))).unwrap();
            if !signature_name.is_match(&block.text) {
                continue;
            }
            if let Some(found) = self.typedoc_source(block.bytes.end..preamble_end) {
                scopes.push(BindingScope {
                    directive: BindingDirective {
                        adapter: Some("typescript".into()),
                        path: Some(found),
                        symbol: Some(name),
                        bytes: heading.bytes.clone(),
                        error: None,
                    },
                    bytes: heading.bytes.start..end,
                    heading_level: heading.level,
                });
            }
        }
        if scopes.is_empty()
            && let Some(block) = self.typed_code_blocks.first()
            && let Some(name) = function.captures(&block.text).map(|c| c[1].to_string())
        {
            let end = self
                .headings
                .iter()
                .find(|h| h.bytes.start >= block.bytes.end)
                .map_or(self.source.len(), |h| h.bytes.start);
            if let Some(found) = self.typedoc_source(block.bytes.end..end) {
                scopes.push(BindingScope {
                    directive: BindingDirective {
                        adapter: Some("typescript".into()),
                        path: Some(found),
                        symbol: Some(name),
                        bytes: block.bytes.clone(),
                        error: None,
                    },
                    bytes: block.bytes.start..self.source.len(),
                    heading_level: 0,
                });
            }
        }
        scopes
    }

    fn typedoc_source(&self, bytes: Range<usize>) -> Option<String> {
        let pattern =
            Regex::new(r"(?m)^Defined in: \[([^]\r\n]+\.(?:ts|tsx|js|jsx)):[0-9]+\]\(").unwrap();
        pattern
            .captures_iter(&self.source[bytes.clone()])
            .find_map(|found| {
                let matched = found.get(0).unwrap();
                self.links
                    .iter()
                    .any(|link| {
                        link.bytes.start >= bytes.start + matched.start()
                            && link.bytes.start < bytes.start + matched.end()
                    })
                    .then(|| found[1].to_string())
            })
    }

    pub fn typedoc_claims(&self, scope: &BindingScope) -> ReferenceClaims {
        let mut claims = ReferenceClaims::default();
        for (index, heading) in self.headings.iter().enumerate() {
            if heading.bytes.start < scope.bytes.start
                || heading.bytes.end > scope.bytes.end
                || heading.level <= scope.heading_level
            {
                continue;
            }
            let end = self
                .headings
                .iter()
                .skip(index + 1)
                .find(|h| h.level <= heading.level)
                .map_or(scope.bytes.end, |h| h.bytes.start)
                .min(scope.bytes.end);
            if heading.text.eq_ignore_ascii_case("returns") {
                claims.return_type = typedoc_type(&self.source, heading.bytes.end..end);
            } else if heading.text.eq_ignore_ascii_case("parameters") {
                for (parameter_index, parameter) in self
                    .headings
                    .iter()
                    .enumerate()
                    .skip(index + 1)
                    .take_while(|(_, h)| h.bytes.start < end)
                {
                    if parameter.level != heading.level + 1 {
                        continue;
                    }
                    let name = parameter
                        .text
                        .trim()
                        .trim_end_matches('?')
                        .trim_start_matches("...");
                    if name.is_empty()
                        || !name
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    {
                        continue;
                    }
                    let parameter_end = self
                        .headings
                        .get(parameter_index + 1)
                        .map_or(end, |h| h.bytes.start)
                        .min(end);
                    claims.parameters.push(DocumentedParameter {
                        name: name.into(),
                        type_name: typedoc_type(&self.source, parameter.bytes.end..parameter_end)
                            .map(|t| t.value),
                        bytes: parameter.bytes.clone(),
                    });
                }
            }
        }
        claims
    }

    pub fn reference_claims(&self, scope: &BindingScope) -> ReferenceClaims {
        let mut claims = ReferenceClaims::default();
        let section_headings = self
            .headings
            .iter()
            .enumerate()
            .filter(|(_, heading)| {
                heading.bytes.start >= scope.bytes.start
                    && heading.bytes.end <= scope.bytes.end
                    && heading.level > scope.heading_level
            })
            .collect::<Vec<_>>();

        for (heading_index, (source_index, heading)) in section_headings.iter().enumerate() {
            let title = heading.text.trim().to_ascii_lowercase();
            if !matches!(title.as_str(), "parameters" | "returns" | "errors") {
                continue;
            }
            let next_within = section_headings
                .iter()
                .skip(heading_index + 1)
                .find(|(_, candidate)| candidate.level <= heading.level)
                .map(|(_, candidate)| candidate.bytes.start);
            let next_global = self
                .headings
                .iter()
                .skip(*source_index + 1)
                .find(|candidate| candidate.level <= heading.level)
                .map(|candidate| candidate.bytes.start);
            let end = next_within
                .or(next_global)
                .unwrap_or(scope.bytes.end)
                .min(scope.bytes.end);
            let body = heading.bytes.end..end;
            let spans = self
                .code_spans
                .iter()
                .filter(|span| span.bytes.start >= body.start && span.bytes.end <= body.end)
                .collect::<Vec<_>>();

            match title.as_str() {
                "parameters" => {
                    for line in lines_with_offsets(&self.source, body.clone()) {
                        let trimmed = line.text.trim_start();
                        if !(trimmed.starts_with("- ")
                            || trimmed.starts_with("* ")
                            || trimmed.starts_with("+ "))
                        {
                            continue;
                        }
                        let line_spans = spans
                            .iter()
                            .filter(|span| {
                                span.bytes.start >= line.bytes.start
                                    && span.bytes.end <= line.bytes.end
                            })
                            .collect::<Vec<_>>();
                        if let Some(name) = line_spans.first() {
                            claims.parameters.push(DocumentedParameter {
                                name: name.text.clone(),
                                type_name: line_spans.get(1).map(|span| span.text.clone()),
                                bytes: line.bytes,
                            });
                        }
                    }
                }
                "returns" => {
                    if let Some(span) = spans.first() {
                        claims.return_type = Some(DocumentedValue {
                            value: span.text.clone(),
                            bytes: span.bytes.clone(),
                        });
                    }
                }
                "errors" => {
                    for line in lines_with_offsets(&self.source, body) {
                        let trimmed = line.text.trim_start();
                        if !(trimmed.starts_with("- ")
                            || trimmed.starts_with("* ")
                            || trimmed.starts_with("+ "))
                        {
                            continue;
                        }
                        if let Some(span) = spans.iter().find(|span| {
                            span.bytes.start >= line.bytes.start && span.bytes.end <= line.bytes.end
                        }) {
                            claims.errors.push(DocumentedValue {
                                value: span.text.clone(),
                                bytes: span.bytes.clone(),
                            });
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        claims
    }
}

// Render only the first type paragraph, preserving generic punctuation and
// stripping Markdown links/emphasis. Defaults are not part of a parameter type.
fn typedoc_type(source: &str, bytes: Range<usize>) -> Option<DocumentedValue> {
    let mut text = String::new();
    let mut has_code = false;
    let mut active = false;
    let mut start = bytes.start;
    let mut end = bytes.start;
    for (event, range) in Parser::new(&source[bytes.clone()]).into_offset_iter() {
        match event {
            Event::Start(Tag::Paragraph) => {
                active = true;
                start = bytes.start + range.start;
            }
            Event::End(TagEnd::Paragraph) => {
                end = bytes.start + range.end;
                break;
            }
            Event::Code(value) if active => {
                has_code = true;
                text.push_str(&value);
            }
            Event::Text(value) if active => {
                if let Some((value, _)) = value.split_once(" = ") {
                    text.push_str(value);
                    end = bytes.start + range.start;
                    break;
                }
                text.push_str(&value);
            }
            Event::SoftBreak if active => text.push(' '),
            Event::Start(Tag::CodeBlock(_) | Tag::Heading { .. }) => return None,
            _ => {}
        }
    }
    (has_code && !text.trim().is_empty()).then(|| DocumentedValue {
        value: text.trim().trim_start_matches('|').trim().into(),
        bytes: start..end.max(start),
    })
}

fn parse_directives(html: &str, base: usize) -> Vec<BindingDirective> {
    let directive_re = Regex::new(r#"(?s)<!--\s*vibedoc:source\s+([^>]*?)-->"#).unwrap();
    let attribute_re = Regex::new(r#"([A-Za-z][A-Za-z0-9_-]*)\s*=\s*"([^"]*)""#).unwrap();
    directive_re
        .captures_iter(html)
        .map(|captures| {
            let full = captures.get(0).unwrap();
            let attributes = captures.get(1).unwrap().as_str();
            let mut adapter = None;
            let mut path = None;
            let mut symbol = None;
            let mut unknown = Vec::new();
            for capture in attribute_re.captures_iter(attributes) {
                let key = capture.get(1).unwrap().as_str();
                let value = capture.get(2).unwrap().as_str().to_string();
                match key {
                    "adapter" => adapter = Some(value),
                    "path" => path = Some(value),
                    "symbol" => symbol = Some(value),
                    _ => unknown.push(key.to_string()),
                }
            }
            let error = if !unknown.is_empty() {
                Some(format!("unknown attributes: {}", unknown.join(", ")))
            } else {
                let missing = [
                    ("adapter", adapter.is_none()),
                    ("path", path.is_none()),
                    ("symbol", symbol.is_none()),
                ]
                .into_iter()
                .filter_map(|(name, missing)| missing.then_some(name))
                .collect::<Vec<_>>();
                (!missing.is_empty()).then(|| format!("missing attributes: {}", missing.join(", ")))
            };
            BindingDirective {
                adapter,
                path,
                symbol,
                bytes: (base + full.start())..(base + full.end()),
                error,
            }
        })
        .collect()
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

struct SourceLine<'a> {
    text: &'a str,
    bytes: Range<usize>,
}

fn lines_with_offsets(source: &str, range: Range<usize>) -> Vec<SourceLine<'_>> {
    let mut output = Vec::new();
    let mut cursor = range.start;
    for segment in source[range.clone()].split_inclusive('\n') {
        let end = cursor + segment.len();
        output.push(SourceLine {
            text: segment,
            bytes: cursor..end,
        });
        cursor = end;
    }
    if cursor < range.end {
        output.push(SourceLine {
            text: &source[cursor..range.end],
            bytes: cursor..range.end,
        });
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_directive_and_reference_claims() {
        let source = r#"<!-- vibedoc:source adapter="typescript" path="src/auth.ts" symbol="login" -->
# `login`

Logs in.

## Parameters

- `email` (`string`): The email.

## Returns

`Promise<User>`

## Errors

- `LoginError`: The credentials are invalid.
"#;
        let document = parse_document(
            "/tmp/auth.md",
            "docs/auth.md",
            DocumentProfile::Reference,
            source.to_string(),
        );
        assert_eq!(document.directives.len(), 1);
        let scopes = document.binding_scopes();
        assert_eq!(scopes.len(), 1);
        let claims = document.reference_claims(&scopes[0]);
        assert_eq!(claims.parameters[0].name, "email");
        assert_eq!(claims.parameters[0].type_name.as_deref(), Some("string"));
        assert_eq!(claims.return_type.unwrap().value, "Promise<User>");
        assert_eq!(claims.errors[0].value, "LoginError");
    }

    #[test]
    fn typedoc_methods_preserve_linked_types_defaults_and_scope() {
        let source = r#"# Client

### load()

```ts
load<T>(key?, options?): Promise<T>;
```

Defined in: [src/client.ts:10](https://example.test/blob/main/src/client.ts#L10)

#### Parameters

##### key?

`string`

##### options?

[`Options`](./Options.md)\<`T`\> = `{}`

#### Returns

`Promise`\<`T`\>

### next()

```ts
next(): boolean;
```

Defined in: [src/client.ts:20](https://example.test/blob/main/src/client.ts#L20)

#### Returns

`boolean`
"#;
        let document = parse_document(
            "/tmp/api.md",
            "api.md",
            DocumentProfile::Reference,
            source.into(),
        );
        let scopes = document.typedoc_scopes();
        assert_eq!(scopes.len(), 2);
        assert_eq!(scopes[0].directive.symbol.as_deref(), Some("load"));
        assert_eq!(scopes[0].directive.path.as_deref(), Some("src/client.ts"));
        let claims = document.typedoc_claims(&scopes[0]);
        assert_eq!(claims.parameters.len(), 2);
        assert_eq!(claims.parameters[0].name, "key");
        assert_eq!(
            claims.parameters[1].type_name.as_deref(),
            Some("Options<T>")
        );
        let returned = claims.return_type.unwrap();
        assert_eq!(returned.value, "Promise<T>");
        assert_eq!(&document.source[returned.bytes], "`Promise`\\<`T`\\>\n");
        assert_eq!(
            document
                .typedoc_claims(&scopes[1])
                .return_type
                .unwrap()
                .value,
            "boolean"
        );
    }

    #[test]
    fn typedoc_function_requires_a_real_source_link_and_accepts_leading_union() {
        let source = "```typescript\nfunction fetch(): string | undefined;\n```\n\nDefined in: [src/a.ts:1](https://example.test/a)\n\n## Returns\n\n| `string` | `undefined`\n";
        let document = parse_document(
            "/tmp/api.md",
            "api.md",
            DocumentProfile::Reference,
            source.into(),
        );
        let scopes = document.typedoc_scopes();
        assert_eq!(scopes.len(), 1);
        assert_eq!(
            document
                .typedoc_claims(&scopes[0])
                .return_type
                .unwrap()
                .value,
            "string | undefined"
        );
        let fake = source.replace(
            "Defined in: [src/a.ts:1](https://example.test/a)",
            "```text\nDefined in: [src/a.ts:1](https://example.test/a)\n```",
        );
        assert!(
            parse_document("/tmp/api.md", "api.md", DocumentProfile::Reference, fake)
                .typedoc_scopes()
                .is_empty()
        );
    }

    #[test]
    fn maps_byte_offsets_to_one_based_positions() {
        let document = parse_document(
            "/tmp/a.md",
            "a.md",
            DocumentProfile::Guide,
            "one\ntwo".to_string(),
        );
        assert_eq!(document.position(4), Position { line: 2, column: 1 });
    }

    #[test]
    fn attaches_directive_to_the_next_heading_subtree() {
        let source = r#"# Overview

Introduction.

<!-- vibedoc:source adapter="typescript" path="src/a.ts" symbol="run" -->
## `run`

### Parameters

- `value` (`string`): Value.

## Next
"#;
        let document = parse_document(
            "/tmp/a.md",
            "a.md",
            DocumentProfile::Reference,
            source.into(),
        );
        let scopes = document.binding_scopes();
        assert_eq!(scopes.len(), 1);
        assert_eq!(scopes[0].heading_level, 2);
        assert!(document.source[scopes[0].bytes.clone()].starts_with("## `run`"));
        assert!(!document.source[scopes[0].bytes.clone()].contains("## Next"));
    }

    #[test]
    fn excludes_code_blocks_from_prose_and_preserves_utf8_byte_columns() {
        let source = "# A\n\n```text\nThis handles things.\n```\n\né handles work.\n";
        let document = parse_document("/tmp/a.md", "a.md", DocumentProfile::Guide, source.into());
        assert!(
            document
                .prose
                .iter()
                .all(|span| !span.text.contains("things"))
        );
        let handles = source.rfind("handles").unwrap();
        assert_eq!(document.position(handles), Position { line: 7, column: 4 });
    }
}
