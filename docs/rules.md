# Vibedoc Software Documentation Profile

This profile is an original controlled-language and evidence-checking profile
for software documentation. ASD-STE100 and ISO plain-language principles inform
its design, but the profile does not claim conformance or certification.

Both profiles parse English CommonMark with GFM extensions. Prose checks ignore
fenced and indented code blocks. Grounding checks inspect inline code only when
the inline code has a defined structural role.

## Profiles

The `guide` profile applies language, terminology, heading, and link rules. The
`reference` profile applies those rules and compiler-backed structural checks.

Reference headings can bind automatically when one inline code span identifies
exactly one source symbol. An explicit binding has this form:

```md
<!-- vibedoc:source adapter="typescript" path="src/auth.ts" symbol="AuthenticationService.login" -->
```

A directive before all headings binds the whole document. Otherwise, the
directive binds the next heading and its subtree.

Reference sections use these shapes:

```md
## Parameters

- `name` (`Type`): Description.

## Returns

`ReturnType`

## Errors

- `ErrorType`: Condition.
```

## Rules

| Rule | Default | Meaning |
| --- | --- | --- |
| `VDOC-L001` | warning | A prose sentence exceeds 25 words. |
| `VDOC-L002` | warning | Prose contains built-in vague wording. |
| `VDOC-L003` | warning | Prose contains an unsupported qualitative word. |
| `VDOC-T001` | warning | Prose uses configured forbidden terminology. |
| `VDOC-D001` | warning | Consecutive headings skip a level. |
| `VDOC-D002` | warning | Link text does not identify its destination. |
| `VDOC-G001` | error | An explicit binding is invalid or unresolved. |
| `VDOC-G002` | warning | An automatic binding matches several symbols. |
| `VDOC-G003` | error | A documented parameter is absent from the source signature. |
| `VDOC-G004` | warning | An inspectable source parameter is undocumented. |
| `VDOC-G005` | error | A documented parameter type contradicts compiler evidence. |
| `VDOC-G006` | error | A documented return type contradicts compiler evidence. |
| `VDOC-G007` | warning | A documented error lacks direct static throw evidence. |
| `VDOC-G008` | warning | Compiler facts are insufficient for comparison. |
| `VDOC-G009` | error | A reference heading names no source symbol. |
| `VDOC-X001` | warning | An experimental return or throw pattern lacks direct evidence. |
| `VDOC-X002` | warning | An experimental operation pattern lacks a direct relationship. |

Use `vibedoc explain RULE_ID` for the rationale, examples, and remediation for
one rule. Set a rule to `off`, `info`, `warning`, or `error` in the `[rules]`
table. Experimental rules are capped at warning severity.

## Severity policy

Structural contradictions are errors by default. Language quality,
completeness, ambiguous bindings, and insufficient static evidence are warnings
by default. Experimental prose checks are warning-only.

No diagnostic for a free-form statement means only that no enabled rule found
a problem. It does not mean that Vibedoc verified the statement against source
code.
