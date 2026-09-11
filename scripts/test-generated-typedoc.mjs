import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { createHash } from 'node:crypto';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { analyzeWorkspace } from '../adapters/typescript/dist/analyzer.js';

assert.ok(process.argv[2] && process.argv[3], 'Usage: node scripts/test-generated-typedoc.mjs IDB_KEYVAL_CHECKOUT GENERATOR_PREFIX [RESULT_JSON]');
const root = fileURLToPath(new URL('..', import.meta.url));
const checkout = path.resolve(process.argv[2]);
const generator = path.resolve(process.argv[3]);
const revision = '17a69a1165bef486d88950cb47d3913744f038ac';
const versions = { typedoc: '0.28.20', 'typedoc-plugin-markdown': '4.13.0', typescript: '5.8.3' };
const git = (...args) => execFileSync('git', ['-C', checkout, ...args], { encoding: 'utf8' }).trim();
assert.equal(git('rev-parse', 'HEAD'), revision);
assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
for (const [name, version] of Object.entries(versions)) {
  assert.equal(JSON.parse(fs.readFileSync(path.join(generator, 'node_modules', name, 'package.json'), 'utf8')).version, version);
}
const temporary = fs.mkdtempSync(path.join(checkout, '.vibedoc-generated-'));
const config = `${temporary}.toml`;
let configCreated = false;
const results = { repository: 'https://github.com/jakearchibald/idb-keyval', revision, generator: versions,
  layout: 'typedoc-plugin-markdown defaults; no signature, source-path, or parameter-format overrides' };
try {
  const output = path.join(temporary, 'docs');
  execFileSync(process.execPath, [path.join(generator, 'node_modules/typedoc/bin/typedoc'),
    '--plugin', path.join(generator, 'node_modules/typedoc-plugin-markdown/dist/index.js'),
    '--tsconfig', 'src/tsconfig.json', '--out', output, 'src/index.ts'], { cwd: checkout, encoding: 'utf8', timeout: 60_000 });
  const functions = fs.readdirSync(path.join(output, 'functions')).filter((file) => file.endsWith('.md')).sort();
  assert.equal(functions.length, 13);
  results.generatedFunctions = functions;
  results.generatedFileSha256 = Object.fromEntries(functions.map((file) => [file,
    createHash('sha256').update(fs.readFileSync(path.join(output, 'functions', file))).digest('hex')]));
  fs.writeFileSync(config, `version = 1\n[[documents]]\ninclude = ["${path.basename(temporary)}/docs/functions/*.md"]\nprofile = "reference"\nadapters = ["typescript"]\n[adapters.typescript]\nprojects = ["src/tsconfig.json"]\n`, { flag: 'wx' });
  configCreated = true;
  const analysis = analyzeWorkspace({ workspaceRoot: checkout, projects: ['src/tsconfig.json'], sourceGlobs: [] });
  assert.deepEqual(analysis.diagnostics, []);
  const clear = analysis.graph.symbols.find((symbol) => symbol.name === 'clear');
  assert.equal(clear?.signatures[0]?.returnType.display, 'Promise<void>');
  assert.equal(clear?.signatures[0]?.returnType.confidence, 'exact');
  for (const file of functions) assert.ok(analysis.graph.symbols.some((s) => s.name === file.slice(0, -3)));
  results.facts = { files: analysis.graph.files.length, symbols: analysis.graph.symbols.length,
    generatedFunctionsFoundInSource: functions.length, clearReturn: clear.signatures[0].returnType };
  const args = ['check', path.join(output, 'functions'), '--config', config, '--profile', 'reference', '--format', 'json',
    '--adapter-command', `typescript=${path.join(root, 'adapters/typescript/bin/vibedoc-adapter-typescript')}`];
  const run = (extra = []) => spawnSync(path.join(root, 'target/debug/vibedoc'), [...args, ...extra],
    { cwd: checkout, encoding: 'utf8', timeout: 60_000 });
  const checked = run();
  assert.ifError(checked.error);
  assert.equal(checked.status, 0, checked.stderr + checked.stdout);
  const report = JSON.parse(checked.stdout);
  const reportFile = path.join(temporary, 'report.json');
  fs.writeFileSync(reportFile, checked.stdout);
  execFileSync(process.execPath, [path.join(root, 'scripts/validate-report.mjs'), path.join(root, 'schemas/vibedoc-report.schema.json'), reportFile]);
  assert.equal(report.verification.verifiedStructuralClaims, 56);
  assert.equal(report.verification.contradictedStructuralClaims, 0);
  assert.equal(report.verification.unverifiedStructuralClaims, 3);
  assert.equal(report.summary.documents, 13);
  assert.deepEqual(report.diagnostics.map((d) => d.ruleId), ['VDOC-G008', 'VDOC-G008', 'VDOC-G008']);
  assert.equal(report.diagnostics.filter((d) => d.ruleId === 'VDOC-G010').length, 0);
  const denied = run(['--deny-warnings']);
  assert.ifError(denied.error);
  assert.equal(denied.status, 1);
  results.check = { exitCode: checked.status, denyWarningsExitCode: denied.status,
    summary: report.summary, verification: report.verification,
    diagnostics: report.diagnostics.map(({ ruleId, message, document }) => ({ file: path.basename(document.path), ruleId, message })) };
  const clearFile = path.join(output, 'functions', 'clear.md');
  const original = fs.readFileSync(clearFile, 'utf8');
  const changed = original.replace('## Returns\n\n`Promise`\\<`void`\\>', '## Returns\n\n`Promise`\\<`string`\\>');
  assert.notEqual(changed, original, 'Mutation must change the generated return claim');
  fs.writeFileSync(clearFile, changed);
  const mutated = run();
  assert.ifError(mutated.error);
  assert.equal(mutated.status, 1, mutated.stderr);
  const mutationReport = JSON.parse(mutated.stdout);
  assert.equal(mutationReport.verification.contradictedStructuralClaims, 1);
  assert.equal(mutationReport.verification.verifiedStructuralClaims, report.verification.verifiedStructuralClaims - 1);
  const contradiction = mutationReport.diagnostics.find((d) => d.ruleId === 'VDOC-G006');
  assert.ok(contradiction);
  assert.equal(path.basename(contradiction.document.path), 'clear.md');
  assert.equal(contradiction.evidence[0].path, 'src/index.ts');
  assert.equal(contradiction.evidence[0].range.start.line, 187);
  contradiction.document.path = 'functions/clear.md';
  results.mutation = { change: 'clear Returns: Promise<void> -> Promise<string>',
    exitCode: mutated.status, verification: mutationReport.verification, diagnostic: contradiction };

} finally {
  if (configCreated) fs.unlinkSync(config);
  fs.rmSync(temporary, { recursive: true, force: true });
}
assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
const serialized = `${JSON.stringify(results, null, 2)}\n`;
if (process.argv[4]) fs.writeFileSync(path.resolve(process.argv[4]), serialized);
console.log(serialized);
