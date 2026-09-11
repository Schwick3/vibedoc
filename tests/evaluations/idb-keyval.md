# Independent TypeDoc evaluation: idb-keyval

Evaluated September 11, 2026 with default TypeDoc layout support added after
Vibedoc commit `62634d4`.

Source: [jakearchibald/idb-keyval](https://github.com/jakearchibald/idb-keyval),
revision [`17a69a1165bef486d88950cb47d3913744f038ac`](https://github.com/jakearchibald/idb-keyval/tree/17a69a1165bef486d88950cb47d3913744f038ac).
The repository was cloned from GitHub and its tracked files were not modified.

## Independent generation

Markdown was generated from the original `src/index.ts`, using the upstream
`src/tsconfig.json`, with TypeDoc 0.28.20, typedoc-plugin-markdown 4.13.0, and
TypeScript 5.8.3. These tools were installed in a separate temporary prefix with
npm lifecycle scripts disabled. Upstream package scripts were not executed.

The generator used its default signature, source-path, and parameter layouts.
Only the input entry point, tsconfig, Markdown plugin, and output directory were
specified. No generated Markdown was rewritten to satisfy Vibedoc. This evaluates
freshly generated documentation, not documentation published by idb-keyval's author.

Vibedoc analyzed the original source with its own pinned TypeScript 6.0.3 adapter.
All 13 generated function names were present in the source fact graph. The graph
reports an exact `Promise<void>` return for `clear`, confirming that source facts
are available independently of whether the Markdown is recognized.

## Results

| Run | Documents | Verified | Contradicted | Unverified | Exit |
| --- | ---: | ---: | ---: | ---: | ---: |
| Baseline before support (`62634d4`) | 13 | 0 | 0 | 0 | 0 |
| Unchanged default generated function pages | 13 | 56 | 0 | 3 | 0 |
| Same check with `--deny-warnings` | 13 | 56 | 0 | 3 | 1 |
| Mutated `clear` Returns section | 13 | 55 | 1 | 3 | 1 |

All 13 function pages now have recognized structural claims. The baseline emitted
13 `VDOC-G010` warnings; the updated check emits none. Three types remain explicitly
unverified: `set.value` and `setMany.entries` contain imprecise compiler types,
and TypeDoc abbreviates `update.updater` to `(oldValue) => T`, omitting its
callback parameter annotation. Those three `VDOC-G008` warnings cause
`--deny-warnings` to fail.

The generator reverses the order of the simple union in `promisifyRequest.request`;
that is accepted as equivalent. Complex type equivalence remains outside the
bounded comparison.

For the negative check, only the generated `clear` Returns paragraph changes from
`Promise<void>` to `Promise<string>`. This produces exactly one `VDOC-G006`
contradiction with evidence at `src/index.ts:187`. The original source remains
unchanged. Hashes describe the pristine generated pages before this mutation.

The default `Function: name()` headings and blockquoted signatures are recognized.
Abbreviated `index.ts` source labels resolve to the single matching file,
`src/index.ts`. Resolution requires a unique file suffix and never fetches the
source URL; ambiguous files require explicit directives. Constructors, properties,
parameter tables, and general TypeScript type equivalence are not established
by this evaluation.

Machine-readable results and hashes of all generated function pages:
[idb-keyval-results.json](idb-keyval-results.json).

## Reproduce

From the Vibedoc repository root, after building the CLI and adapter:

```sh
git clone https://github.com/jakearchibald/idb-keyval.git /tmp/vibedoc-idb-keyval
git -C /tmp/vibedoc-idb-keyval checkout --detach 17a69a1165bef486d88950cb47d3913744f038ac
npm install --prefix /tmp/vibedoc-typedoc-generator --save-exact --ignore-scripts --no-audit --no-fund typedoc@0.28.20 typedoc-plugin-markdown@4.13.0 typescript@5.8.3
node scripts/test-generated-typedoc.mjs /tmp/vibedoc-idb-keyval /tmp/vibedoc-typedoc-generator /tmp/idb-keyval-results.json
```

The script requires the pinned source revision, a clean tracked checkout, and
matching generator versions. It generates documentation in a temporary checkout
subdirectory, checks the default function pages, validates the JSON report schema,
checks exact verification counts, mutates the return claim, verifies source
evidence, and removes its temporary files. The same evaluation is included in
the real-projects CI job. Transitive npm dependencies are not
lockfile-pinned by these reproduction instructions; generated-page hashes provide
an additional comparison against this recorded run.
