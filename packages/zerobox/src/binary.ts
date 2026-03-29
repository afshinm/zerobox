import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { platformPackage } from "./platforms.js";

/**
 * Resolve the path to the zerobox binary.
 *
 * Resolution order:
 *   1. ZEROBOX_BIN environment variable
 *   2. Platform-specific optional dependency package
 *   3. "zerobox" on PATH
 */
export function resolveBinary(): string {
  if (process.env.ZEROBOX_BIN) {
    return process.env.ZEROBOX_BIN;
  }

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

  return "zerobox";
}

/** Verify the binary is reachable and return its path. */
export function verifyBinary(): string {
  const bin = resolveBinary();
  try {
    execFileSync(bin, ["--help"], { stdio: "pipe" });
  } catch (e: unknown) {
    if ((e as NodeJS.ErrnoException).code === "ENOENT") {
      throw new Error(
        `zerobox binary not found at "${bin}". Install the package (npm install zerobox) or set ZEROBOX_BIN.`,
      );
    }
    // Binary exists but --help failed -- still usable.
  }
  return bin;
}
