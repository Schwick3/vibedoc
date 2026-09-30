# Python adapter prototype

Requires Python 3.10 or newer. Uses the standard library only. The launcher runs
Python in isolated mode; analyzed source is parsed with
[Python's AST parser](https://docs.python.org/3/library/ast.html), never imported,
executed, or passed to annotation evaluation.

Install with `vibedoc adapter install python` when a versioned manifest is available.
For local packages before publication, see [managed adapters](../../docs/adapters.md).
Managed installation uses an existing `python3` on PATH and requires no pip packages.

For source development, build the CLI with `cargo build --workspace`. Configure
explicit Python sources and documents in `vibedoc.toml`:

```toml
version = 1

[[documents]]
include = ["docs/api.md"]
profile = "reference"
adapters = ["python"]

[adapters.python]
sources = ["src/**/*.py"]
```

Run from the project root, replacing the launcher path with your Vibedoc checkout:

```sh
/path/to/vibedoc/target/debug/vibedoc check \
  --adapter-command python=/path/to/vibedoc/adapters/python/bin/vibedoc-adapter-python
```

A reference section uses the existing Markdown format:

```markdown
<!-- vibedoc:source adapter="python" path="src/api.py" symbol="Client.send" -->
# Send

## Parameters
- `body`: `str`

## Returns
- `bool`
```

Explicit bindings select Python even when a document is passed directly on the
command line. For documents without bindings, use configured document discovery;
the CLI's default adapter for explicit reference paths remains TypeScript.

## Evidence boundaries

- Top-level functions, async functions, and directly declared class methods.
  Class declarations are emitted for binding. Nested functions, inherited methods, re-exports, and
  properties are not independently verified.
- Positional, keyword-only, variadic parameters, and source locations. A default
  records optionality; its expression is not evaluated or exported. The current
  protocol does not preserve positional-only/keyword-only calling conventions.
- Instance/class receivers are omitted for normal methods and unshadowed builtin
  `classmethod`; `staticmethod` keeps all parameters.
- Exact type facts mean supported written annotations, not proof that a function
  returns that type at runtime. Supported types are unshadowed builtin names,
  builtin collection annotations, `None`, unions, and the bounded imported/alias
  forms below. Async return annotations describe the awaited result.
- Missing annotations, string forward references, unsupported imports/aliases,
  annotation expressions, and generator return types remain incomplete.
  Shadowing detection deliberately over-approximates across the file.
- Unknown decorators, unsupported inheritance, decorated classes, conditional definitions,
  repeated definitions, and detected callable reassignment yield incomplete
  symbol evidence. Overloads never select the signature that happens to match.
  Dynamic monkey-patching across files is not analyzed.
- No call relationships, throw facts, inferred return types, docstring parsing,
  environment resolution, or general Python type equivalence.
- `projects` is unsupported: configure `sources`. With no sources, `**/*.py`
  is used, excluding common dependency/build directories. External symlink
  targets are excluded. No matches and parse failures are operational errors.

These are conservative source facts, not a Python type checker. The adapter
reuses protocol version 1 and the shared Markdown rules without changing their
wire format.

## Imported types and aliases

Analysis parses selected files once, indexes bindings, then emits facts. Imports
never expand the source globs or load runtime modules. An earlier, unique,
unconditional module binding can identify a supported class, a single-name
assignment alias (`Label = str | None`), or a relative import such as
`from .models import Response as Reply`. The target must directly declare the
class or alias; each traversed package requires its selected `__init__.py`.
Module/package collisions and excluded, malformed, or escaping targets abstain.

Aliases resolve recursively, with cycle detection and a depth limit of 64.
Expanded types are bounded to 4,096 nodes and 16 KiB of normalized text; exceeding
these limits makes the type incomplete. Supported `typing` imports (including
renaming) are Optional, Union, List, Dict, Tuple, Set, and FrozenSet. Optional
requires one argument, Union at least two; collection arity is enforced and
only fixed-length tuples are supported. Bare typing constructors are incomplete.
Builtin collection constructors retain their existing support.

Rebinding, deletion, duplicates, wildcard imports, conditional/TYPE_CHECKING
bindings, and detectable typing shadowing prevent promotion. Absolute project
imports, re-export chains, quoted forward references, generic aliases, arbitrary
attribute expressions, external types, Any, IO, Callable, iterators, mappings,
and sequences remain unsupported. One unresolved component makes the entire
annotation incomplete. Existing class and callable confidence rules still apply.

`display` preserves the written annotation. `normalized` expands supported aliases
and typing forms and qualifies project classes by workspace-relative module path:
`pkg/models.py` becomes `pkg.models.Response`, and `pkg/models/__init__.py` uses
the same module identity. `src/pkg/models.py` uses `src.pkg.models.Response`;
this is a source identity, not a claim about the runtime import environment.
The resolver never chooses a class by basename alone or emits extra alias symbols.

Python comparison accepts established source spellings and structurally equivalent
normalized forms. For example, source `Optional[str]` verifies documentation of
`str | None`, and a source alias expanding to `list[str | None]` accepts that
expansion. Supported builtin expressions that differ contradict. Unmatched names
or unsupported documentation syntax remain unverified: another document-side
alias is not resolved from the runtime environment. In particular, an unmatched
`Optional[int]` spelling is not normalized without established source evidence.
Parameter and callable declaration locations remain the diagnostic evidence.
These comparisons also apply to handwritten member-return rows.

## Validation

```sh
cargo build --workspace
python3 -m unittest discover -s adapters/python -p 'test_*.py'
python3 scripts/test-python-project.py /path/to/python-dotenv /tmp/python-results.json
```

See the [pinned real-project evaluation](../../tests/evaluations/python-dotenv.md).

## Local inheritance

Directly declared methods can be checked on a module-level class whose single
base resolves to an earlier, uniquely declared class in the same file. Chains
may terminate at a class with no base or the unshadowed builtin object. Class
and method confidence both reflect this resolution; inherited-only methods
are not copied into subclasses.

The entire file is checked before promoting confidence, so later assignment,
import, deletion, or redeclaration of an ancestor keeps its descendants
incomplete. Imported, nested, conditional, multiple, generic, expression-based,
cyclic, or unresolved bases remain unsupported. Class decorators, keywords
(including metaclasses), generic parameters, and ancestor subclass/attribute
hooks (__init_subclass__, __getattribute__, __getattr__) also prevent promotion.

Existing method-level checks still apply to decorators, overloads, duplicate
definitions, and detected replacement. Imported annotations are checked only
within the bounded subset described above. This verifies written source signatures, not
runtime behavior or arbitrary dynamic monkey-patching.

## Handwritten class reference lists

A reference heading containing a single inline-code class name binds only when
that name identifies one source symbol. Under that heading, top-level list rows
of the form shown below supply method return claims:

    ## `Response`

    * `def .read()` - **bytes**
    * `def .take(key)` - `bytes`

The optional dot before the method name is accepted. Members are resolved by
qualified class name and source file; a same-named method on another class or
in another file is not substituted. Duplicate class names require an explicit
class source directive, which takes precedence.

This format checks return types only. Parameter text inside the signature
identifies the row's callable but is not verified; no missing-parameter warnings
are inferred from these rows. Constructor rows, properties, rows without return
types, and generator directives are outside this subset. Missing, ambiguous,
decorated, or otherwise incomplete members remain unverified.

The [HTTPX evaluation](../../tests/evaluations/httpx.md) checks unchanged native
Markdown and a deliberately wrong native return claim.
