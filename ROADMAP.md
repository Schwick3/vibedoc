# Roadmap

Vibedoc v1 is a source-installable research prototype. The following items are
explicit candidates for later milestones:

- CLI-managed `adapter install`, `adapter update`, and `adapter remove`
  commands.
- Standalone macOS and Linux release binaries as the likely next distribution
  stage.
- A combined npm distribution only if demand justifies an intermediate,
  JavaScript-focused distribution stage.
- SARIF output for code-scanning integrations.
- Automatic fixes, caching, editor integration, and Windows support.
- Strict standards profiles and language adapters beyond TypeScript and
  JavaScript.
- Documentation generation and LLM-based semantic verification.
- Framework-specific source support for Vue and Svelte components.

The v1 architecture keeps these additions possible without placing the
TypeScript compiler or an LLM inside the Rust rule engine.
