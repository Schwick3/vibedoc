# Python adapter prototype

Requires Python 3.10 or newer. Uses the standard library only. The launcher runs
Python in isolated mode; analyzed source is parsed with
[Python's AST parser](https://docs.python.org/3/library/ast.html), never imported,
executed, or passed to annotation evaluation.

Build the CLI with `cargo build --workspace`, then configure explicit Python
sources and documents in `vibedoc.toml`:

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
  Nested functions, inherited methods, re-exports, classes themselves, and
  properties are not independently verified.
- Positional, keyword-only, variadic parameters, and source locations. A default
  records optionality; its expression is not evaluated or exported. The current
  protocol does not preserve positional-only/keyword-only calling conventions.
- Instance/class receivers are omitted for normal methods and unshadowed builtin
  `classmethod`; `staticmethod` keeps all parameters.
- Exact type facts mean supported written annotations, not proof that a function
  returns that type at runtime. Supported types are unshadowed builtin names,
  builtin collection annotations, `None`, and unions of supported annotations.
  Async return annotations describe the awaited result.
- Missing annotations, string forward references, aliases, imported types,
  annotation expressions, and generator return types remain incomplete.
  Shadowing detection deliberately over-approximates across the file.
- Unknown decorators, decorated/inherited classes, conditional definitions,
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

## Validation

```sh
cargo build --workspace
python3 -m unittest discover -s adapters/python -p 'test_*.py'
python3 scripts/test-python-project.py /path/to/python-dotenv /tmp/python-results.json
```

See the [pinned real-project evaluation](../../tests/evaluations/python-dotenv.md).
