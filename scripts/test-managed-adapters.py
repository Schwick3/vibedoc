"""Install real packages in an isolated home; never use developer/PATH adapters."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/vibedoc"


def build(*args):
    subprocess.run(args, cwd=ROOT, check=True)


build("cargo", "build", "--workspace", "--locked")
version = subprocess.check_output([BINARY, "--version"], text=True).strip().split()[1]
with tempfile.TemporaryDirectory(prefix="vibedoc-managed-") as temporary:
    directory = Path(temporary)
    artifacts = directory / "artifacts"
    build(str(ROOT / "scripts/package-typescript-adapter.sh"), version, str(artifacts))
    build(str(ROOT / "scripts/package-python-adapter.sh"), version, str(artifacts))
    build("node", str(ROOT / "scripts/write-adapter-manifest.mjs"), version, str(artifacts))
    manifest = artifacts / "adapters.json"
    store = directory / "store"
    runtime_bin = directory / "bin"
    runtime_bin.mkdir()
    for command in ("node", "python3", "dirname"):
        (runtime_bin / command).symlink_to(shutil.which(command))
    env = dict(os.environ, VIBEDOC_ADAPTER_HOME=str(store), PATH=str(runtime_bin))

    def run(*args, cwd=directory, code=0, environment=env):
        completed = subprocess.run([str(BINARY), *args, "--format", "json"], cwd=cwd,
                                   env=environment, text=True, capture_output=True, timeout=60)
        assert completed.returncode == code, completed.stdout + completed.stderr
        return json.loads(completed.stdout)

    assert all(item["activePath"] is None for item in run("adapter", "list")["adapters"])
    assert not store.exists(), "listing must not create the store"
    for name in ("typescript", "python"):
        installed = run("adapter", "install", name, "--manifest", str(manifest))
        assert installed["adapter"]["version"] == version
        again = run("adapter", "install", name, "--manifest", str(manifest))
        assert installed["path"] == again["path"]

    ts_project = ROOT / "tests/projects/task-service"
    run("check", cwd=ts_project)
    run("doctor", cwd=ts_project)
    run("inspect", cwd=ts_project)

    project = directory / "python-project"
    project.mkdir()
    (project / "api.py").write_text('def greet(name: str) -> str:\n    return name\n')
    (project / "vibedoc.toml").write_text('''version = 1
[adapters.python]
sources = ["api.py"]
[[documents]]
include = ["reference.md"]
profile = "reference"
adapters = ["python"]
''')
    (project / "reference.md").write_text('''<!-- vibedoc:source adapter="python" path="api.py" symbol="greet" -->
# Greet

## Parameters
- `name`: `str`

## Returns
- `str`
''')
    report = run("check", cwd=project)
    assert report["verification"]["verifiedStructuralClaims"] == 3
    run("doctor", cwd=project)
    run("inspect", "--adapter", "python", cwd=project)
    (project / "reference.md").write_text((project / "reference.md").read_text().replace("`name`", "`wrong`"))
    assert run("check", cwd=project, code=1)["verification"]["contradictedStructuralClaims"] == 1

    # A failing PATH executable cannot shadow a valid managed adapter.
    fallback = runtime_bin / "vibedoc-adapter-python"
    fallback.write_text("#!/bin/sh\nexit 42\n")
    fallback.chmod(0o755)
    run("inspect", "--adapter", "python", cwd=project)
    listed = next(item for item in run("adapter", "list")["adapters"] if item["name"] == "python")
    assert listed["shadowed"] and listed["pathFallback"] == str(fallback)
    run("check", "--adapter-command", f"python={fallback}", cwd=project, code=2)

    # Broken metadata must not silently fall back, but explicit overrides bypass it.
    active = store / "python/active.json"
    saved = active.read_bytes()
    active.write_text("{}")
    run("inspect", "--adapter", "python", cwd=project, code=2)
    run("adapter", "list", code=2)
    explicit = ROOT / "adapters/python/bin/vibedoc-adapter-python"
    run("check", "--adapter-command", f"python={explicit}", cwd=project, code=1)
    active.write_bytes(saved)

    # Missing runtimes fail without changing activation.
    absent_runtime = dict(env, PATH=str(directory / "missing-runtime"))
    run("adapter", "install", "python", "--manifest", str(manifest), environment=absent_runtime, code=2)
    assert active.read_bytes() == saved
    isolated = dict(env, VIBEDOC_ADAPTER_HOME=str(directory / "other-store"))
    assert all(item["activePath"] is None for item in run("adapter", "list", environment=isolated)["adapters"])
    run("adapter", "list", environment=dict(env, VIBEDOC_ADAPTER_HOME="relative"), code=2)
    run("adapter", "install", "python", "--version", "latest", code=2)
    conflict = subprocess.run([str(BINARY), "adapter", "install", "python", "--version", version,
                               "--manifest", str(manifest)], env=env, capture_output=True)
    assert conflict.returncode == 2

    run("adapter", "remove", "python")
    run("adapter", "remove", "python")
    run("inspect", "--adapter", "python", cwd=project, code=2)
    assert fallback.exists() and not (store / "python").exists()
    # Replacing the PATH fixture now restores ordinary discovery after removal.
    import shlex
    fallback.write_text(f"#!/bin/sh\nexec {shlex.quote(str(explicit))} \"$@\"\n")
    run("inspect", "--adapter", "python", cwd=project)
    run("doctor", cwd=ts_project)
    run("adapter", "remove", "typescript")
    assert all(item["activePath"] is None for item in run("adapter", "list")["adapters"])
print("Managed adapter packages, discovery, verification, and removal passed")
