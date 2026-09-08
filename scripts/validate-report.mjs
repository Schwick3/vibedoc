import fs from "node:fs";
import process from "node:process";
import Ajv2020 from "ajv/dist/2020.js";

const [schemaPath, reportPath] = process.argv.slice(2);
if (!schemaPath || !reportPath) {
  process.stderr.write("Usage: node validate-report.mjs SCHEMA REPORT\n");
  process.exit(2);
}

const schema = JSON.parse(fs.readFileSync(schemaPath, "utf8"));
const report = JSON.parse(fs.readFileSync(reportPath, "utf8"));
const ajv = new Ajv2020({ allErrors: true, strict: true });
const validate = ajv.compile(schema);

if (!validate(report)) {
  process.stderr.write(`${ajv.errorsText(validate.errors, { separator: "\n" })}\n`);
  process.exit(1);
}
