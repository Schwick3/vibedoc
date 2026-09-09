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
  validateReleaseVersions,
} from "./release-version.mjs";
import { checksumFiles } from "./write-release-checksums.mjs";

test("release versions require a stable matching tag", () => {
  assert.equal(
    parseWorkspaceVersion(
      "[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"1.2.3\"\n",
    ),
    "1.2.3",
  );
  assert.equal(validateReleaseVersions("v1.2.3", "1.2.3", "1.2.3"), "1.2.3");
  assert.throws(
    () => validateReleaseVersions("1.2.3", "1.2.3", "1.2.3"),
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
