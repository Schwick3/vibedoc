# Roadmap

Vibedoc's initial release is distributed as native macOS and Linux archives and
through a project-owned Homebrew tap. The TypeScript adapter is packaged
separately but installed with the main Homebrew formula. The Python adapter is
also available, with conservative source-only annotation verification.

Per-user `adapter install`, `adapter list`, and `adapter remove` commands now
support Python and TypeScript using HTTPS or local manifests. Existing runtimes
are required; unpublished versions can be tested with local packages. Python
verification includes same-file inheritance plus bounded project-relative type
imports, assignment aliases, and selected typing forms.

The following items are explicit candidates for later milestones:

- CLI-managed `adapter update` and project-specific version selection.
- Broader Python type resolution, including forward references, additional
  typing forms, absolute imports, re-exports, and document-side aliases, each
  selected using measured evaluation gaps. Cross-file inheritance remains later work.
- A combined npm distribution only if demand justifies an intermediate,
  JavaScript-focused distribution stage.
- Additional package managers and a standalone shell installer that consume
  the existing release artifacts.
- Intel macOS release artifacts and Homebrew installation support if user
  demand justifies the additional release target.
- Artifact signing and provenance attestations.
- SARIF output for code-scanning integrations.
- Automatic fixes, caching, editor integration, and Windows support.
- Strict standards profiles and language adapters beyond TypeScript,
  JavaScript, and Python.
- Documentation generation and LLM-based semantic verification.
- Framework-specific source support for Vue and Svelte components.

The v1 architecture keeps these additions possible without placing the
TypeScript compiler or an LLM inside the Rust rule engine.
