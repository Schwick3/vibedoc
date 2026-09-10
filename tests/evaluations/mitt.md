# Real-project evaluation: mitt

Evaluated on September 10, 2026 with Vibedoc 0.1.2 plus the compiler-fact
correctness changes in this working tree, using the pinned TypeScript 6.0.3.

Upstream: [developit/mitt](https://github.com/developit/mitt), revision
[`6b41670516ed8e8b738612f60491995470aa63b3`](https://github.com/developit/mitt/tree/6b41670516ed8e8b738612f60491995470aa63b3).
The repository was cloned from GitHub. Its tracked files were left unchanged.

## Method

The evaluation analyzes the original `src/index.ts` and checks the original
README as a guide. It independently asserts the factory's optional `all`
parameter, `Emitter<Events>` return type, declaration line, unique symbol IDs,
and two signatures each for `Emitter.on`, `Emitter.off`, and `Emitter.emit`.
It also checks the optional handler parameter on `Emitter.off`.

A temporary TypeScript configuration extends upstream's configuration, retaining
strict mode. It selects the library source, excludes tests requiring upstream
development dependencies, and sets ES2022, ESNext modules, and Bundler resolution
for the adapter's pinned compiler. No upstream dependencies or package scripts
are installed or executed. This is a source-adapter evaluation, not a run of
mitt's own test suite or its unmodified TypeScript 4.9 build configuration.

Temporary reference documents exercise correct and incorrect claims about the
real `mitt` factory and its interface. A temporary copy of the source changes
only the factory return annotation from `Emitter<Events>` to
`MissingEmitter<Events>` to check incomplete evidence. All five CLI reports are
validated against Vibedoc's report schema. Temporary files are removed afterward.

## Results

The adapter returned 1 source file, 12 symbols, 1 relationship, and no adapter
diagnostics. It preserved the factory's exact generic return and all three
pairs of overload signatures.

| Run | Exit | Errors | Warnings | Verified | Contradicted | Unverified |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Original README, guide profile | 0 | 0 | 1 | 0 | 0 | 0 |
| Correct factory reference | 0 | 0 | 0 | 2 | 0 | 0 |
| Incorrect factory reference | 1 | 2 | 1 | 0 | 2 | 0 |
| Overloaded interface method | 0 | 0 | 1 | 0 | 0 | 1 |
| Unresolved return in source copy | 0 | 0 | 1 | 1 | 0 | 1 |

The README warning is `VDOC-D001`: a level-four heading follows a level-two
heading. That is a structural style finding, not an API contradiction.

The incorrect reference documents a nonexistent `ghost` parameter and a
`number` return. Vibedoc reports `VDOC-G003` and `VDOC-G006`, plus the missing
`all` parameter warning. The unresolved return produces `VDOC-G008`, avoiding
a false contradiction. The overloaded method also produces `VDOC-G008`, as
required by the existing conservative overload policy.

Machine-readable evidence: [mitt-results.json](mitt-results.json).

## Limits

Mitt is a small, single-source library. This run does not establish accuracy
for large monorepos, framework code, or arbitrary documentation. The original
README's prose is not factually verified. The added reference documents and
source mutation are controlled tests, not independently authored upstream API
documentation. Namespace identity is covered separately by adapter regression
tests because mitt does not use namespaces.

Type completeness checks inspect compiler flags and nested type arguments;
they are not a full structural proof of every property in a named object type.
The adapter still does not report every TypeScript semantic diagnostic. A normal
TypeScript build remains a separate check.

## Reproduce

From the Vibedoc repository root:

```sh
npm ci
npm run build
cargo build --workspace

git clone https://github.com/developit/mitt.git /tmp/vibedoc-mitt
git -C /tmp/vibedoc-mitt checkout --detach 6b41670516ed8e8b738612f60491995470aa63b3
npm run test:real-project -- /tmp/vibedoc-mitt /tmp/mitt-results.json
```

Use a clean checkout at the pinned revision. The script refuses modified tracked
files and creates uniquely named temporary evaluation files within the checkout.
The optional final argument writes the results JSON. Normal tests remain offline;
this real-project evaluation runs explicitly after cloning upstream.
