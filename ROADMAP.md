# Roadmap

Vibedoc's initial release is distributed as native macOS and Linux archives and
through a project-owned Homebrew tap. The TypeScript adapter is packaged
separately but installed with the main Homebrew formula.

The following items are explicit candidates for later milestones:

- CLI-managed `adapter install`, `adapter update`, and `adapter remove`
  commands.
- A combined npm distribution only if demand justifies an intermediate,
  JavaScript-focused distribution stage.
- Additional package managers and a standalone shell installer that consume
  the existing release artifacts.
- Artifact signing and provenance attestations.
- SARIF output for code-scanning integrations.
- Automatic fixes, caching, editor integration, and Windows support.
- Strict standards profiles and language adapters beyond TypeScript and
  JavaScript.
- Documentation generation and LLM-based semantic verification.
- Framework-specific source support for Vue and Svelte components.

The v1 architecture keeps these additions possible without placing the
TypeScript compiler or an LLM inside the Rust rule engine.
