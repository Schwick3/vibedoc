# Homebrew formula validation

Validated September 11, 2026 on Apple Silicon macOS (`arm64`) with Homebrew
`6.0.22-237-g6b0073f`.

The formula was generated from the working-tree generator using the published
Vibedoc v0.1.2 `SHA256SUMS`, downloaded with `gh release download`. It was placed
in the temporary local tap `codex/vibedoc-validation` for validation.

## Results

- Ruby syntax validation (`ruby -c`): passed.
- First `brew audit --strict`: failed because `url` appeared after `license`.
- Generator corrected to emit `url`, `sha256`, then `license`; the corresponding
  release-tool test was updated.
- Strict audit on the corrected formula: passed with no findings.
- After explicit approval to trust the temporary tap, `brew readall --os=all
  --arch=all codex/vibedoc-validation`: passed.
- `brew install codex/vibedoc-validation/vibedoc`: passed, including downloads
  and checksum verification of the CLI and TypeScript adapter resource.
- Homebrew installed Node.js 26.8.1 and its required dependencies.
- `brew test codex/vibedoc-validation/vibedoc`: passed (`--version`, `doctor`,
  and `check` against the formula's reference-document fixture).
- The installed CLI reported `vibedoc 0.1.2`.
- Against `tests/projects/task-service`, installed `doctor` loaded 4 source files
  and 14 symbols. Installed `check` verified 14 structural claims across two
  documents, with zero errors, warnings, contradictions, or unverified claims.

For the task-service run, `/opt/homebrew/bin` was placed first on `PATH` to ensure
that the packaged Node.js and adapter were used instead of the existing NVM
installation's development adapter.

The temporary Vibedoc installation, the 15 dependencies installed only for this
validation, the validation tap, and its trust entries were removed afterward.
The installed Homebrew formula list was compared against its pre-test snapshot.

This validates installation and runtime behavior on Apple Silicon macOS. The
platform-definition checks do not substitute for actual installations on Linux;
the existing release workflows cover those targets. This tested published v0.1.2
artifacts, not an unreleased CLI containing the new zero-coverage warning.
No release or public tap was modified.
