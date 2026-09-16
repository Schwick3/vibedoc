#!/usr/bin/env node
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { readReleaseVersions, validateReleaseVersions } from "./release-version.mjs";

export function adapterManifest(version, directory) {
  const versions = readReleaseVersions(path.resolve(import.meta.dirname, ".."));
  validateReleaseVersions(`v${version}`, versions.cargoVersion, versions.adapterVersion, versions.pythonVersion);
  return {
    schemaVersion: 1,
    adapters: ["python", "typescript"].map((name) => {
      const archive = `vibedoc-adapter-${name}-v${version}.tar.gz`;
      return {
        name, version, protocolVersion: 1,
        runtime: name === "python" ? { name: "python3", minVersion: "3.10.0" } : { name: "node", minVersion: "22.0.0" },
        archive,
        sha256: createHash("sha256").update(fs.readFileSync(path.join(directory, archive))).digest("hex"),
        executable: name === "python" ? "bin/vibedoc-adapter-python" : "dist/index.js",
      };
    }),
  };
}
if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  try {
    const [, , version, directory] = process.argv;
    if (!version || !directory) throw new Error("usage: node scripts/write-adapter-manifest.mjs VERSION ARTIFACT_DIRECTORY");
    fs.writeFileSync(path.join(directory, "adapters.json"), JSON.stringify(adapterManifest(version, directory), null, 2) + "\n");
  } catch (error) { console.error(error.message); process.exitCode = 2; }
}
