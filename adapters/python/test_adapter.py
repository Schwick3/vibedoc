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
        for name in ("Client.send", "Client.open", "Client.parse", "Derived.send"):
            self.assertEqual(len(facts[name]["signatures"][0]["parameters"]), 1)
            self.assertEqual(facts[name]["confidence"], "exact")
        for name in ("Client.value", "wrapped"):
            self.assertEqual(facts[name]["confidence"], "incomplete")

    def test_local_inheritance_chain_and_receivers(self):
        facts = {s["qualifiedName"]: s for s in self.facts("""
class Base(object):
    def inherited(self) -> bool: return True
class Middle(Base):
    def direct(self, value: int) -> int: return value
class Child(Middle):
    async def fetch(self, key: str) -> str: return key
    @classmethod
    def create(cls, key: str) -> str: return key
    @staticmethod
    def parse(value: str) -> str: return value
""")}
        for name in ("Base", "Middle", "Child", "Middle.direct", "Child.fetch", "Child.create", "Child.parse"):
            self.assertEqual(facts[name]["confidence"], "exact")
        for name in ("Child.fetch", "Child.create", "Child.parse"):
            self.assertEqual(len(facts[name]["signatures"][0]["parameters"]), 1)
        self.assertNotIn("Child.inherited", facts)
        self.assertNotIn("Child.direct", facts)

    def test_unrelated_nested_class_names_keep_existing_confidence(self):
        facts = {s["qualifiedName"]: s for s in self.facts("""
class First:
    class Nested:
        def read(self) -> bytes: return b""
class Second:
    class Nested:
        def read(self) -> bytes: return b""
""")}
        for name in ("First.Nested", "First.Nested.read", "Second.Nested", "Second.Nested.read"):
            self.assertEqual(facts[name]["confidence"], "exact")

    def test_unsupported_base_forms_propagate(self):
        cases = {
            "import": "from external import Base",
            "conditional": "if enabled:\n    class Base: pass",
            "duplicate": "class Base: pass\nclass Base: pass",
            "rebound": "class Base: pass\nBase = other",
            "nested": "class Outer:\n    class Base: pass",
            "unresolved": "",
            "decorated": "@decorate\nclass Base: pass",
            "metaclass": "class Base(metaclass=Meta): pass",
            "keyword": "class Base(option=True): pass",
            "late": "",
            "cycle": "class Base(Other): pass\nclass Other(Base): pass",
        }
        for label, prefix in cases.items():
            with self.subTest(label=label):
                source = prefix + "\nclass Child(Base):\n    def read(self) -> bytes: return b''\nclass Leaf(Child):\n    def read(self) -> bytes: return b''"
                if label == "late":
                    source += "\nclass Base: pass"
                facts = {s["qualifiedName"]: s for s in self.facts(source)}
                for name in ("Child", "Child.read", "Leaf", "Leaf.read"):
                    self.assertEqual(facts[name]["confidence"], "incomplete")
        for base in ("Base, Other", "Base[int]", "module.Base", "factory()", "Outer.Base"):
            with self.subTest(base=base):
                facts = {s["qualifiedName"]: s for s in self.facts(
                    "class Base: pass\nclass Other: pass\nclass Outer:\n    class Base: pass\n"
                    + f"class Child({base}):\n    def read(self) -> bytes: return b''")}
                self.assertEqual(facts["Child.read"]["confidence"], "incomplete")

    def test_shadowed_object_and_nested_subclasses(self):
        for binding in ("object = other", "from external import object", "class object: pass", "from external import *"):
            facts = {s["qualifiedName"]: s for s in self.facts(
                binding + "\nclass Child(object):\n    def read(self) -> bytes: return b''")}
            self.assertEqual(facts["Child.read"]["confidence"], "incomplete")
        facts = {s["qualifiedName"]: s for s in self.facts(
            "class Base: pass\nclass Outer:\n    class Child(Base):\n        def read(self) -> bytes: return b''")}
        self.assertEqual(facts["Outer.Child.read"]["confidence"], "incomplete")

    def test_ancestor_hooks_and_late_rebinding(self):
        for hook in ("__init_subclass__", "__getattribute__", "__getattr__"):
            for body in (f"def {hook}(self): pass", f"{hook} = replacement"):
                with self.subTest(hook=hook, body=body):
                    facts = {s["qualifiedName"]: s for s in self.facts(
                        f"class Base:\n    {body}\nclass Middle(Base): pass\nclass Child(Middle):\n    def read(self) -> bytes: return b''")}
                    self.assertEqual(facts["Child.read"]["confidence"], "incomplete")
        for tail in ("Base = other", "del Base", "from external import Base", "class Base: pass",
                     "Base.__init_subclass__ = replacement", "match value:\n    case Base: pass"):
            with self.subTest(tail=tail):
                facts = {s["qualifiedName"]: s for s in self.facts(
                    "class Base: pass\nclass Child(Base):\n    def read(self) -> bytes: return b''\n" + tail)}
                self.assertEqual(facts["Child"]["confidence"], "incomplete")
                self.assertEqual(facts["Child.read"]["confidence"], "incomplete")

    @unittest.skipUnless(hasattr(__import__("ast"), "TypeVar"), "Python 3.12 generic syntax")
    def test_generic_ancestors_remain_incomplete(self):
        facts = {s["qualifiedName"]: s for s in self.facts(
            "class Base[T]: pass\nclass Child(Base):\n    def read(self) -> bytes: return b''")}
        self.assertEqual(facts["Child.read"]["confidence"], "incomplete")

    def test_method_uncertainty_is_preserved_on_supported_subclasses(self):
        facts = {s["qualifiedName"]: s for s in self.facts("""
class Base: pass
class Child(Base):
    @unknown
    def wrapped(self) -> int: return 1
    @property
    def value(self) -> int: return 1
    @overload
    def repeated(self, a: int) -> int: ...
    def repeated(self, a: str) -> str: return a
    def replaced(self) -> int: return 1
Child.replaced = replacement
""")}
        self.assertEqual(facts["Child"]["confidence"], "exact")
        for name in ("wrapped", "value", "repeated", "replaced"):
            self.assertEqual(facts["Child." + name]["confidence"], "incomplete")

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
            (root / "api.py").write_text("""class Base: pass
class Response(Base):
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

            method_doc = """<!-- vibedoc:source adapter="python" path="api.py" symbol="Response.take" -->
# Take
## Parameters
- §key§: §int§
## Returns
- §bytes§
""".replace("§", chr(96))
            self.assertEqual(check(method_doc)["verification"]["verifiedStructuralClaims"], 3)
            wrong_parameter = check(method_doc.replace(chr(96) + "key" + chr(96), chr(96) + "missing" + chr(96)))
            self.assertEqual(wrong_parameter["verification"]["contradictedStructuralClaims"], 1)
            parameter_error = next(d for d in wrong_parameter["diagnostics"] if d["ruleId"] == "VDOC-G003")
            self.assertEqual(parameter_error["evidence"][0]["path"], "api.py")

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

    def project_facts(self, sources, globs=None):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for name, source in sources.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(source)
            return adapter.analyze({"workspaceRoot": folder, "sourceGlobs": globs or ["**/*.py"]})

    def return_fact(self, source, name="read"):
        return next(s for s in self.facts(source) if s["qualifiedName"] == name)["signatures"][0]["returnType"]

    def test_local_aliases_and_typing_constructors(self):
        cases = {
            "from typing import Optional\nAlias = Optional[str]": "None | str",
            "import typing as t\nAlias = t.List[t.Optional[str]]": "list[None | str]",
            "from typing import Union as U, Tuple as T\nAlias = U[str, T[int, bool]]": "str | tuple[int, bool]",
            "from typing import Dict, Set, FrozenSet\nAlias = Dict[str, Set[FrozenSet[int]]]": "dict[str, set[frozenset[int]]]",
            "Original = list[str | None]\nAlias = Original": "list[None | str]",
            "class Reply: pass\nAlias = Reply": "src.api.Reply",
        }
        for prefix, normalized in cases.items():
            with self.subTest(prefix=prefix):
                fact = self.return_fact(prefix + "\ndef read() -> Alias: pass")
                self.assertEqual(fact, {"display": "Alias", "normalized": normalized, "confidence": "exact"})

    def test_relative_imports_and_distinct_class_identities(self):
        result = self.project_facts({
            "pkg/__init__.py": "raise RuntimeError('not executed')",
            "pkg/models.py": "class Reply: pass\nAlias = list[Reply]",
            "pkg/other.py": "class Reply: pass",
            "pkg/sub/__init__.py": "",
            "pkg/sub/api.py": "from ..models import Reply as First, Alias\nfrom ..other import Reply as Second\ndef read(a: First, b: Second) -> Alias: pass",
        })
        self.assertEqual(result["diagnostics"], [])
        symbol = next(s for s in result["graph"]["symbols"] if s["qualifiedName"] == "read")
        signature = symbol["signatures"][0]
        self.assertEqual([p["typeFact"]["normalized"] for p in signature["parameters"]], ["pkg.models.Reply", "pkg.other.Reply"])
        self.assertEqual(signature["returnType"]["normalized"], "list[pkg.models.Reply]")
        self.assertTrue(all(p["typeFact"]["confidence"] == "exact" for p in signature["parameters"]))
        self.assertEqual([s["kind"] for s in result["graph"]["symbols"]].count("typeAlias"), 0)

    def test_package_targets_and_unrelated_import_cycles(self):
        result = self.project_facts({
            "pkg/__init__.py": "",
            "pkg/models/__init__.py": "from ..api import read\nclass Reply: pass",
            "pkg/api.py": "from .models import Reply\ndef read() -> Reply: pass",
        })
        self.assertEqual(result["diagnostics"], [])
        fact = next(s for s in result["graph"]["symbols"] if s["name"] == "read")["signatures"][0]["returnType"]
        self.assertEqual(fact["normalized"], "pkg.models.Reply")
        self.assertEqual(fact["confidence"], "exact")

    def test_import_boundaries_and_failures(self):
        base = {"pkg/__init__.py": "", "pkg/models.py": "class Reply: pass",
                "pkg/api.py": "from .models import Reply\ndef read() -> Reply: pass"}
        cases = [
            ({"pkg/models.py": "def broken("}, None),
            ({}, ["pkg/__init__.py", "pkg/api.py"]),
            ({}, ["pkg/models.py", "pkg/api.py"]),
            ({"pkg/models/__init__.py": "class Reply: pass"}, None),
            ({"pkg/models/__init__.py": "def broken("}, None),
            ({"pkg/models.py": "from .other import Reply", "pkg/other.py": "class Reply: pass"}, None),
            ({"pkg/api.py": "from pkg.models import Reply\ndef read() -> Reply: pass"}, None),
            ({"pkg/api.py": "import pkg.models as m\ndef read() -> m.Reply: pass"}, None),
            ({"pkg/models.py": "if enabled:\n    class Reply: pass"}, None),
            ({"pkg/models.py": "class Reply: pass\nReply = object"}, None),
            ({"pkg/models.py": "from .api import Alias\nReply = Alias", "pkg/api.py": "from .models import Reply\nAlias = Reply\ndef read() -> Alias: pass"}, None),
        ]
        for changes, globs in cases:
            with self.subTest(changes=changes, globs=globs):
                result = self.project_facts(base | changes, globs)
                fact = next(s for s in result["graph"]["symbols"] if s["name"] == "read")["signatures"][0]["returnType"]
                self.assertEqual(fact["confidence"], "incomplete")
                if any("broken(" in source for source in changes.values()):
                    self.assertTrue(any(d["code"] == "PY001" for d in result["diagnostics"]))

    def test_alias_rebinding_and_forward_declarations_abstain(self):
        cases = [
            "Alias = int\nAlias = str\ndef read() -> Alias: pass",
            "Alias = int\ndef read() -> Alias: pass\nAlias = str",
            "Alias = int\ndef other(Alias): pass\ndef read() -> Alias: pass",
            "if flag:\n    Alias = int\ndef read() -> Alias: pass",
            "from typing import TYPE_CHECKING\nif TYPE_CHECKING:\n    from .models import Alias\ndef read() -> Alias: pass",
            "Alias = Later\nLater = int\ndef read() -> Alias: pass",
            "def read() -> Alias: pass\nAlias = int",
            "A = B\nB = A\ndef read() -> A: pass",
            "from anything import *\nAlias = int\ndef read() -> Alias: pass",
            "@decorate\nclass Reply: pass\ndef read() -> Reply: pass",
            "class Reply(metaclass=Meta): pass\ndef read() -> Reply: pass",
            "class Reply(Missing): pass\ndef read() -> Reply: pass",
            "Alias = int\nmatch data:\n    case {'x': Alias}: pass\ndef read() -> Alias: pass",
            "Alias = int\ndel Alias\ndef read() -> Alias: pass",
        ]
        for source in cases:
            with self.subTest(source=source):
                self.assertEqual(self.return_fact(source)["confidence"], "incomplete")

    def test_typing_shadowing_and_unsupported_forms_abstain(self):
        cases = [
            "import typing as t\nt = replacement\ndef read() -> t.Optional[str]: pass",
            "import typing as t\nt.Optional = replacement\ndef read() -> t.Optional[str]: pass",
            "from typing import Optional as O\nO = other\ndef read() -> O[str]: pass",
            "import typing as t\nt.Optional = other\nfrom typing import Optional as O\ndef read() -> O[str]: pass",
            "from typing import Optional\ndef other(Optional): pass\ndef read() -> Optional[str]: pass",
            "from typing import List\nint = str\ndef read() -> List[int]: pass",
            "from typing import Optional\ndef read() -> Optional[Unknown]: pass",
            "from typing import Union\ndef read() -> Union[int]: pass",
            "from typing import Tuple\ndef read() -> Tuple[int, ...]: pass",
            "from typing import Dict\ndef read() -> Dict[str]: pass",
            "from typing import List\ndef read() -> List[str, int]: pass",
            "from typing import Optional\ndef read() -> Optional[str, int]: pass",
            "from typing import List\nAlias = List\ndef read() -> Alias[str]: pass",
            "Alias = list[str]\ndef read() -> Alias[int]: pass",
            "Alias = factory()\ndef read() -> Alias: pass",
            "Alias: type = str\ndef read() -> Alias: pass",
            "class Reply: pass\ndef read() -> 'Reply': pass",
        ]
        for form in ("Any", "IO[str]", "Callable[[str], int]", "Iterator[str]", "Mapping[str, int]", "Sequence[str]"):
            cases.append(f"import typing\ndef read() -> typing.{form}: pass")
        for source in cases:
            with self.subTest(source=source):
                self.assertEqual(self.return_fact(source)["confidence"], "incomplete")
        result = self.project_facts({"typing.py": "Optional = object", "api.py": "from typing import Optional\ndef read() -> Optional[str]: pass"})
        self.assertEqual(next(s for s in result["graph"]["symbols"] if s["name"] == "read")["signatures"][0]["returnType"]["confidence"], "incomplete")

    def test_resolution_depth_is_bounded_and_cache_independent(self):
        chain = "A0 = int\n" + "".join(f"A{i} = A{i-1}\n" for i in range(1, 70))
        for prefix in ("", "def short() -> A10: pass\n"):
            self.assertEqual(self.return_fact(chain + prefix + "def read() -> A69: pass")["confidence"], "incomplete")
        self.assertEqual(self.return_fact(chain + "def read() -> A10: pass")["normalized"], "int")
        expanding = "A0 = int\n" + "".join(f"A{i} = tuple[A{i-1}, A{i-1}]\n" for i in range(1, 30))
        self.assertEqual(self.return_fact(expanding + "def read() -> A29: pass")["confidence"], "incomplete")


    def test_relative_import_does_not_follow_external_symlinks(self):
        with tempfile.TemporaryDirectory() as folder, tempfile.TemporaryDirectory() as outside:
            root = Path(folder)
            (root / "pkg").mkdir()
            (root / "pkg/__init__.py").write_text("")
            (Path(outside) / "models.py").write_text("class Reply: pass")
            (root / "pkg/models.py").symlink_to(Path(outside) / "models.py")
            (root / "pkg/api.py").write_text("from .models import Reply\ndef read() -> Reply: pass")
            result = adapter.analyze({"workspaceRoot": folder})
            self.assertNotIn("pkg/models.py", [f["path"] for f in result["graph"]["files"]])
            self.assertEqual(result["graph"]["symbols"][0]["signatures"][0]["returnType"]["confidence"], "incomplete")

    def test_cli_imported_types_aliases_and_native_member_returns(self):
        binary = Path(__file__).resolve().parents[2] / "target/debug/vibedoc"
        command = Path(__file__).resolve().parent / "bin/vibedoc-adapter-python"
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "pkg").mkdir()
            (root / "pkg/__init__.py").write_text("")
            (root / "pkg/types.py").write_text("from typing import Optional\nclass Response: pass\nLabel = Optional[str]\n")
            (root / "pkg/api.py").write_text("from .types import Label, Response as Reply\ndef load(value: Label) -> Reply: pass\nclass Client:\n    def read(self) -> Reply: pass\n")
            (root / "vibedoc.toml").write_text('version = 1\n[adapters.python]\nsources = ["pkg/**/*.py"]\n')
            doc = '''<!-- vibedoc:source adapter="python" path="pkg/api.py" symbol="load" -->
# Read
## Parameters
- `value`: `Label`
## Returns
- `Reply`
'''
            def check(source, counts):
                (root / "api.md").write_text(source)
                run = subprocess.run([str(binary), "check", "api.md", "--profile", "reference", "--format", "json",
                                      "--adapter-command", f"python={command}"], cwd=root, capture_output=True, text=True)
                self.assertEqual(run.returncode, 1 if counts[1] else 0, run.stderr + run.stdout)
                report = json.loads(run.stdout)
                self.assertEqual([report["verification"][k] for k in ("verifiedStructuralClaims", "contradictedStructuralClaims", "unverifiedStructuralClaims")], counts)
                return report
            check(doc, [3, 0, 0])
            check(doc.replace("`Label`", "`str | None`").replace("`Reply`", "`pkg.types.Response`"), [3, 0, 0])
            wrong = check(doc.replace("`Label`", "`int`"), [2, 1, 0])
            error = next(d for d in wrong["diagnostics"] if d["ruleId"] == "VDOC-G005")
            self.assertEqual(error["evidence"][0]["path"], "pkg/api.py")
            self.assertEqual(error["evidence"][0]["range"]["start"], {"line": 2, "column": 10})
            wrong = check(doc.replace("`Reply`", "`str`"), [2, 1, 0])
            error = next(d for d in wrong["diagnostics"] if d["ruleId"] == "VDOC-G006")
            self.assertEqual(error["evidence"][0]["range"]["start"], {"line": 2, "column": 1})
            check(doc.replace("`Label`", "`AnotherAlias`"), [2, 0, 1])
            check(doc.replace("`Reply`", "`other.Response`"), [2, 0, 1])
            native = '<!-- vibedoc:source adapter="python" path="pkg/api.py" symbol="Client" -->\n## `Client`\n\n* `def .read()` - **Reply**\n'
            check(native, [1, 0, 0])
            wrong = check(native.replace("**Reply**", "**str**"), [0, 1, 0])
            error = next(d for d in wrong["diagnostics"] if d["ruleId"] == "VDOC-G006")
            self.assertEqual(error["evidence"][0]["range"]["start"], {"line": 4, "column": 5})
            check(native.replace("**Reply**", "**OtherReply**"), [0, 0, 1])

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
