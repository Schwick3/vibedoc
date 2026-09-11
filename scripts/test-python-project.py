"""Pinned, source-only real-project evaluation through the actual CLI."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CHECKOUT = Path(sys.argv[1]).resolve()
REVISION = "a00cb2eed0704cd6d2071b2004c37e95ccc86ee5"


def git(*args):
    return subprocess.check_output(["git", "-C", str(CHECKOUT), *args], text=True).strip()


assert git("rev-parse", "HEAD") == REVISION
assert git("status", "--porcelain", "--untracked-files=no") == ""
adapter = ROOT / "adapters/python/bin/vibedoc-adapter-python"
results = {"repository": "https://github.com/theskumar/python-dotenv", "revision": REVISION,
           "runtime": sys.version.split()[0], "checks": {}}
with tempfile.TemporaryDirectory(prefix=".vibedoc-python-", dir=CHECKOUT) as directory:
    temporary = Path(directory)
    config = CHECKOUT / (temporary.name + ".toml")
    config.write_text('version = 1\n[adapters.python]\nsources = ["src/dotenv/**/*.py"]\n')
    try:
        def check(name, document, expected_exit, profile="reference", extra=()):
            run = subprocess.run(
                [str(ROOT / "target/debug/vibedoc"), "check", *([str(document)] if document else []), "--config", str(config),
                 "--profile", profile, "--format", "json", "--adapter-command", f"python={adapter}", *extra],
                cwd=CHECKOUT, text=True, capture_output=True, timeout=60)
            assert run.returncode == expected_exit, run.stderr + run.stdout
            report = json.loads(run.stdout)
            saved = temporary / "report.json"
            saved.write_text(run.stdout)
            subprocess.run(["node", str(ROOT / "scripts/validate-report.mjs"),
                            str(ROOT / "schemas/vibedoc-report.schema.json"), str(saved)], check=True,
                           capture_output=True)
            results["checks"][name] = {
                "exitCode": run.returncode, "summary": report["summary"], "verification": report["verification"],
                "diagnostics": [{"ruleId": d["ruleId"], "message": d["message"],
                                 "evidence": d.get("evidence", [])} for d in report["diagnostics"]]}
            return report

        # Explicitly select Python for the unchanged README, which has no bindings.
        with config.open("a") as stream:
            stream.write('[[documents]]\ninclude = ["README.md"]\nprofile = "reference"\nadapters = ["python"]\n')
        native = check("upstreamReadme", None, 0)
        assert native["verification"]["verifiedStructuralClaims"] == 0
        assert any(d["ruleId"] == "VDOC-G010" for d in native["diagnostics"])

        source = """<!-- vibedoc:source adapter="python" path="src/dotenv/main.py" symbol="find_dotenv" -->
# Find dotenv

## Parameters
- `filename`: `str`
- `raise_error_if_not_found`: `bool`
- `usecwd`: `bool`

## Returns
- `str`
"""
        document = temporary / "reference.md"
        document.write_text(source)
        valid = check("validReference", document, 0)
        assert valid["verification"]["verifiedStructuralClaims"] == 7
        assert valid["summary"]["warnings"] == 0
        document.write_text(source.replace("`filename`", "`missing`").replace("## Returns\n- `str`", "## Returns\n- `int`"))
        wrong = check("mutatedReference", document, 1)
        assert wrong["verification"]["contradictedStructuralClaims"] == 2
        assert {d["ruleId"] for d in wrong["diagnostics"]} >= {"VDOC-G003", "VDOC-G006"}
        for diagnostic in wrong["diagnostics"]:
            if diagnostic["ruleId"] in {"VDOC-G003", "VDOC-G006"}:
                assert diagnostic["evidence"][0]["path"] == "src/dotenv/main.py"

        document.write_text("""<!-- vibedoc:source adapter="python" path="src/dotenv/main.py" symbol="load_dotenv" -->
# Load dotenv

## Parameters
- `dotenv_path`: `Optional[StrPath]`
- `stream`: `Optional[IO[str]]`
- `verbose`: `bool`
- `override`: `bool`
- `interpolate`: `bool`
- `encoding`: `Optional[str]`

## Returns
- `bool`
""")
        uncertain = check("partialAnnotations", document, 0)
        assert uncertain["verification"]["verifiedStructuralClaims"] == 10
        assert uncertain["verification"]["unverifiedStructuralClaims"] == 3
        assert uncertain["verification"]["contradictedStructuralClaims"] == 0
        check("partialAnnotationsDenyWarnings", document, 1, extra=("--deny-warnings",))
    finally:
        config.unlink()
assert git("status", "--porcelain", "--untracked-files=no") == ""
serialized = json.dumps(results, indent=2) + "\n"
if len(sys.argv) > 2:
    Path(sys.argv[2]).write_text(serialized)
print(serialized)
