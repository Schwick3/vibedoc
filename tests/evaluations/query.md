# Real-project evaluation: TanStack Query

Re-evaluated September 11, 2026 using Vibedoc 0.1.2 with the monorepo correctness
changes, TypeDoc Markdown support, and the pinned TypeScript 6.0.3 compiler.

Upstream: [TanStack/query](https://github.com/TanStack/query), revision
[`7452ef68a2901fc9a5673a8e5f53185a506b182f`](https://github.com/TanStack/query/tree/7452ef68a2901fc9a5673a8e5f53185a506b182f).
This repository was cloned from GitHub. Tracked upstream files remained unchanged.

## Scope and configuration

The evaluation selects the `query-core` and `query-persist-client-core` packages:
27 source files, 1,049 symbols, and 708 relationships. It analyzes both packages
in separate compiler programs, with shared core source appearing in both.

Temporary configurations extend each package's own configuration. They select
`src/*.ts`, disable composite/incremental output, clear ambient `types`, and map
`@tanstack/query-core` to its source entry point. This keeps upstream strictness
and module settings while avoiding unrelated test/build dependencies. No upstream
package scripts run or dependencies are installed. This does not run TanStack's
own test suite or reproduce its complete monorepo build.

## Findings and fixes

Before the fix, `persistQueryClientRestore`'s call to `hydrate` and
`persistQueryClientSave`'s call to `dehydrate` had no target IDs when both projects
were selected. TypeScript symbols belong to individual compiler programs, but
Vibedoc skipped shared files globally before resolving relationships.

The adapter now resolves relationships within each program, then combines facts.
Both cross-package target IDs resolve correctly. Identical shared facts are
collapsed without creating artificial overloads. The evaluation asserts that
reversing project order produces an identical graph and diagnostic set.

A separate regression fixture selects the same source with strict null checks
both enabled and disabled. Conflicting facts now produce `TSADAPTER004`, and
structural claims about the affected symbol remain unverified. Named object
properties, callbacks, index signatures, and recursive types also receive
bounded completeness checks. Relevant missing-name/module/member TypeScript
diagnostics are surfaced as adapter warnings.

## Documentation results

| Run | Errors | Warnings | Verified | Contradicted | Unverified |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original QueryClient page, guide | 0 | 0 | 0 | 0 | 0 |
| Original persistence page, guide | 0 | 2 | 0 | 0 | 0 |
| Original QueryClient page, reference | 0 | 30 | 78 | 0 | 30 |
| Original dehydrate function page | 0 | 3 | 2 | 0 | 3 |
| Original hydrate function page | 0 | 3 | 4 | 0 | 3 |
| Native QueryClient page with one seeded error | 1 | 30 | 77 | 1 | 30 |
| Controlled correct references | 0 | 0 | 2 | 0 | 0 |
| Controlled incorrect references | 2 | 0 | 0 | 2 | 0 |
| Named dynamic return type | 0 | 1 | 2 | 0 | 1 |

The two persistence-page warnings identify sentences longer than the configured
25-word maximum, at lines 166 and 183. Manual inspection confirms long sentences
at those locations. They are style findings, not factual API defects. No false
positive API contradictions were observed in these runs.

TypeDoc Markdown support now binds method headings and standalone function
signatures through their repository-relative “Defined in” source labels.
The unchanged QueryClient page verifies 78 structural claims; 30 remain unverified
because of incomplete types or overloads. It produces no API contradictions.
The native `dehydrate` and `hydrate` pages verify another six claims. Guide-profile
behavior remains unchanged.

A temporary copy of the native QueryClient page changes only the Returns section
for `clear` from `void` to `string`. Vibedoc reports exactly one `VDOC-G006` error
against the correct source method. This confirms that native Markdown is checked,
not merely counted. The source checkout and original documentation are unchanged.

Controlled documents explicitly bind `QueryClient.clear` and
`Subscribable.hasListeners`. Their correct return types verify, and both seeded
incorrect return types are caught (two of two; no misses in this controlled set).
A `dehydrate` return claim remains unverified because `DehydratedState` contains
dynamic nested evidence; its two parameter names still verify. This intentionally
trades coverage for avoiding confident conclusions from incomplete types.

These small, selected samples do not establish real-world precision or recall.
The supported format is deliberately bounded: method headings, standalone
function pages, TypeScript signature blocks used to identify callables, “Defined
in” links, parameter subheadings, and inline/linked return types. Constructors,
properties, parameter tables, and arbitrary fenced examples are not checked by
this parser. Signature blocks establish bindings; their parameter and return text
is not independently compared in addition to the dedicated sections. Nonzero
coverage is not evidence that every statement in a document was verified.

Machine-readable results: [query-results.json](query-results.json).

## Reproduce

From the Vibedoc root:

```sh
npm ci
npm run build
cargo build --workspace --locked
git clone https://github.com/TanStack/query.git /tmp/vibedoc-query
git -C /tmp/vibedoc-query checkout --detach 7452ef68a2901fc9a5673a8e5f53185a506b182f
npm run test:monorepo -- /tmp/vibedoc-query /tmp/query-results.json
```

The script requires a clean tracked checkout at the pinned revision and removes
its temporary evaluation files. All nine CLI reports are validated against the
report schema. CI runs this evaluation and the pinned mitt evaluation in the
`real-projects` job; ordinary unit tests remain offline.
