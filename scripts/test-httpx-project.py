"""Pinned HTTPX evaluation: original Markdown versus controlled bindings."""
import collections
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
CHECKOUT = Path(sys.argv[1]).resolve()
REVISION = "b5addb64f0161ff6bfe94c124ef76f6a1fba5254"
ADAPTER = ROOT / "adapters/python/bin/vibedoc-adapter-python"
TICK = chr(96)


def git(*args):
    return subprocess.check_output(["git", "-C", str(CHECKOUT), *args], text=True).strip()


assert git("rev-parse", "HEAD") == REVISION
assert git("status", "--porcelain", "--untracked-files=no") == ""
requests = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": 1}},
    {"jsonrpc": "2.0", "id": 2, "method": "analyze",
     "params": {"workspaceRoot": str(CHECKOUT), "sourceGlobs": ["httpx/**/*.py"], "projects": []}},
    {"jsonrpc": "2.0", "id": 3, "method": "shutdown", "params": {}},
]
run = subprocess.run([str(ADAPTER), "--stdio"],
                     input="".join(json.dumps(r) + "\n" for r in requests),
                     text=True, capture_output=True, timeout=60, check=True)
replies = [json.loads(line) for line in run.stdout.splitlines()]
assert len(replies) == 3 and all("result" in r for r in replies), replies
analysis = replies[1]["result"]
assert analysis["diagnostics"] == []
graph = analysis["graph"]
assert len(graph["files"]) == 23
assert len(graph["symbols"]) == 515
assert len({s["id"] for s in graph["symbols"]}) == 515
symbols = {s["qualifiedName"]: s for s in graph["symbols"]}
selected = {name: symbols[name] for name in ("request", "Response.read", "Client.get", "Client.close")}
assert selected["Response.read"]["confidence"] == "exact"
assert selected["Response.read"]["signatures"][0]["returnType"] == {
    "display": "bytes", "normalized": "bytes", "confidence": "exact"}
assert selected["Client.get"]["confidence"] == "exact"
native = (CHECKOUT / "docs/api.md").read_text()
results = {
    "repository": "https://github.com/encode/httpx", "revision": REVISION,
    "vibedocBaseRevision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
    "implementation": "Same-file single-inheritance confidence for directly declared Python methods",
    "adapter": replies[0]["result"]["adapter"],
    "nativeDocument": {"path": "docs/api.md", "sha256": hashlib.sha256(native.encode()).hexdigest()},
    "facts": {"files": len(graph["files"]), "symbols": len(graph["symbols"]),
              "symbolConfidence": dict(collections.Counter(s["confidence"] for s in graph["symbols"])),
              "selected": selected},
    "checks": {},
}
with tempfile.TemporaryDirectory(prefix=".vibedoc-httpx-", dir=CHECKOUT) as directory:
    temporary = Path(directory)
    config = CHECKOUT / (temporary.name + ".toml")
    try:
        def check(name, document, expected_counts):
            relative = document.relative_to(CHECKOUT).as_posix()
            config.write_text(f'version = 1\n[adapters.python]\nsources = ["httpx/**/*.py"]\n'
                              f'[[documents]]\ninclude = ["{relative}"]\nprofile = "reference"\nadapters = ["python"]\n')
            run = subprocess.run(
                [str(ROOT / "target/debug/vibedoc"), "check", "--config", str(config),
                 "--format", "json", "--adapter-command", f"python={ADAPTER}"],
                cwd=CHECKOUT, text=True, capture_output=True, timeout=60)
            assert run.returncode in (0, 1), run.stderr + run.stdout
            report = json.loads(run.stdout)
            saved = temporary / "report.json"
            saved.write_text(run.stdout)
            subprocess.run(["node", str(ROOT / "scripts/validate-report.mjs"),
                            str(ROOT / "schemas/vibedoc-report.schema.json"), str(saved)],
                           capture_output=True, check=True)
            assert run.returncode == (1 if report["summary"]["errors"] else 0), report
            counts = report["verification"]
            assert [counts[k] for k in ("verifiedStructuralClaims", "contradictedStructuralClaims",
                                       "unverifiedStructuralClaims")] == expected_counts, report
            results["checks"][name] = {
                "exitCode": run.returncode, "summary": report["summary"], "verification": counts,
                "diagnostics": [{"ruleId": d["ruleId"], "message": d["message"],
                                 "evidence": d.get("evidence", [])} for d in report["diagnostics"]],
            }
            return report

        original = check("unchangedApi", CHECKOUT / "docs/api.md", [4, 0, 14])
        assert not any(d["ruleId"] in ("VDOC-G009", "VDOC-G010") for d in original["diagnostics"])
        document = temporary / "api.md"
        before = f"* {TICK}def .read(){TICK} - **bytes**"
        after = f"* {TICK}def .read(){TICK} - **str**"
        assert native.count(before) == 1
        document.write_text(native.replace(before, after))
        mutated = check("mutatedNativeReturn", document, [3, 1, 14])
        error = next(d for d in mutated["diagnostics"] if d["ruleId"] == "VDOC-G006")
        assert error["evidence"][0] == selected["Response.read"]["declaration"]

        def reference(symbol, returns=None):
            signature = symbol["signatures"][0]
            parameters = "\n".join(f'- {TICK}{p["name"]}{TICK}: {TICK}{p["typeFact"]["display"]}{TICK}'
                                   for p in signature["parameters"])
            return (f'<!-- vibedoc:source adapter="python" path="{symbol["declaration"]["path"]}" '
                    f'symbol="{symbol["qualifiedName"]}" -->\n# Reference\n\n## Parameters\n{parameters}\n'
                    f'\n## Returns\n- {TICK}{returns or signature["returnType"]["display"]}{TICK}\n')

        document.write_text(reference(selected["Response.read"]))
        check("boundRead", document, [1, 0, 0])
        document.write_text(reference(selected["Response.read"], "str"))
        wrong = check("boundReadWrongReturn", document, [0, 1, 0])
        error = next(d for d in wrong["diagnostics"] if d["ruleId"] == "VDOC-G006")
        assert error["evidence"][0] == selected["Response.read"]["declaration"]
        document.write_text(reference(selected["request"]))
        check("boundRequest", document, [18, 0, 13])
        document.write_text(reference(selected["request"]).replace(f"{TICK}method{TICK}: {TICK}str{TICK}", f"{TICK}missing{TICK}: {TICK}str{TICK}"))
        wrong = check("boundRequestWrongParameter", document, [16, 1, 13])
        assert any(d["ruleId"] == "VDOC-G003" for d in wrong["diagnostics"])
        document.write_text(reference(selected["Client.get"]))
        check("boundSubclassMethod", document, [8, 0, 9])
        document.write_text(reference(selected["Client.get"]).replace(f"{TICK}url{TICK}:", f"{TICK}missing{TICK}:"))
        wrong = check("boundSubclassWrongParameter", document, [7, 1, 8])
        error = next(d for d in wrong["diagnostics"] if d["ruleId"] == "VDOC-G003")
        assert error["evidence"][0] == selected["Client.get"]["declaration"]
        assert selected["Client.close"]["confidence"] == "exact"
        assert selected["Client.close"]["signatures"][0]["returnType"] == {
            "display": "None", "normalized": "None", "confidence": "exact"}
        document.write_text(reference(selected["Client.close"]))
        check("boundSubclassClose", document, [1, 0, 0])
        document.write_text(reference(selected["Client.close"], "str"))
        wrong = check("boundSubclassCloseWrongReturn", document, [0, 1, 0])
        error = next(d for d in wrong["diagnostics"] if d["ruleId"] == "VDOC-G006")
        assert error["evidence"][0] == selected["Client.close"]["declaration"]
    finally:
        if config.exists():
            config.unlink()
assert git("status", "--porcelain", "--untracked-files=no") == ""
assert hashlib.sha256((CHECKOUT / "docs/api.md").read_bytes()).hexdigest() == results["nativeDocument"]["sha256"]
serialized = json.dumps(results, indent=2) + "\n"
if len(sys.argv) > 2:
    Path(sys.argv[2]).write_text(serialized)
print(serialized)
