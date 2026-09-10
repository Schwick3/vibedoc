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

function analyzeSource(source: string) {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "vibedoc-facts-"));
  try {
    fs.writeFileSync(path.join(temporary, "source.ts"), source);
    return analyzeWorkspace({ workspaceRoot: temporary, projects: [], sourceGlobs: ["*.ts"] });
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
}

test("keeps namespace identities, overloads, and call targets distinct", () => {
  const { graph } = analyzeSource(`
    export namespace A {
      export function parse(value: string): string;
      export function parse(value: number): number;
      export function parse(value: string | number) { return value; }
      function hidden() {}
      export namespace Nested { export function parse(): boolean { return true; } }
    }
    export namespace B { export function parse(): number { return 1; } }
    export function run() { A.parse('a'); B.parse(); A.Nested.parse(); }
  `);
  const byName = new Map(graph.symbols.map((symbol) => [symbol.qualifiedName, symbol]));
  assert.equal(byName.get("A.parse")?.signatures.length, 3);
  assert.equal(byName.get("B.parse")?.signatures.length, 1);
  assert.equal(byName.get("A.Nested.parse")?.signatures[0]?.returnType.display, "boolean");
  assert.equal(byName.get("A.hidden")?.exported, false);
  assert.equal(new Set(graph.symbols.map((symbol) => symbol.id)).size, graph.symbols.length);
  for (const name of ["A.parse", "B.parse", "A.Nested.parse"]) {
    assert.ok(graph.relationships.some((relationship) =>
      relationship.display === name && relationship.to === byName.get(name)?.id));
  }
});

test("marks unresolved and nested dynamic types incomplete while preserving precise generics", () => {
  const { graph } = analyzeSource(`
    type Box<T> = { value: T };
    export function missing(value: MissingType): MissingType { return value; }
    export function nested(value: Promise<MissingType[]>): Promise<MissingType[]> { return value; }
    export function dynamic(value: Array<any>): Promise<unknown> { return Promise.resolve(value); }
    export function alias(value: Box<MissingType>): Box<MissingType> { return value; }
    export function union(value: string | Promise<any>): string | Promise<any> { return value; }
    export function tuple(value: [string, MissingType]): [string, MissingType] { return value; }
    export function precise(value: Promise<string[]>): Promise<string[]> { return value; }
    export function generic<T>(value: T): T { return value; }
    export function brokenThrow(): never { throw new MissingError(); }
    export function validThrow(): never { throw new Error('failure'); }
  `);
  for (const name of ["missing", "nested", "dynamic", "alias", "union", "tuple"]) {
    const signature = graph.symbols.find((symbol) => symbol.name === name)?.signatures[0];
    assert.equal(signature?.parameters[0]?.typeFact.confidence, "incomplete", name);
    assert.equal(signature?.returnType.confidence, "incomplete", name);
  }
  for (const name of ["precise", "generic"]) {
    const signature = graph.symbols.find((symbol) => symbol.name === name)?.signatures[0];
    assert.equal(signature?.parameters[0]?.typeFact.confidence, "exact", name);
    assert.equal(signature?.returnType.confidence, "exact", name);
  }
  assert.equal(graph.symbols.find((symbol) => symbol.name === "brokenThrow")?.throws[0]?.confidence, "incomplete");
  assert.equal(graph.symbols.find((symbol) => symbol.name === "validThrow")?.throws[0]?.confidence, "exact");
});

test("detects unresolved members in named, recursive, and callable shapes", () => {
  const result = analyzeSource(`
    interface Broken { payload: MissingPayload }
    interface Recursive { next?: Recursive; value: string }
    interface Callback { run: () => MissingPayload }
    export function broken(value: Broken): Broken { return value; }
    export function callback(value: Callback): Callback { return value; }
    export function recursive(value: Recursive): Recursive { return value; }
  `);
  for (const name of ["broken", "callback"]) {
    assert.equal(result.graph.symbols.find((s) => s.name === name)?.signatures[0]?.returnType.confidence, "incomplete");
  }
  assert.equal(result.graph.symbols.find((s) => s.name === "recursive")?.signatures[0]?.returnType.confidence, "exact");
  assert.ok(result.diagnostics.some((d) => d.code === "TS2304" && d.severity === "warning"));
});

test("resolves cross-project calls and rejects conflicting shared-file facts deterministically", () => {
  const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "vibedoc-projects-"));
  try {
    fs.writeFileSync(path.join(temporary, "shared.ts"), 'export function target(value: string | null): string | null { return value; }');
    fs.writeFileSync(path.join(temporary, "consumer.ts"), 'import { target } from "./shared"; export function call() { return target(null); }');
    const project = (name: string, strictNullChecks: boolean, include: string[]) => {
      const file = path.join(temporary, name);
      fs.writeFileSync(file, JSON.stringify({ compilerOptions: { strictNullChecks, target: "ES2022", module: "ESNext", moduleResolution: "Bundler" }, include }));
      return file;
    };
    const first = project("a.json", true, ["shared.ts"]);
    const second = project("b.json", true, ["consumer.ts"]);
    const params = { workspaceRoot: temporary, projects: [first, second], sourceGlobs: [] };
    const clean = analyzeWorkspace(params);
    assert.ok(clean.graph.relationships.some((r) => r.display === "target" && r.to === "typescript:shared.ts#target"));
    assert.equal(clean.graph.symbols.find((s) => s.name === "target")?.signatures.length, 1);
    assert.deepEqual(clean.diagnostics, []);
    project("b.json", false, ["consumer.ts"]);
    const conflict = analyzeWorkspace(params);
    assert.equal(conflict.graph.symbols.find((s) => s.name === "target")?.confidence, "incomplete");
    assert.ok(conflict.diagnostics.some((d) => d.code === "TSADAPTER004"));
    assert.deepEqual(analyzeWorkspace({ ...params, projects: [second, first] }), conflict);
  } finally {
    fs.rmSync(temporary, { recursive: true, force: true });
  }
});
