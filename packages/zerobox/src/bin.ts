#!/usr/bin/env node

import { execFileSync } from "node:child_process";
import { createRequire } from "node:module";
import { delimiter, dirname, join } from "node:path";
import { existsSync, openSync, readSync, closeSync, realpathSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { platformPackage } from "./platforms.js";

const __filename = fileURLToPath(import.meta.url);

function resolveBinary(): string {
  // 1. Explicit override.
  if (process.env.ZEROBOX_BIN) {
    return process.env.ZEROBOX_BIN;
  }

  // 2. Platform-specific optional dependency.
  const pkg = platformPackage();
  if (pkg) {
    try {
      const nodeRequire = createRequire(import.meta.url);
      const dir = dirname(nodeRequire.resolve(`${pkg}/package.json`));
      const bin = join(dir, "zerobox");
      if (existsSync(bin)) {
        return bin;
      }
    } catch {
      // Package not installed.
    }
  }

  // 3. Fall back to PATH, skipping our own shim to avoid infinite recursion.
  const pathDirs = (process.env.PATH ?? "").split(delimiter);
  let selfPath: string | undefined;
  try {
    selfPath = realpathSync(__filename);
  } catch {
    // Ignore.
  }

  for (const dir of pathDirs) {
    const candidate = join(dir, "zerobox");
    try {
      if (!existsSync(candidate)) continue;
      const resolved = realpathSync(candidate);
      if (selfPath && resolved === selfPath) continue;
      // Read only the first 64 bytes to check for a Node.js shebang
      // without loading the entire (potentially multi-MB) binary.
      if (isNodeScript(candidate)) continue;
      return candidate;
    } catch {
      continue;
    }
  }

  console.error(
    "error: zerobox binary not found.\n\n" +
      "Install it with one of:\n" +
      "  cargo install zerobox\n" +
      "  Set ZEROBOX_BIN=/path/to/zerobox\n",
  );
  process.exit(1);
}

/** Check if a file starts with a Node.js shebang without reading the whole file. */
function isNodeScript(path: string): boolean {
  const buf = Buffer.alloc(64);
  let fd: number | undefined;
  try {
    fd = openSync(path, "r");
    readSync(fd, buf, 0, 64, 0);
    const head = buf.toString("utf8");
    return head.startsWith("#!") && head.includes("node");
  } catch {
    return false;
  } finally {
    if (fd !== undefined) closeSync(fd);
  }
}

try {
  execFileSync(resolveBinary(), process.argv.slice(2), {
    stdio: "inherit",
  });
} catch (e: unknown) {
  const status = (e as { status?: number }).status;
  process.exit(status ?? 1);
}
