#!/usr/bin/env node

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

export function checksumFiles(files) {
  return [...files]
    .sort((left, right) =>
      path.basename(left).localeCompare(path.basename(right), "en"),
    )
    .map((file) => {
      const checksum = createHash("sha256")
        .update(fs.readFileSync(file))
        .digest("hex");
      return checksum + "  " + path.basename(file);
    });
}

function main() {
  const [output, ...files] = process.argv.slice(2);
  if (!output || files.length === 0) {
    throw new Error(
      "usage: node scripts/write-release-checksums.mjs OUTPUT FILE...",
    );
  }
  const lines = checksumFiles(files);
  fs.writeFileSync(output, lines.join("\n") + "\n");
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  try {
    main();
  } catch (error) {
    process.stderr.write(
      (error instanceof Error ? error.message : String(error)) + "\n",
    );
    process.exitCode = 2;
  }
}
