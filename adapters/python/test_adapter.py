import json
from pathlib import Path
import subprocess
import tempfile
import unittest

import adapter


class PythonFacts(unittest.TestCase):
    def facts(self, source):
        return adapter.analyze_file("src/api.py", source)

    def test_signature_kinds_defaults_and_async(self):
        facts = self.facts("""
async def fetch(key: str, /, optional: bool = False, *items: int,
                required: bytes, flag: bool = True, **options: str) -> list[str]:
    return []
""")
        signature = facts[0]["signatures"][0]
        self.assertEqual([p["name"] for p in signature["parameters"]],
                         ["key", "optional", "items", "required", "flag", "options"])
        self.assertEqual([p["optional"] for p in signature["parameters"]],
                         [False, True, True, False, True, True])
        self.assertEqual([p["rest"] for p in signature["parameters"]],
                         [False, False, True, False, False, True])
        self.assertEqual(signature["returnType"]["display"], "list[str]")
        self.assertEqual(signature["returnType"]["confidence"], "exact")

    def test_methods_and_decorators(self):
        facts = {s["qualifiedName"]: s for s in self.facts("""
class Client:
    def send(self, body: str) -> bool: return True
    @classmethod
    def open(cls, key: str) -> bool: return True
    @staticmethod
    def parse(self: str) -> bool: return True
    @property
    def value(self) -> int: return 1
@wraps
def wrapped(value: int) -> int: return value
class Derived(Client):
    def send(self, body: str) -> bool: return True
""")}
        for name in ("Client.send", "Client.open", "Client.parse"):
            self.assertEqual(len(facts[name]["signatures"][0]["parameters"]), 1)
            self.assertEqual(facts[name]["confidence"], "exact")
        for name in ("Client.value", "wrapped", "Derived.send"):
            self.assertEqual(facts[name]["confidence"], "incomplete")

    def test_uncertain_annotations_and_shadowing(self):
        for annotation in ('"User"', "Alias", "typing.Any", "Optional[int]", "(int, str)", "list[int, str]", "int"):
            prefix = "int = str\n" if annotation == "int" else ""
            fact = self.facts(prefix + f"def read(value: {annotation}): pass")[0]
            self.assertEqual(fact["signatures"][0]["parameters"][0]["typeFact"]["confidence"], "incomplete")
            self.assertEqual(fact["signatures"][0]["returnType"]["confidence"], "incomplete")
        fact = self.facts("from something import *\ndef read() -> str: return ''")[0]
        self.assertEqual(fact["signatures"][0]["returnType"]["confidence"], "incomplete")

    def test_duplicate_conditional_and_generator(self):
        facts = {s["name"]: s for s in self.facts("""
def twice(a: int) -> int: return a
def twice(b: str) -> str: return b
if enabled:
    def conditional() -> int: return 1
def stream() -> int: yield 1
def outer() -> int:
    def hidden(): pass
    return 1
""")}
        self.assertEqual(len(facts["twice"]["signatures"]), 2)
        self.assertEqual(facts["twice"]["confidence"], "incomplete")
        self.assertEqual(facts["conditional"]["confidence"], "incomplete")
        self.assertEqual(facts["stream"]["signatures"][0]["returnType"]["confidence"], "incomplete")
        self.assertNotIn("hidden", facts)

    def test_source_is_never_executed_and_utf8_locations(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "api.py").write_text("raise RuntimeError('must not execute')\ndef café(é: str) -> bool: return True\n")
            result = adapter.analyze({"workspaceRoot": folder})
            self.assertEqual(result["diagnostics"], [])
            symbol = result["graph"]["symbols"][0]
            parameter = symbol["signatures"][0]["parameters"][0]
            self.assertEqual(parameter["location"]["range"]["start"], {"line": 2, "column": 11})

    def test_discovery_errors_and_symlinks(self):
        with tempfile.TemporaryDirectory() as folder, tempfile.TemporaryDirectory() as outside:
            root = Path(folder)
            (Path(outside) / "secret.py").write_text("def secret(): pass")
            (root / "escape").symlink_to(outside, target_is_directory=True)
            self.assertEqual(adapter.analyze({"workspaceRoot": folder})["graph"]["files"], [])
            (root / "bad.py").write_text("def broken(")
            result = adapter.analyze({"workspaceRoot": folder})
            self.assertEqual(result["diagnostics"][0]["severity"], "error")
            with self.assertRaises(ValueError):
                adapter.analyze({"workspaceRoot": folder, "projects": ["tsconfig.json"]})
            with self.assertRaises(ValueError):
                adapter.analyze({"workspaceRoot": folder, "sourceGlobs": ["../*.py"]})

    def test_rebinding_cannot_retain_exact_symbol_evidence(self):
        for tail in ("read = other", "from other import read"):
            self.assertEqual(self.facts("def read() -> str: return ''\n" + tail)[0]["confidence"], "incomplete")
        facts = self.facts("class Client:\n    def read(self) -> str: return ''\nClient.read = other")
        self.assertEqual(next(s for s in facts if s["qualifiedName"] == "Client.read")["confidence"], "incomplete")
        facts = self.facts("class Client:\n    def read(self) -> str: return ''\nclass Client: pass")
        self.assertEqual(next(s for s in facts if s["qualifiedName"] == "Client.read")["confidence"], "incomplete")

    def test_cli_python_binding_in_workspace_with_tsconfig(self):
        binary = Path(__file__).resolve().parents[2] / "target/debug/vibedoc"
        self.assertTrue(binary.exists(), "Build the CLI before running adapter tests")
        command = Path(__file__).resolve().parent / "bin/vibedoc-adapter-python"
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "tsconfig.json").write_text("{}")
            (root / "vibedoc.toml").write_text('version = 1')
            (root / "api.py").write_text("def read(value: str) -> bool: return True")
            (root / "api.md").write_text("""<!-- vibedoc:source adapter="python" path="api.py" symbol="read" -->
# Read
## Parameters
- `value`: `str`
## Returns
- `bool`
""")
            run = subprocess.run([str(binary), "check", "api.md", "--profile", "reference",
                                  "--format", "json", "--adapter-command", f"python={command}"],
                                 cwd=root, capture_output=True, text=True, check=True)
            self.assertEqual(json.loads(run.stdout)["verification"]["verifiedStructuralClaims"], 3)

    def test_native_class_member_returns_and_ambiguity(self):
        binary = Path(__file__).resolve().parents[2] / "target/debug/vibedoc"
        command = Path(__file__).resolve().parent / "bin/vibedoc-adapter-python"
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "vibedoc.toml").write_text("""version = 1
[[documents]]
include = ["api.md"]
profile = "reference"
adapters = ["python"]
[adapters.python]
sources = ["*.py"]
""")
            (root / "api.py").write_text("""class Response:
    def read(self) -> bytes: return b""
    def take(self, key: int) -> bytes: return b""
class Other:
    def read(self) -> str: return ""
""")
            source = """## §Response§

* §def .read()§ - **bytes**
* §def .take(key)§ - **bytes**
* §.status§ - **int**
* §def __init__(...)§ - **None**

~~~md
* §def .read()§ - **str**
~~~

> * §def .read()§ - **str**
""".replace("§", chr(96))

            def check(text):
                (root / "api.md").write_text(text)
                # Config discovery selects Python; no explicit path overrides.
                run = subprocess.run([str(binary), "check", "--format", "json",
                                      "--adapter-command", f"python={command}"],
                                     cwd=root, capture_output=True, text=True)
                self.assertIn(run.returncode, (0, 1), run.stderr)
                return json.loads(run.stdout)

            good = check(source)
            self.assertEqual(good["verification"]["verifiedStructuralClaims"], 2)
            self.assertEqual(good["diagnostics"], [])
            wrong = check(source.replace("**bytes**", "**str**", 1))
            self.assertEqual(wrong["verification"]["contradictedStructuralClaims"], 1)
            self.assertEqual(wrong["diagnostics"][0]["evidence"][0]["path"], "api.py")

            (root / "other.py").write_text("class Response:\n    def read(self) -> str: return ''")
            ambiguous = check(source)
            self.assertEqual(ambiguous["verification"]["verifiedStructuralClaims"], 0)
            self.assertTrue(any(d["ruleId"] == "VDOC-G002" for d in ambiguous["diagnostics"]))
            explicit = '<!-- vibedoc:source adapter="python" path="api.py" symbol="Response" -->\n'
            self.assertEqual(check(explicit + source)["verification"]["verifiedStructuralClaims"], 2)
            (root / "other.py").unlink()

            (root / "api.py").write_text("class Response(Base):\n    def read(self) -> bytes: return b''")
            uncertain = check(source)
            self.assertEqual(uncertain["verification"]["verifiedStructuralClaims"], 0)
            self.assertEqual(uncertain["verification"]["unverifiedStructuralClaims"], 2)

    def test_stdio_roundtrip_and_recovery(self):
        command = Path(__file__).parent / "bin/vibedoc-adapter-python"
        requests = [
            "{invalid",
            json.dumps({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": 1}}),
            json.dumps({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}),
        ]
        run = subprocess.run([str(command), "--stdio"], input="\n".join(requests) + "\n",
                             text=True, capture_output=True, check=True)
        replies = [json.loads(line) for line in run.stdout.splitlines()]
        self.assertIn("error", replies[0])
        self.assertEqual(replies[1]["result"]["adapter"]["name"], "python")
        self.assertEqual(replies[2]["result"], {})


if __name__ == "__main__":
    unittest.main()
