#!/usr/bin/env node

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";
import { pathToFileURL } from "node:url";

const TARGETS = [
  ["macos", "arm", "aarch64-apple-darwin"],
  ["linux", "arm", "aarch64-unknown-linux-musl"],
  ["linux", "intel", "x86_64-unknown-linux-musl"],
];
const CODE = String.fromCharCode(96);

export function parseChecksums(contents) {
  const checksums = new Map();
  for (const line of contents.split(/\r?\n/)) {
    if (line.length === 0) continue;
    const match = /^([0-9a-f]{64})  ([^/]+)$/.exec(line);
    assert(match, "Invalid checksum line: " + line);
    assert(!checksums.has(match[2]), "Duplicate checksum for " + match[2]);
    checksums.set(match[2], match[1]);
  }
  return checksums;
}

function requireChecksum(checksums, filename) {
  const checksum = checksums.get(filename);
  assert(checksum, "Missing checksum for " + filename);
  return checksum;
}

function sourceBlock(cpu, filename, releaseBaseUrl, checksum) {
  return [
    "    on_" + cpu + " do",
    "      url \"" + releaseBaseUrl + "/" + filename + "\"",
    "      sha256 \"" + checksum + "\"",
    "    end",
  ];
}

export function renderFormulae(version, releaseBaseUrl, checksums) {
  assert(/^\d+\.\d+\.\d+$/.test(version), "Invalid release version: " + version);
  const adapterFilename =
    "vibedoc-adapter-typescript-v" + version + ".tar.gz";
  const sourcesByOs = new Map([
    ["macos", []],
    ["linux", []],
  ]);
  for (const [os, cpu, target] of TARGETS) {
    const filename = "vibedoc-v" + version + "-" + target + ".tar.gz";
    sourcesByOs.get(os).push(
      ...sourceBlock(
        cpu,
        filename,
        releaseBaseUrl,
        requireChecksum(checksums, filename),
      ),
    );
  }

  const cli = [
    "class Vibedoc < Formula",
    "  desc \"Evidence-aware software documentation checker\"",
    "  homepage \"https://github.com/Schwick3/vibedoc\"",
    "  license \"MIT\"",
    "",
    "  depends_on \"node\"",
    "",
    "  on_macos do",
    "    depends_on arch: :arm64",
    ...sourcesByOs.get("macos"),
    "  end",
    "  on_linux do",
    ...sourcesByOs.get("linux"),
    "  end",
    "",
    "  resource \"typescript-adapter\" do",
    "    url \"" + releaseBaseUrl + "/" + adapterFilename + "\"",
    "    sha256 \"" + requireChecksum(checksums, adapterFilename) + "\"",
    "  end",
    "",
    "  def install",
    "    bin.install \"vibedoc\"",
    "    pkgshare.install \"README.md\"",
    "    prefix.install \"LICENSE\"",
    "",
    "    resource(\"typescript-adapter\").stage do",
    "      (libexec/\"typescript-adapter\").install Dir[\"*\"]",
    "    end",
    "    bin.install_symlink libexec/\"typescript-adapter/dist/index.js\" => \"vibedoc-adapter-typescript\"",
    "  end",
    "",
    "  test do",
    "    assert_match version.to_s, shell_output(\"#{bin}/vibedoc --version\")",
    "",
    "    (testpath/\"src\").mkpath",
    "    (testpath/\"docs/reference\").mkpath",
    "    (testpath/\"src/greet.ts\").write <<~TYPESCRIPT",
    "      export function greet(name: string): string {",
    "        return \"Hello \" + name;",
    "      }",
    "    TYPESCRIPT",
    "    (testpath/\"tsconfig.json\").write <<~JSON",
    "      {",
    "        \"compilerOptions\": { \"strict\": true, \"noEmit\": true },",
    "        \"include\": [\"src/**/*.ts\"]",
    "      }",
    "    JSON",
    "    (testpath/\"vibedoc.toml\").write <<~TOML",
    "      version = 1",
    "",
    "      [[documents]]",
    "      include = [\"docs/**/*.md\"]",
    "      profile = \"reference\"",
    "      adapters = [\"typescript\"]",
    "",
    "      [adapters.typescript]",
    "      projects = [\"tsconfig.json\"]",
    "    TOML",
    "    (testpath/\"docs/reference/greet.md\").write <<~MARKDOWN",
    "      <!-- vibedoc:source adapter=\"typescript\" path=\"src/greet.ts\" symbol=\"greet\" -->",
    "      # " + CODE + "greet" + CODE,
    "",
    "      " + CODE + "greet" + CODE + " returns a greeting.",
    "",
    "      ## Parameters",
    "",
    "      - " + CODE + "name" + CODE + " (" + CODE + "string" + CODE + "): The name to include in the greeting.",
    "",
    "      ## Returns",
    "",
    "      " + CODE + "string" + CODE,
    "    MARKDOWN",
    "",
    "    system bin/\"vibedoc\", \"doctor\", \"--format\", \"json\"",
    "    system bin/\"vibedoc\", \"check\"",
    "  end",
    "end",
    "",
  ].join("\n");

  return {
    "vibedoc.rb": cli,
  };
}

function option(name) {
  const index = process.argv.indexOf(name);
  if (index === -1 || !process.argv[index + 1]) {
    throw new Error("Missing required option " + name);
  }
  return process.argv[index + 1];
}

function main() {
  const version = option("--version");
  const checksumsPath = option("--checksums");
  const outputDirectory = option("--output");
  const releaseBaseUrl =
    process.argv.includes("--release-base-url")
      ? option("--release-base-url")
      : "https://github.com/Schwick3/vibedoc/releases/download/v" + version;
  const checksums = parseChecksums(fs.readFileSync(checksumsPath, "utf8"));
  const formulae = renderFormulae(version, releaseBaseUrl, checksums);
  fs.mkdirSync(outputDirectory, { recursive: true });
  for (const [filename, contents] of Object.entries(formulae)) {
    fs.writeFileSync(path.join(outputDirectory, filename), contents);
  }
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href
) {
  try {
    main();
  } catch (error) {
    process.stderr.write((error instanceof Error ? error.message : String(error)) + "\n");
    process.exitCode = 2;
  }
}
