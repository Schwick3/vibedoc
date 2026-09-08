#!/usr/bin/env node
import { createInterface } from "node:readline";
import { readFileSync } from "node:fs";
import process from "node:process";
import ts from "typescript";
import { analyzeWorkspace } from "./analyzer.js";
import { PROTOCOL_VERSION, type AnalyzeParams } from "./protocol.js";

interface PackageMetadata {
  version?: unknown;
}

function readAdapterVersion(): string {
  const metadata = JSON.parse(
    readFileSync(new URL("../package.json", import.meta.url), "utf8"),
  ) as PackageMetadata;
  if (typeof metadata.version !== "string" || metadata.version.length === 0) {
    throw new Error("The adapter package.json does not contain a version");
  }
  return metadata.version;
}

const ADAPTER_VERSION = readAdapterVersion();

interface Request {
  jsonrpc: string;
  id: number;
  method: string;
  params?: unknown;
}

function respond(id: number, result: unknown): void {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id, result })}\n`);
}

function fail(id: number, code: number, message: string): void {
  process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", id, error: { code, message } })}\n`);
}

if (!process.argv.includes("--stdio")) {
  process.stderr.write("vibedoc-adapter-typescript must be run with --stdio\n");
  process.exitCode = 2;
} else {
  const input = createInterface({ input: process.stdin, crlfDelay: Infinity });
  input.on("line", (line) => {
    let request: Request;
    try {
      request = JSON.parse(line) as Request;
    } catch (error) {
      fail(0, -32700, error instanceof Error ? error.message : "Invalid JSON");
      return;
    }
    if (request.jsonrpc !== "2.0" || typeof request.id !== "number" || typeof request.method !== "string") {
      fail(request.id ?? 0, -32600, "Invalid JSON-RPC request");
      return;
    }
    try {
      switch (request.method) {
        case "initialize": {
          const params = request.params as { protocolVersion?: number };
          if (params?.protocolVersion !== PROTOCOL_VERSION) {
            fail(request.id, -32001, `Unsupported protocol ${String(params?.protocolVersion)}`);
            break;
          }
          respond(request.id, {
            protocolVersion: PROTOCOL_VERSION,
            adapter: {
              name: "typescript",
              version: ADAPTER_VERSION,
              runtime: `node ${process.versions.node}; typescript ${ts.version}`,
            },
            capabilities: {
              languages: ["typescript", "tsx", "javascript", "jsx"],
              extensions: [".ts", ".tsx", ".js", ".jsx"],
              relationships: ["calls", "writes", "extends", "implements"],
            },
          });
          break;
        }
        case "analyze":
          respond(request.id, analyzeWorkspace(request.params as AnalyzeParams));
          break;
        case "shutdown":
          process.stdout.write(
            `${JSON.stringify({ jsonrpc: "2.0", id: request.id, result: {} })}\n`,
            () => process.exit(0),
          );
          break;
        default:
          fail(request.id, -32601, `Unknown method: ${request.method}`);
      }
    } catch (error) {
      const message = error instanceof Error ? error.stack ?? error.message : String(error);
      process.stderr.write(`${message}\n`);
      fail(request.id, -32000, error instanceof Error ? error.message : String(error));
    }
  });
}
