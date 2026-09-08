# Adapter protocol version 1

Vibedoc adapters communicate through newline-delimited JSON-RPC 2.0 on standard
input and standard output. Each line contains one complete JSON object. An
adapter must write logs only to standard error.

The core starts an adapter with `--stdio`. Requests occur in this order:

1. `initialize`
2. zero or more `analyze` requests
3. `shutdown`

## Initialize

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"coreVersion":"0.1.0","workspaceRoot":"/repo"}}
```

The result declares the negotiated version, adapter identity, runtime, source
extensions, languages, and relationships.

```json
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"adapter":{"name":"typescript","version":"0.1.0","runtime":"node 22; typescript 6.0.3"},"capabilities":{"languages":["typescript","tsx","javascript","jsx"],"extensions":[".ts",".tsx",".js",".jsx"],"relationships":["calls","writes","extends","implements"]}}}
```

## Analyze

The request identifies the workspace root, explicit project configurations,
and fallback source globs.

```json
{"jsonrpc":"2.0","id":2,"method":"analyze","params":{"workspaceRoot":"/repo","projects":["/repo/tsconfig.json"],"sourceGlobs":[]}}
```

The result contains `graph` and `diagnostics`. The graph contains `files`,
`symbols`, and `relationships`. Paths inside evidence locations are
repository-relative and use `/` separators. Lines and UTF-8 byte columns are
one-based.

The TypeScript adapter accepts `.ts`, `.tsx`, `.js`, and `.jsx`. It uses the
compiler options in each supplied `tsconfig.json` or `jsconfig.json` and uses
the compiler implementation pinned with the adapter.

## Shutdown

```json
{"jsonrpc":"2.0","id":3,"method":"shutdown","params":{}}
```

The adapter responds with an empty object and exits successfully.

## Failures

JSON-RPC errors use the standard `error` object. An adapter can also return
source-analysis diagnostics from `analyze`. An error-level adapter diagnostic
causes an operational failure. Warnings and informational messages are written
to standard error by the CLI.
