import assert from "node:assert/strict";
import { test } from "node:test";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { analyzeWorkspace } from "./analyzer.js";

const fixture = fileURLToPath(
  new URL("../../../tests/fixtures/typescript-project/", import.meta.url),
);

test("analyzes TypeScript and JavaScript project facts", () => {
  const result = analyzeWorkspace({
    workspaceRoot: fixture,
    projects: [path.join(fixture, "tsconfig.json")],
    sourceGlobs: [],
  });

  assert.deepEqual(
    result.diagnostics.filter((diagnostic) => diagnostic.severity === "error"),
    [],
  );
  const extensions = new Set(result.graph.files.map((file) => path.extname(file.path)));
  assert.deepEqual(extensions, new Set([".ts", ".tsx", ".js", ".jsx"]));

  const login = result.graph.symbols.find(
    (symbol) => symbol.qualifiedName === "AuthenticationService.login",
  );
  assert.ok(login);
  assert.equal(login.exported, true);
  assert.equal(login.signatures[0]?.parameters[0]?.name, "email");
  assert.equal(login.signatures[0]?.parameters[1]?.optional, true);
  assert.equal(login.signatures[0]?.parameters[1]?.typeFact.confidence, "inferred");
  assert.equal(login.signatures[0]?.returnType.display, "Promise<User>");
  assert.equal(login.throws[0]?.typeName, "InvalidCredentialsError");
  assert.equal(login.declaration.path, "src/auth.ts");
  assert.equal(login.id, "typescript:src/auth.ts#AuthenticationService.login");
  assert.deepEqual(login.declaration.range.start, { line: 16, column: 3 });

  const restFunction = result.graph.symbols.find(
    (symbol) => symbol.qualifiedName === "joinValues",
  );
  assert.equal(restFunction?.signatures[0]?.parameters[1]?.rest, true);

  const greeting = result.graph.symbols.find(
    (symbol) => symbol.qualifiedName === "Greeting",
  );
  assert.equal(greeting?.signatures[0]?.parameters[0]?.destructured, true);

  const dynamicJavaScript = result.graph.symbols.find(
    (symbol) => symbol.qualifiedName === "normalize",
  );
  assert.equal(dynamicJavaScript?.signatures[0]?.parameters[0]?.typeFact.confidence, "incomplete");

  const defaultExport = result.graph.symbols.find(
    (symbol) => symbol.declaration.path === "src/default-export.ts",
  );
  assert.equal(defaultExport?.kind, "defaultExport");

  const reexported = result.graph.symbols.find(
    (symbol) => symbol.qualifiedName === "createUser",
  );
  assert.equal(reexported?.exported, true);

  assert.ok(
    result.graph.relationships.some(
      (relationship) =>
        relationship.from === login.id &&
        relationship.kind === "calls" &&
        relationship.display === "audit",
    ),
  );
  assert.ok(
    result.graph.relationships.some(
      (relationship) =>
        relationship.from === login.id && relationship.kind === "writes",
    ),
  );
});

test("merges overload declarations into one symbol", () => {
  const result = analyzeWorkspace({
    workspaceRoot: fixture,
    projects: [path.join(fixture, "tsconfig.json")],
    sourceGlobs: [],
  });
  const parseSymbols = result.graph.symbols.filter(
    (symbol) => symbol.qualifiedName === "parse",
  );
  assert.equal(parseSymbols.length, 1);
  assert.equal(parseSymbols[0]?.signatures.length, 3);
});

test("reports a missing project without throwing", () => {
  const result = analyzeWorkspace({
    workspaceRoot: fixture,
    projects: [path.join(fixture, "missing.json")],
    sourceGlobs: [],
  });
  assert.equal(result.diagnostics[0]?.code, "TSADAPTER001");
  assert.equal(result.diagnostics[0]?.severity, "error");
});

test("uses fallback source globs when no project is supplied", () => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "vibedoc-adapter-"));
  try {
    fs.mkdirSync(path.join(temporary, "src"));
    fs.writeFileSync(
      path.join(temporary, "src", "value.ts"),
      "export const value = (input: string): string => input;\n",
    );
    const result = analyzeWorkspace({
      workspaceRoot: temporary,
      projects: [],
      sourceGlobs: ["src/**/*.ts"],
    });
    assert.deepEqual(result.diagnostics, []);
    assert.equal(result.graph.symbols[0]?.qualifiedName, "value");
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});

test("reports invalid project configuration", () => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "vibedoc-adapter-"));
  try {
    fs.writeFileSync(path.join(temporary, "tsconfig.json"), "{ invalid json");
    const result = analyzeWorkspace({
      workspaceRoot: temporary,
      projects: [path.join(temporary, "tsconfig.json")],
      sourceGlobs: [],
    });
    assert.equal(result.diagnostics[0]?.severity, "error");
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});
