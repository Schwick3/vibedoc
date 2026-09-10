import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { analyzeWorkspace } from '../adapters/typescript/dist/analyzer.js';

const revision = '7452ef68a2901fc9a5673a8e5f53185a506b182f';
const repository = fileURLToPath(new URL('..', import.meta.url));
assert.ok(process.argv[2], 'Usage: node scripts/test-monorepo.mjs QUERY_CHECKOUT [RESULT_JSON]');
const checkout = path.resolve(process.argv[2]);
const git = (...args) => execFileSync('git', ['-C', checkout, ...args], { encoding: 'utf8' }).trim();
assert.equal(git('rev-parse', 'HEAD'), revision);
assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
const temporary = fs.mkdtempSync(path.join(checkout, '.vibedoc-evaluation-'));
const directory = path.basename(temporary);
const config = `${temporary}.toml`;
let configCreated = false;
const results = { repository: 'https://github.com/TanStack/query', revision, runs: {} };
const source = (name) => `packages/query-core/src/${name}.ts`;
function reference(symbol, file, body) {
  return `<!-- vibedoc:source adapter="typescript" path="${file}" symbol="${symbol}" -->\n# \`${symbol}\`\n\n${body}\n`;
}
function write(name, content) {
  fs.writeFileSync(path.join(temporary, name), content);
  return `${directory}/${name}`;
}
function check(name, document, profile, expectedExit) {
  const run = spawnSync(path.join(repository, 'target/debug/vibedoc'), [
    'check', document, '--config', config, '--profile', profile, '--format', 'json',
    '--adapter-command', `typescript=${path.join(repository, 'adapters/typescript/bin/vibedoc-adapter-typescript')}`,
  ], { cwd: checkout, encoding: 'utf8', timeout: 120_000 });
  assert.ifError(run.error);
  assert.equal(run.status, expectedExit, `${name}: ${run.stderr}\n${run.stdout}`);
  const report = JSON.parse(run.stdout);
  const saved = path.join(temporary, `${name}.json`);
  fs.writeFileSync(saved, run.stdout);
  execFileSync(process.execPath, [path.join(repository, 'scripts/validate-report.mjs'), path.join(repository, 'schemas/vibedoc-report.schema.json'), saved]);
  results.runs[name] = { exitCode: run.status, summary: report.summary, verification: report.verification,
    diagnostics: report.diagnostics.map(({ ruleId, message, document }) => ({ ruleId, message, line: document.range.start.line })) };
  return report;
}
try {
  const projects = ['query-core', 'query-persist-client-core'].map((name) => {
    const file = path.join(temporary, `${name}.json`);
    fs.writeFileSync(file, JSON.stringify({ extends: `../packages/${name}/tsconfig.json`,
      compilerOptions: { composite: false, incremental: false, rootDir: '..', types: [],
        paths: { '@tanstack/query-core': ['../packages/query-core/src/index.ts'] } },
      include: [`../packages/${name}/src/*.ts`], exclude: [] }));
    return file;
  });
  fs.writeFileSync(config, `version = 1\n[[documents]]\ninclude = ["${directory}/*.md", "docs/**/*.md"]\nprofile = "reference"\nadapters = ["typescript"]\n[adapters.typescript]\nprojects = ${JSON.stringify(projects)}\n`, { flag: 'wx' });
  configCreated = true;
  const params = { workspaceRoot: checkout, projects, sourceGlobs: [] };
  const analysis = analyzeWorkspace(params);
  assert.deepEqual(analysis.diagnostics, []);
  assert.equal(analysis.graph.files.length, 27);
  assert.equal(analysis.graph.symbols.length, 1049);
  assert.equal(new Set(analysis.graph.symbols.map((s) => s.id)).size, analysis.graph.symbols.length);
  for (const [caller, callee] of [['persistQueryClientRestore', 'hydrate'], ['persistQueryClientSave', 'dehydrate']]) {
    assert.ok(analysis.graph.relationships.some((r) => r.from.endsWith(`#${caller}`) &&
      r.to === `typescript:${source('hydration')}#${callee}` && r.confidence === 'exact'), `${caller} -> ${callee}`);
  }
  assert.deepEqual(analyzeWorkspace({ ...params, projects: [...projects].reverse() }), analysis);
  results.facts = { files: analysis.graph.files.length, symbols: analysis.graph.symbols.length,
    relationships: analysis.graph.relationships.length, crossPackageTargetsResolved: 2, projectOrderIndependent: true };
  check('upstreamQueryClientGuide', 'docs/framework/react/reference/classes/QueryClient.md', 'guide', 0);
  check('upstreamPersistenceGuide', 'docs/framework/react/plugins/persistQueryClient.md', 'guide', 0);
  // Also measure native reference coverage without adding bindings to upstream docs.
  check('upstreamQueryClientReference', 'docs/framework/react/reference/classes/QueryClient.md', 'reference', 0);
  const valid = write('valid.md', '# Reference\n\n' + reference('QueryClient.clear', source('queryClient'), '## Returns\n\n`void`') + '\n' +
    reference('Subscribable.hasListeners', source('subscribable'), '## Returns\n\n`boolean`'));
  const clean = check('validReference', valid, 'reference', 0);
  assert.equal(clean.verification.verifiedStructuralClaims, 2);
  assert.equal(clean.summary.warnings, 0);
  const invalid = write('invalid.md', '# Reference\n\n' + reference('QueryClient.clear', source('queryClient'), '## Returns\n\n`string`') + '\n' +
    reference('Subscribable.hasListeners', source('subscribable'), '## Returns\n\n`number`'));
  const wrong = check('invalidReference', invalid, 'reference', 1);
  assert.equal(wrong.verification.contradictedStructuralClaims, 2);
  assert.equal(wrong.summary.errors, 2);
  const dynamic = write('dynamic.md', reference('dehydrate', source('hydration'), '## Parameters\n\n- `client`: The client.\n- `options`: The options.\n\n## Returns\n\n`DehydratedState`'));
  const uncertain = check('dynamicObjectReference', dynamic, 'reference', 0);
  assert.equal(uncertain.verification.unverifiedStructuralClaims, 1);
  assert.equal(uncertain.verification.contradictedStructuralClaims, 0);
} finally {
  if (configCreated) fs.unlinkSync(config);
  fs.rmSync(temporary, { recursive: true, force: true });
}
assert.equal(git('status', '--porcelain', '--untracked-files=no'), '');
const serialized = `${JSON.stringify(results, null, 2)}\n`;
if (process.argv[3]) fs.writeFileSync(path.resolve(process.argv[3]), serialized);
console.log(serialized);
