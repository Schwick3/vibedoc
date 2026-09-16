# Managed adapters

On macOS and Linux, Vibedoc can install the Python and TypeScript adapters in a
per-user store shared across projects. Install Node 22+ for TypeScript or Python
3.10+ (available as `python3`) for Python first. Vibedoc does not install runtimes,
run npm/pip, or execute installation hooks. TypeScript's compiler dependency is
bundled; Python uses the standard library and its isolated launcher.

```sh
vibedoc adapter install python
vibedoc adapter install typescript --version 0.1.2
vibedoc adapter list --format json
vibedoc adapter remove python
```

All three commands support `--format text|json`. Operational failures return 2.
Installation defaults to the CLI's exact version, using
`https://github.com/Schwick3/vibedoc/releases/download/vVERSION/adapters.json`.
Only exact stable versions are accepted; there is no `latest` lookup or automatic
update. Existing releases without manifests cannot be installed this way.

## Local validation before publication

From a built source checkout with npm dependencies installed:

```sh
./scripts/package-typescript-adapter.sh 0.1.2 target/adapter-artifacts
./scripts/package-python-adapter.sh 0.1.2 target/adapter-artifacts
node scripts/write-adapter-manifest.mjs 0.1.2 target/adapter-artifacts
vibedoc adapter install python --manifest target/adapter-artifacts/adapters.json
vibedoc adapter install typescript --manifest target/adapter-artifacts/adapters.json
```

`--manifest` accepts a local path or HTTPS URL and conflicts with `--version`.
It is an explicit trust decision: the installed executable runs during the
validation probe and subsequent source analysis. SHA-256 detects corruption;
it is not an independent signature for an untrusted manifest.

## Discovery and recovery

Discovery uses an explicit absolute `--adapter-command` first, the active managed
installation second, then `vibedoc-adapter-NAME` on PATH. Project commands never
download adapters. Repository configuration cannot select executable paths or
change the managed store. A managed TypeScript installation takes precedence over
Homebrew's adapter; removal restores PATH discovery without modifying Homebrew.

The store defaults to `~/Library/Application Support/vibedoc/adapters` on macOS
and `${XDG_DATA_HOME:-$HOME/.local/share}/vibedoc/adapters` on Linux. Override it
with an absolute `VIBEDOC_ADAPTER_HOME`. `list` reads records and PATH without
executing adapters, downloading, or creating a store. It reports recorded
versions, the active executable, and any shadowed PATH fallback.

Installation verifies runtime, archive checksum, safe extraction, adapter
identity/version, and protocol v1 before atomically activating a new record.
Failures leave the previous activation intact. Repeating an installation checks
the recorded files and repairs damage through staging. A previously recorded
version cannot acquire a different checksum. Downloaded archives are cached and
rechecked before reuse; manifest retrieval still requires its original source.
Older directories are retained so activation does not remove files used by
running checks. `remove` deletes all managed versions and cache for that adapter;
finish running checks before removing their adapter.

Broken active metadata causes an error rather than silently selecting PATH.
Reinstall to repair damaged files; use `remove` then `install` for corrupt registry
metadata. Explicit command overrides remain available. Removal is idempotent and
never deletes PATH, npm, Homebrew, or development installations.

## Manifest v1

The packaging script generates this shape (the SHA-256 below is illustrative):

```json
{
  "schemaVersion": 1,
  "adapters": [{
    "name": "python",
    "version": "0.1.2",
    "protocolVersion": 1,
    "runtime": { "name": "python3", "minVersion": "3.10.0" },
    "archive": "vibedoc-adapter-python-v0.1.2.tar.gz",
    "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
    "executable": "bin/vibedoc-adapter-python"
  }]
}
```

TypeScript uses runtime `node` with minimum `22.0.0` and executable
`dist/index.js`. Unknown fields, duplicate/unsupported adapters, unsupported
protocols, invalid versions/hashes, and escaping executable paths are rejected.
Relative artifact locations resolve against the manifest. Local manifests can
reference local files or HTTPS URLs; remote manifests can reference relative
URLs or absolute HTTPS URLs, never local files. TLS verification is always on,
with up to five HTTPS redirects, a 30-second connection timeout and a five-minute
transfer timeout. Limits are 1 MiB per manifest, 128 MiB per compressed archive,
512 MiB of extracted data and 20,000 entries. Links, special files, traversal,
absolute paths, and duplicate normalized archive paths are rejected.

Per-project pinning, Windows, runtime downloads, automatic updates, and third-party
adapter catalogs are not supported.
