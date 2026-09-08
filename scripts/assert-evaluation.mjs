import assert from "node:assert/strict";
import fs from "node:fs";
import process from "node:process";

const [kind, reportPath] = process.argv.slice(2);
if (!kind || !reportPath) {
  process.stderr.write("Usage: node assert-evaluation.mjs KIND REPORT\n");
  process.exit(2);
}

const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
assert.equal(report.schemaVersion, 1);
assert.equal(report.tool.name, "vibedoc");

switch (kind) {
  case "doctor":
    assert.equal(report.status, "pass");
    assert.ok(report.checks.every((check) => check.ok));
    assert.match(
      report.checks.find((check) => check.name === "adapter:typescript").message,
      /Loaded 4 source file\(s\) and 14 symbol\(s\)/,
    );
    break;
  case "inspect": {
    assert.equal(report.status, "pass");
    const symbols = report.analysis.graph.symbols;
    assert.equal(symbols.length, 1);
    const [symbol] = symbols;
    assert.equal(
      symbol.id,
      "typescript:src/task-service.ts#TaskService.complete",
    );
    assert.deepEqual(
      symbol.signatures[0].parameters.map((parameter) => [
        parameter.name,
        parameter.typeFact.display,
      ]),
      [
        ["taskId", "string"],
        ["completedAt", "Date"],
      ],
    );
    assert.equal(symbol.signatures[0].returnType.display, "Promise<Task>");
    assert.equal(symbol.throws[0].typeName, "TaskNotFoundError");
    assert.deepEqual(
      report.analysis.graph.relationships.map((relationship) => relationship.display),
      ["this.repository.findById", "this.repository.save"],
    );
    break;
  }
  case "valid":
    assert.equal(report.status, "pass");
    assert.deepEqual(report.summary, {
      errors: 0,
      warnings: 0,
      info: 0,
      documents: 2,
    });
    assert.deepEqual(report.verification, {
      verifiedStructuralClaims: 14,
      contradictedStructuralClaims: 0,
      unverifiedStructuralClaims: 0,
      freeFormProseEvaluated: false,
    });
    break;
  case "invalid": {
    assert.equal(report.status, "fail");
    assert.deepEqual(report.summary, {
      errors: 3,
      warnings: 13,
      info: 0,
      documents: 1,
    });
    assert.deepEqual(report.verification, {
      verifiedStructuralClaims: 1,
      contradictedStructuralClaims: 3,
      unverifiedStructuralClaims: 1,
      freeFormProseEvaluated: true,
    });
    const ruleIds = new Set(report.diagnostics.map((diagnostic) => diagnostic.ruleId));
    for (const ruleId of [
      "VDOC-L001",
      "VDOC-L002",
      "VDOC-L003",
      "VDOC-T001",
      "VDOC-D001",
      "VDOC-D002",
      "VDOC-G003",
      "VDOC-G004",
      "VDOC-G005",
      "VDOC-G006",
      "VDOC-G007",
      "VDOC-X001",
      "VDOC-X002",
    ]) {
      assert.ok(ruleIds.has(ruleId), `missing ${ruleId}`);
    }
    break;
  }
  default:
    throw new Error(`Unknown evaluation assertion kind: ${kind}`);
}
