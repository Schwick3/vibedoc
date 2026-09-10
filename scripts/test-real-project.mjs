import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { analyzeWorkspace } from '../adapters/typescript/dist/analyzer.js';

// Deliberately opt-in: clone the pinned upstream revision separately, then run this script.
const revision = '6b41670516ed8e8b738612f60491995470aa63b3';
const repository = path.resolve(fileURLToPath(new URL('..', import.meta.url)));
const checkout = path.resolve(process.argv[2] ?? '');
assert.ok(process.argv[2], 'Usage: node scripts/test-real-project.mjs MITT_CHECKOUT [RESULT_JSON]');
const git = (...args) => execFileSync('git', ['-C', checkout, ...args], { encoding: 'utf8' }).trim();
assert.equal(git('rev-parse', 'HEAD'), revision, 'Use the pinned mitt revision');
assert.equal(git('status', '--porcelain', '--untracked-files=no'), '', 'Upstream tracked files must be clean');
const temporary = fs.mkdtempSync(path.join(checkout, '.vibedoc-evaluation-'));
const relative = path.basename(temporary);
const config = path.join(checkout, `${relative}.toml`);
const project = path.join(temporary, 'tsconfig.json');
let configCreated = false;
const results = { repository: 'https://github.com/developit/mitt', revision, runs: {} };

function write(name, text) {
  fs.writeFileSync(path.join(temporary, name), text);
  return `${relative}/${name}`;
}
function reference(symbol, body, source = 'src/index.ts') {
  return `<!-- vibedoc:source adapter="typescript" path="${source}" symbol="${symbol}" -->\n# \`${symbol}\`\n\n${body}\n`;
}
function check(name, file, profile, expectedExit) {
  const run = spawnSync(path.join(repository, 'target/debug/vibedoc'), [
    'check', file, '--config', config, '--profile', profile, '--format', 'json',
    '--adapter-command', `typescript=${path.join(repository, 'adapters/typescript/bin/vibedoc-adapter-typescript')}`,
  ], { cwd: checkout, encoding: 'utf8', timeout: 60_000 });
  assert.ifError(run.error);
  assert.equal(run.status, expectedExit, `${name}: ${run.stderr}\n${run.stdout}`);
  const report = JSON.parse(run.stdout);
  const saved = path.join(temporary, `${name}.json`);
  fs.writeFileSync(saved, run.stdout);
  execFileSync(process.execPath, [path.join(repository, 'scripts/validate-report.mjs'),
    path.join(repository, 'schemas/vibedoc-report.schema.json'), saved]);
  // Temporary document names are omitted from the persistent, reproducible summary.
  results.runs[name] = {
    exitCode: run.status, summary: report.summary, verification: report.verification,
    diagnostics: report.diagnostics.map(({ ruleId, message }) => ({ ruleId, message })),
  };
  return report;
}

try {
  // Keep upstream strictness; modernize module resolution for our pinned compiler,
  // and exclude upstream test files that require unrelated development packages.
  fs.writeFileSync(project, JSON.stringify({
    extends: '../tsconfig.json',
    compilerOptions: { target: 'ES2022', module: 'ESNext', moduleResolution: 'Bundler', types: [] },
    include: ['../src/index.ts', 'broken.ts'],
  }));
  fs.writeFileSync(config, `version = 1\n[[documents]]\ninclude = ["README.md", "${relative}/*.md"]\nprofile = "reference"\nadapters = ["typescript"]\n[adapters.typescript]\nprojects = ["${relative}/tsconfig.json"]\n`, { flag: 'wx' });
  configCreated = true;
  const params = { workspaceRoot: checkout, projects: [project], sourceGlobs: [] };
  const analysis = analyzeWorkspace(params);
  assert.deepEqual(analysis.diagnostics, []);
  assert.deepEqual(analysis.graph.files.map((file) => file.path), ['src/index.ts']);
  const symbol = (name) => {
    const matches = analysis.graph.symbols.filter((fact) => fact.qualifiedName === name);
    assert.equal(matches.length, 1, name);
    return matches[0];
  };
  const factory = symbol('mitt');
  assert.equal(factory.exported, true);
  assert.equal(factory.signatures.length, 1);
  assert.equal(factory.signatures[0].parameters[0].name, 'all');
  assert.equal(factory.signatures[0].parameters[0].optional, true);
  assert.equal(factory.signatures[0].returnType.display, 'Emitter<Events>');
  assert.equal(factory.signatures[0].returnType.confidence, 'exact');
  assert.equal(factory.declaration.range.start.line, 46);
  for (const method of ['Emitter.on', 'Emitter.off', 'Emitter.emit']) {
    assert.equal(symbol(method).signatures.length, 2, method);
  }
  assert.equal(symbol('Emitter.off').signatures[0].parameters[1].optional, true);
  assert.equal(new Set(analysis.graph.symbols.map((fact) => fact.id)).size, analysis.graph.symbols.length);
  results.facts = { files: analysis.graph.files.length, symbols: analysis.graph.symbols.length,
    relationships: analysis.graph.relationships.length, factoryReturn: factory.signatures[0].returnType.display,
    overloadsPerMethod: 2 };

  check('upstreamReadme', 'README.md', 'guide', 0);
  const valid = write('valid.md', reference('mitt', '## Parameters\n\n- `all`: The handler map.\n\n## Returns\n\n`Emitter<Events>`'));
  const clean = check('validReference', valid, 'reference', 0);
  assert.equal(clean.summary.warnings, 0);
  assert.equal(clean.verification.verifiedStructuralClaims, 2);
  assert.equal(clean.verification.unverifiedStructuralClaims, 0);
  const invalid = write('invalid.md', reference('mitt', '## Parameters\n\n- `ghost`: The handler map.\n\n## Returns\n\n`number`'));
  const wrong = check('invalidReference', invalid, 'reference', 1);
  assert.equal(wrong.verification.contradictedStructuralClaims, 2);
  assert.deepEqual(wrong.diagnostics.filter((d) => d.severity === 'error').map((d) => d.ruleId).sort(), ['VDOC-G003', 'VDOC-G006']);
  const overloaded = write('overloaded.md', reference('Emitter.on', '## Returns\n\n`void`'));
  const overload = check('overloadedReference', overloaded, 'reference', 0);
  assert.equal(overload.verification.unverifiedStructuralClaims, 1);
  assert.equal(overload.verification.verifiedStructuralClaims, 0);

  // Mutate a temporary copy, never upstream's tracked source.
  const source = fs.readFileSync(path.join(checkout, 'src/index.ts'), 'utf8');
  const brokenSource = source.replace('): Emitter<Events> {', '): MissingEmitter<Events> {');
  assert.notEqual(brokenSource, source);
  write('broken.ts', brokenSource);
  const broken = analyzeWorkspace(params).graph.symbols.find((fact) =>
    fact.name === 'mitt' && fact.declaration.path === `${relative}/broken.ts`);
  assert.equal(broken?.signatures[0].returnType.confidence, 'incomplete');
  const brokenDoc = write('incomplete.md', reference('mitt', '## Parameters\n\n- `all`: The handler map.\n\n## Returns\n\n`Emitter<Events>`', `${relative}/broken.ts`));
  const incomplete = check('unresolvedReturn', brokenDoc, 'reference', 0);
  assert.equal(incomplete.verification.unverifiedStructuralClaims, 1);
  assert.equal(incomplete.verification.contradictedStructuralClaims, 0);
  assert.ok(incomplete.diagnostics.some((d) => d.ruleId === 'VDOC-G008'));
  assert.equal(fs.readFileSync(path.join(checkout, 'src/index.ts'), 'utf8'), source);
} finally {
  if (configCreated) fs.unlinkSync(config);
  fs.rmSync(temporary, { recursive: true, force: true });
}
assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
const serialized = `${JSON.stringify(results, null, 2)}\n`;
if (process.argv[3]) fs.writeFileSync(path.resolve(process.argv[3]), serialized);
console.log(serialized);
