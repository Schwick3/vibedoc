# Task-service evaluation

This project tests Vibedoc against a small TypeScript service rather than an
isolated parser fixture. The project contains four source files, repository
abstraction calls, re-exports, exact signatures, and direct error evidence.

The configured corpus contains a guide and one reference document. The
`cases/invalid-reference.md` file is excluded from normal checks and contains
deliberate defects.

## Baseline results

The baseline was recorded on September 8, 2026 with Vibedoc 0.1.0.

| Run | Status | Errors | Warnings | Verified | Contradicted | Unverified |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Configured documents | pass | 0 | 0 | 14 | 0 | 0 |
| Seeded invalid reference | fail | 3 | 13 | 1 | 3 | 1 |

`vibedoc doctor` found two Markdown documents and loaded four source files with
14 symbols. Inspection of `TaskService.complete` returned two parameters, the
`Promise<Task>` return type, `TaskNotFoundError`, and direct repository calls.

## Manual classification

The clean document contains no known seeded defects and produced no findings.
All findings in the invalid document correspond to deliberate defects:

- a sentence longer than 25 words;
- vague and qualitative wording;
- two forbidden terminology uses;
- a skipped heading level and vague link text;
- one unknown parameter and one missing parameter;
- incorrect parameter and return types;
- an error without direct throw evidence;
- unsupported experimental return and call statements.

This controlled corpus therefore produced no observed false positives or false
negatives. It is not evidence of real-world precision because the defects were
constructed from the current rule definitions.

## Verification boundary observed

The run confirms that the TypeScript adapter can resolve calls through an
interface-typed repository. It also confirms that deterministic reference
sections contribute explicit coverage counts.

The ordinary summary sentences were not factually verified. The experimental
patterns evaluated only their narrow return and call syntax. A later evaluation
against independently authored documentation is still required.

## Reproduction

From the Vibedoc repository root, run:

```sh
./scripts/test-evaluation.sh
```
