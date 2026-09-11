# Vibedoc

Vibedoc is an evidence-aware checker for software documentation. It combines a
small controlled-language profile with compiler facts from external source
adapters. The first adapter analyzes TypeScript and JavaScript with a pinned
TypeScript compiler.

Vibedoc does not generate documentation, call an LLM, use the network at
runtime, or claim formal conformance with ASD-STE100 or ISO 24495.

## Install

Install the CLI and TypeScript adapter with Homebrew:

```sh
brew install Schwick3/tap/vibedoc
```

Homebrew installs Node.js, the pinned TypeScript compiler, and both Vibedoc
executables. No Cargo or npm installation and no manual `PATH` change is
required.

Prebuilt releases support Apple Silicon macOS and ARM64 or x86-64 Linux.
Intel macOS is not supported in this research-stage release.

Confirm that the complete installation is available:

```sh
vibedoc --version
vibedoc doctor
```

The TypeScript adapter remains a separate release artifact, but the Homebrew
formula installs it as Vibedoc's default adapter. Future adapters can use the
same external-adapter protocol without being compiled into the Rust CLI.

## Install from source

The source installation is intended for Vibedoc contributors. It requires:

- macOS or Linux
- Rust with Cargo
- Node.js 22 or later
- npm

Run the development installer from the repository root:

```sh
./scripts/install-dev.sh
```

The script installs `vibedoc` with Cargo and links
`vibedoc-adapter-typescript` with npm. Both executable directories must be on
`PATH`.

For repository-local development, build both workspaces and add the adapter
directory to `PATH`:

```sh
cargo build --workspace
npm ci
npm run build
export PATH="$PWD/adapters/typescript/bin:$PATH"
```

## Quick start

Create a configuration file in a TypeScript or JavaScript project:

```sh
vibedoc init
vibedoc doctor
vibedoc check
```

Use `vibedoc init --force` only when the existing `vibedoc.toml` can be
replaced.

Reference documentation can bind Markdown to a source symbol:

```md
<!-- vibedoc:source adapter="typescript" path="src/auth.ts" symbol="AuthenticationService.login" -->
# `AuthenticationService.login`

## Parameters

- `email` (`string`): The email address.

## Returns

`Promise<User>`

## Errors

- `InvalidCredentialsError`: The email address is not valid.
```

Run a check with deterministic human output:

```sh
vibedoc check
```

Machine-readable output uses the checked-in versioned schema:

```sh
vibedoc check --format json
```

The [report schema](schemas/vibedoc-report.schema.json) defines the JSON
contract. A successful check can still contain warnings. Use
`--deny-warnings` when warnings must fail the command.

Reference documents with no recognized structural claims emit `VDOC-G010`:
their API documentation has not been verified. This is evaluated per document,
so verified claims in one file cannot hide zero coverage in another. Fenced
signatures alone do not count as structural claims. Guide documents are exempt.

The reference profile accepts [TypeDoc-style Markdown](docs/rules.md#typedoc-markdown)
with callable signatures, “Defined in” source links, Parameters subheadings, and
Returns sections. Existing supported API pages can be checked without rewriting
them into Vibedoc's list format.

## Commands

```text
vibedoc init [--force]
vibedoc check [PATHS...] [--config PATH] [--profile guide|reference]
              [--format text|json] [--project adapter=PATH]
              [--adapter-command adapter=/ABSOLUTE/PATH]
              [--experimental] [--deny-warnings]
vibedoc doctor [--format text|json]
vibedoc inspect [--adapter NAME] [--symbol QUERY] [--format text|json]
vibedoc explain RULE_ID [--format text|json]
```

`vibedoc check` returns 0 when no diagnostics fail, 1 for failing diagnostics,
and 2 for configuration, adapter, protocol, or execution failures.

## Configuration

Vibedoc finds the nearest `vibedoc.toml` by walking upward. Relative paths are
resolved from the directory that contains the configuration file. Unknown TOML
keys are rejected.

```toml
version = 1

[[documents]]
include = ["README.md", "docs/**/*.md"]
exclude = ["docs/api/**", "docs/archive/**"]
profile = "guide"

[[documents]]
include = ["docs/api/**/*.md"]
profile = "reference"
adapters = ["typescript"]

[adapters.typescript]
projects = ["tsconfig.json", "packages/web/tsconfig.json"]
# Used only when no project configuration is available.
sources = ["src/**/*.ts", "src/**/*.tsx", "src/**/*.js", "src/**/*.jsx"]

[terms."access token"]
forbidden = ["auth token", "login token"]

[rules]
VDOC-L001 = "info"
VDOC-D002 = "off"

[experimental]
prose_grounding = false
```

Document sets must not assign conflicting profiles to one file. Monorepo
projects are intentionally explicit; Vibedoc does not recursively discover
project configurations.

Adapter executables are resolved as `vibedoc-adapter-<name>` on `PATH`. The
`--adapter-command` option accepts an absolute executable path. Repository
configuration cannot supply executable commands, and Vibedoc never starts an
adapter through a shell.

See the [architecture](docs/architecture.md), [protocol](docs/protocol.md), and
[rule profile](docs/rules.md) for the research contracts.

## Development

Run the complete local verification sequence:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm ci
npm test
./scripts/test-e2e.sh
./scripts/test-evaluation.sh
```

The TypeScript compiler version is pinned in the adapter package and lockfile.
The adapter uses compiler options from each selected project without loading
that project's installed TypeScript implementation.

The task-service [evaluation project](tests/projects/task-service/EVALUATION.md)
exercises the CLI against a small multi-file application with both clean and
seeded-failure documentation.

The [mitt evaluation](tests/evaluations/mitt.md) checks a pinned GitHub project,
its original README, and controlled reference claims against its real source.
It includes an opt-in reproduction script and recorded results.

The [TanStack Query evaluation](tests/evaluations/query.md) covers two monorepo
packages, cross-package calls, and native documentation coverage. CI runs both
evaluations against pinned upstream revisions.

## Releases

Public releases contain native CLI archives for Apple Silicon macOS and ARM64
or x86-64 Linux, plus a platform-independent TypeScript adapter archive. See
[RELEASING.md](RELEASING.md) for the release and Homebrew publication process.

## Scope

Vibedoc v1 supports Markdown and TypeScript, TSX, JavaScript, and JSX source
files. Vue and Svelte components, automatic fixes, caching, editor integration,
Windows, generation, and semantic LLM verification are outside this milestone.
The [roadmap](ROADMAP.md) records the requested distribution and integration
candidates.
