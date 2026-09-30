# Python adapter evaluation: python-dotenv

Evaluated September 30, 2026 with bounded Python type resolution and Python 3.12.0.

Source: [theskumar/python-dotenv](https://github.com/theskumar/python-dotenv),
revision [a00cb2eed0704cd6d2071b2004c37e95ccc86ee5](https://github.com/theskumar/python-dotenv/tree/a00cb2eed0704cd6d2071b2004c37e95ccc86ee5).

The repository was cloned from GitHub. Its source and tracked documentation were
not changed. No upstream packages were installed and no project code, annotation
expressions, or upstream scripts were executed. The adapter parsed
`src/dotenv/**/*.py`; the CLI consumed its protocol-v1 facts.

## Results

| Check | Verified | Contradicted | Unverified | Exit |
| --- | ---: | ---: | ---: | ---: |
| Unchanged upstream README, reference profile | 0 | 0 | 0 | 0 |
| Controlled `find_dotenv` reference | 7 | 0 | 0 | 0 |
| Wrong parameter name and return type | 4 | 2 | 0 | 1 |
| Controlled `load_dotenv` reference | 11 | 0 | 2 | 0 |
| Same partial reference with `--deny-warnings` | 11 | 0 | 2 | 1 |
| Equivalent `str \| None` encoding type | 11 | 0 | 2 | 0 |
| Wrong encoding type (`int`) | 10 | 1 | 2 | 1 |

The unchanged README emits `VDOC-G010`: its prose and examples have no recognized
structural reference claims. This is a coverage gap, not successful verification
of the project's existing documentation.

The controlled Markdown is written by the evaluation, using explicit source
bindings. It is not upstream documentation. For `find_dotenv`, three parameter
names, three builtin annotations, and the return annotation are verified.
Changing `filename` to `missing` and the return type from `str` to `int`
produces `VDOC-G003` and `VDOC-G006`, with evidence at
`src/dotenv/main.py:337`. A missing-parameter warning also identifies the omitted
`filename`.

For `load_dotenv`, all six names, three boolean annotations, the boolean return,
and `encoding: Optional[str]` now verify. The imported Optional constructor is
recognized and normalizes to `None | str`; documenting `str | None` also verifies.
Changing the documented encoding type to `int` produces VDOC-G005 with the exact
parameter declaration as evidence. That location is checked independently from
the pinned AST.

`Optional[StrPath]` and `Optional[IO[str]]` remain unverified with VDOC-G008.
StrPath contains a quoted external PathLike reference, and IO is outside the
supported typing forms. Matching annotation text does not make these types exact.

This establishes source binding, positive checks, contradictions, and abstention
for a bounded Python subset. It does not establish broad Python documentation
coverage, type inference, runtime behavior, or general alias equivalence.
Protocol v1 remains unchanged; it does not carry default expressions or
positional-only/keyword-only calling conventions.

Machine-readable results: [python-dotenv-results.json](python-dotenv-results.json).

## Reproduce

From the Vibedoc checkout:

```sh
cargo build --workspace
npm ci
git clone https://github.com/theskumar/python-dotenv.git /tmp/vibedoc-python-dotenv
git -C /tmp/vibedoc-python-dotenv checkout --detach a00cb2eed0704cd6d2071b2004c37e95ccc86ee5
python3 -m unittest discover -s adapters/python -p 'test_*.py'
python3 scripts/test-python-project.py /tmp/vibedoc-python-dotenv /tmp/python-dotenv-results.json
```

The script requires the pinned revision and a clean tracked checkout, validates
CLI report schemas and expected results, and removes its temporary documents
and configuration. CI runs the adapter tests on macOS and Linux and this pinned
real-project evaluation on Linux.
