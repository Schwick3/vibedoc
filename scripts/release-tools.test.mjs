import assert from "node:assert/strict";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";

import {
  parseChecksums,
  renderFormulae,
} from "./generate-homebrew-formulae.mjs";
import {
  parseWorkspaceVersion,
  readReleaseVersions,
  validateReleaseVersions,
} from "./release-version.mjs";
import { adapterManifest } from "./write-adapter-manifest.mjs";
import { createHash } from "node:crypto";
import { checksumFiles } from "./write-release-checksums.mjs";

test("release versions require a stable matching tag", () => {
  assert.equal(
    parseWorkspaceVersion(
      "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"1.2.3\"\n",
    ),
    "1.2.3",
  );
  assert.equal(validateReleaseVersions("v1.2.3", "1.2.3", "1.2.3", "1.2.3"), "1.2.3");
  assert.throws(
    () => validateReleaseVersions("1.2.3", "1.2.3", "1.2.3", "1.2.3"),
    /vMAJOR\.MINOR\.PATCH/,
  );
  assert.throws(
    () => validateReleaseVersions("v1.2.3", "1.2.4", "1.2.3"),
    /Cargo workspace version/,
  );
  assert.throws(
    () => validateReleaseVersions("v1.2.3", "1.2.3", "1.2.4"),
    /TypeScript adapter version/,
  );
});

test("Python version must match too", () => {
  assert.throws(() => validateReleaseVersions("v1.2.3", "1.2.3", "1.2.3", "1.2.4"), /Python adapter version/);
});

test("checksums are deterministic and formulae require every artifact", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "vibedoc-release-"));
  try {
    const alpha = path.join(directory, "zeta");
    const beta = path.join(directory, "alpha");
    fs.writeFileSync(alpha, "z");
    fs.writeFileSync(beta, "a");
    const lines = checksumFiles([alpha, beta]);
    assert.match(lines[0], /  alpha$/);
    assert.match(lines[1], /  zeta$/);

    const names = [
      "vibedoc-v0.1.0-aarch64-apple-darwin.tar.gz",
      "vibedoc-v0.1.0-aarch64-unknown-linux-musl.tar.gz",
      "vibedoc-v0.1.0-x86_64-unknown-linux-musl.tar.gz",
      "vibedoc-adapter-typescript-v0.1.0.tar.gz",
    ];
    const checksumText = names
      .map((name, index) => String(index + 1).padStart(64, "0") + "  " + name)
      .join("\n");
    const checksums = parseChecksums(checksumText);
    const formulae = renderFormulae(
      "0.1.0",
      "https://example.test/v0.1.0",
      checksums,
    );
    assert.match(formulae["vibedoc.rb"], /class Vibedoc < Formula/);
    assert.match(
      formulae["vibedoc.rb"],
      /resource "typescript-adapter"/,
    );
    assert.match(formulae["vibedoc.rb"], /depends_on "node"/);
    assert.match(formulae["vibedoc.rb"], /depends_on arch: :arm64/);
    assert.match(
      formulae["vibedoc.rb"],
      /url .*aarch64-apple-darwin.*\n  sha256 .*\n  license "MIT"/,
    );
    assert.match(
      formulae["vibedoc.rb"],
      /on_macos do\n    depends_on arch: :arm64\n  end/,
    );
    assert.doesNotMatch(formulae["vibedoc.rb"], /x86_64-apple-darwin/);
    assert.equal(Object.keys(formulae).length, 1);

    checksums.delete(names[0]);
    assert.throws(
      () => renderFormulae("0.1.0", "https://example.test", checksums),
      /Missing checksum/,
    );
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
});


test("adapter manifests pin both actual archives and executable layouts", () => {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "vibedoc-manifest-"));
  try {
    const { cargoVersion: version } = readReleaseVersions(path.resolve(import.meta.dirname, ".."));
    for (const name of ["python", "typescript"]) {
      fs.writeFileSync(path.join(directory, `vibedoc-adapter-${name}-v${version}.tar.gz`), name);
    }
    const manifest = adapterManifest(version, directory);
    assert.equal(manifest.schemaVersion, 1);
    assert.deepEqual(manifest.adapters.map((entry) => entry.executable), ["bin/vibedoc-adapter-python", "dist/index.js"]);
    for (const entry of manifest.adapters) {
      assert.equal(entry.sha256, createHash("sha256").update(entry.name).digest("hex"));
      assert.equal(entry.version, version);
      assert.equal(entry.protocolVersion, 1);
      assert.equal(path.basename(entry.archive), entry.archive);
    }
    assert.deepEqual(adapterManifest(version, directory), manifest);
    fs.unlinkSync(path.join(directory, manifest.adapters[0].archive));
    assert.throws(() => adapterManifest(version, directory), /ENOENT/);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
