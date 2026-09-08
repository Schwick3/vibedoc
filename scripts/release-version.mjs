#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

const STABLE_TAG = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function parseWorkspaceVersion(cargoToml) {
  const section = cargoToml.match(
    /\[workspace\.package\]([\s\S]*?)(?=\n\[[^\]]+\]|$)/,
  );
  assert(section, "Cargo.toml does not contain [workspace.package]");
  const version = section[1].match(/^version\s*=\s*"([^"]+)"\s*$/m);
  assert(version, "[workspace.package] does not contain a version");
  return version[1];
}

export function validateReleaseVersions(tag, cargoVersion, adapterVersion) {
  const match = STABLE_TAG.exec(tag);
  assert(match, "release tag must match vMAJOR.MINOR.PATCH; received " + tag);
  const releaseVersion = tag.slice(1);
  assert.equal(
    cargoVersion,
    releaseVersion,
    "Cargo workspace version " + cargoVersion + " does not match tag " + tag,
  );
  assert.equal(
    adapterVersion,
    releaseVersion,
    "TypeScript adapter version " + adapterVersion + " does not match tag " + tag,
  );
  return releaseVersion;
}

export function readReleaseVersions(repositoryRoot) {
  const cargoToml = fs.readFileSync(
    path.join(repositoryRoot, "Cargo.toml"),
    "utf8",
  );
  const adapterPackage = JSON.parse(
    fs.readFileSync(
      path.join(repositoryRoot, "adapters/typescript/package.json"),
      "utf8",
    ),
  );
  assert.equal(
    typeof adapterPackage.version,
    "string",
    "TypeScript adapter package.json does not contain a version",
  );
  return {
    cargoVersion: parseWorkspaceVersion(cargoToml),
    adapterVersion: adapterPackage.version,
  };
}

function main() {
  const tag = process.argv[2];
  if (!tag) {
    throw new Error("usage: node scripts/release-version.mjs vMAJOR.MINOR.PATCH");
  }
  const repositoryRoot = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    "..",
  );
  const versions = readReleaseVersions(repositoryRoot);
  process.stdout.write(
    validateReleaseVersions(
      tag,
      versions.cargoVersion,
      versions.adapterVersion,
    ) + "\n",
  );
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
