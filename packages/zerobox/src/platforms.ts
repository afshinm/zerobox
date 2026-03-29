import { execSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";

const GLIBC_PLATFORMS: Record<string, string> = {
  "darwin-arm64": "@zerobox/cli-darwin-arm64",
  "darwin-x64": "@zerobox/cli-darwin-x64",
  "linux-arm64": "@zerobox/cli-linux-arm64",
  "linux-x64": "@zerobox/cli-linux-x64",
};

const MUSL_PLATFORMS: Record<string, string> = {
  "linux-arm64": "@zerobox/cli-linux-arm64-musl",
  "linux-x64": "@zerobox/cli-linux-x64-musl",
};

// Map Node.js arch names to musl dynamic linker filenames.
const MUSL_LINKER: Record<string, string> = {
  arm64: "/lib/ld-musl-aarch64.so.1",
  x64: "/lib/ld-musl-x86_64.so.1",
};

/** Injected dependencies for testing. */
export interface PlatformEnv {
  platform: string;
  arch: string;
  linkerExists: (path: string) => boolean;
  glibcVersion: () => string | undefined;
  lddOutput: () => string | undefined;
  osRelease: () => string | undefined;
}

function realEnv(): PlatformEnv {
  return {
    platform: process.platform,
    arch: process.arch,
    linkerExists: (path: string) => existsSync(path),
    glibcVersion: () => {
      try {
        // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- process.report can be undefined in older Node
        const report = process.report?.getReport() as
          | { header?: { glibcVersionRuntime?: string } }
          | undefined;
        return report?.header?.glibcVersionRuntime;
      } catch {
        return undefined;
      }
    },
    lddOutput: () => {
      try {
        return execSync("ldd --version 2>&1 || true", {
          encoding: "utf8",
          timeout: 3000,
        });
      } catch {
        return undefined;
      }
    },
    osRelease: () => {
      try {
        return readFileSync("/etc/os-release", "utf8");
      } catch {
        return undefined;
      }
    },
  };
}

/**
 * Detect if the system uses musl libc.
 *
 * Detection order (fastest and most reliable first):
 *   1. Check for musl dynamic linker on disk (no spawning, works on Alpine)
 *   2. Check Node.js process.report for glibc version (no spawning)
 *   3. Fall back to `ldd --version` output (spawns a process)
 *   4. Check /etc/os-release for Alpine (last resort)
 */
export function detectMusl(env: PlatformEnv): boolean {
  if (env.platform !== "linux") return false;

  // 1. Check for musl dynamic linker file.
  const linker = MUSL_LINKER[env.arch] as string | undefined;
  if (linker && env.linkerExists(linker)) {
    return true;
  }

  // 2. If Node.js reports a glibc version, it's glibc.
  if (env.glibcVersion()) {
    return false;
  }

  // 3. Check ldd output.
  const ldd = env.lddOutput();
  if (ldd) {
    const lower = ldd.toLowerCase();
    if (lower.includes("musl")) return true;
    if (lower.includes("gnu")) return false;
  }

  // 4. Check /etc/os-release for Alpine.
  const osRelease = env.osRelease();
  if (osRelease?.toLowerCase().includes("alpine")) {
    return true;
  }

  return false;
}

/** Resolve the platform-specific npm package name for the current system. */
export function platformPackage(env?: PlatformEnv): string | undefined {
  const e = env ?? realEnv();
  const key = `${e.platform}-${e.arch}`;
  if (e.platform === "linux" && detectMusl(e)) {
    return MUSL_PLATFORMS[key] ?? GLIBC_PLATFORMS[key];
  }
  return GLIBC_PLATFORMS[key] as string | undefined;
}
