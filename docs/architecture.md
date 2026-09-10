# Architecture

Vibedoc separates documentation policy from language-specific compiler
analysis. The Rust CLI discovers configuration and documents, starts each
requested adapter, and applies deterministic rules to Markdown plus a
language-neutral fact graph.

```text
Markdown ───────────────┐
                       ├─> Rust rule engine ─> diagnostics and coverage
Source ─> adapter ─> facts┘
```

The Cargo workspace contains three crates:

- `vibedoc` implements the command-line interface and output rendering.
- `vibedoc-core` implements discovery, Markdown parsing, adapter process
  management, diagnostics, and rules.
- `vibedoc-protocol` defines JSON-RPC messages and the fact model.

The npm workspace contains `vibedoc-adapter-typescript`. The adapter owns all
TypeScript compiler integration. This boundary lets later adapters use their
language's native tooling without coupling the Rust core to one compiler.

## Trust boundary

An adapter is an executable selected from `PATH` or an absolute command-line
override. Vibedoc passes arguments directly to the operating system. It does
not interpret shell text and does not read executable paths from repository
configuration.

Standard output is reserved for newline-delimited JSON-RPC messages. Adapter
logs use standard error. A protocol violation, timeout, incompatible version,
or nonzero exit is an operational failure.

## Fact model

Adapters return repository-relative POSIX paths and stable symbol identifiers.
A TypeScript method can have this identifier:

```text
typescript:src/auth.ts#AuthenticationService.login
```

Facts include files, symbols, exports, declarations, signatures, parameters,
return types, direct throws, and relationships. Every evidence-bearing fact has
a source range and a confidence of `exact`, `inferred`, or `incomplete`.

The rule engine does not convert insufficient compiler information into a
contradiction. Overloads, dynamic values, `any`, `unknown`, and destructured
parameters remain unverified when a safe comparison is unavailable.

The TypeScript adapter checks compiler type flags, unions, intersections, and
type arguments for incomplete evidence. An unresolved type can retain a name
such as `MissingType` in compiler output while internally being an error type;
its name alone does not make it exact. Arrays, tuples, and generic containers
with incomplete type arguments also remain unverified. Direct throws with
incomplete types cannot verify an Errors claim.

Named object properties, call and construct signatures, and index signatures
are traversed with cycle detection and a 256-type budget. Exceeding the budget
also yields incomplete evidence. Standard-library member internals are skipped;
their container type arguments are still checked. Type parameters remain symbolic.

Each project's compiler program resolves its own symbol relationships before
facts are combined. Identical shared facts are deduplicated. If projects disagree
about a symbol, `TSADAPTER004` marks its evidence incomplete and structural claims
remain unverified. Project ordering does not select a more confident result.
Relevant missing-name, module, and member compiler diagnostics are surfaced as
adapter warnings; this is not a complete TypeScript diagnostic report.

Namespace and nested namespace names participate in symbol identifiers, so
`A.parse` and `B.parse` remain distinct while overloads of `A.parse` merge.
This analysis is not a substitute for TypeScript's full project type check.

## Verification boundary

Deterministic reference sections are verified against compiler facts. General
prose is checked for language and document structure, but it is not factually
verified by default. The output reports verified, contradicted, and unverified
structural claim counts and states whether experimental prose patterns ran.

The experimental patterns recognize a narrow syntax for return, throw, call,
read, and write claims. They can emit warnings only. They are research signals,
not semantic proof.
