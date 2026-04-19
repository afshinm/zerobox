import { describe, it, expect } from "vitest";
import { detectMusl, platformPackage, type PlatformEnv } from "./platforms.js";

function makeEnv(overrides: Partial<PlatformEnv> = {}): PlatformEnv {
  return {
    platform: "linux",
    arch: "x64",
    linkerExists: () => false,
    glibcVersion: () => undefined,
    lddOutput: () => undefined,
    osRelease: () => undefined,
    ...overrides,
  };
}

describe("detectMusl", () => {
  it("returns false on non-linux platforms", () => {
    expect(detectMusl(makeEnv({ platform: "darwin" }))).toBe(false);
    expect(detectMusl(makeEnv({ platform: "win32" }))).toBe(false);
  });

  // ── Tier 1: musl linker file ──

  it("detects musl via dynamic linker on x64", () => {
    const env = makeEnv({
      arch: "x64",
      linkerExists: (p) => p === "/lib/ld-musl-x86_64.so.1",
    });
    expect(detectMusl(env)).toBe(true);
  });

  it("detects musl via dynamic linker on arm64", () => {
    const env = makeEnv({
      arch: "arm64",
      linkerExists: (p) => p === "/lib/ld-musl-aarch64.so.1",
    });
    expect(detectMusl(env)).toBe(true);
  });

  it("skips linker check for unknown arch", () => {
    const env = makeEnv({
      arch: "s390x",
      linkerExists: () => true, // shouldn't be called with a valid path
      lddOutput: () => "musl libc",
    });
    expect(detectMusl(env)).toBe(true);
  });

  // ── Tier 2: glibc version from process.report ──

  it("returns false when glibc version is reported", () => {
    const env = makeEnv({ glibcVersion: () => "2.39" });
    expect(detectMusl(env)).toBe(false);
  });

  it("continues when glibc version is undefined", () => {
    const env = makeEnv({
      glibcVersion: () => undefined,
      lddOutput: () => "musl libc (x86_64)\nVersion 1.2.4",
    });
    expect(detectMusl(env)).toBe(true);
  });

  // ── Tier 3: ldd output ──

  it("detects musl from ldd output", () => {
    const env = makeEnv({
      lddOutput: () => "musl libc (x86_64)\nVersion 1.2.4\nDynamic Program Loader",
    });
    expect(detectMusl(env)).toBe(true);
  });

  it("detects glibc from ldd output", () => {
    const env = makeEnv({
      lddOutput: () => "ldd (GNU libc) 2.39\nCopyright (C) 2024 Free Software Foundation",
    });
    expect(detectMusl(env)).toBe(false);
  });

  it("continues when ldd returns unknown output", () => {
    const env = makeEnv({
      lddOutput: () => "some unknown output",
      osRelease: () => 'NAME="Alpine Linux"\nID=alpine',
    });
    expect(detectMusl(env)).toBe(true);
  });

  // ── Tier 4: /etc/os-release ──

  it("detects musl via Alpine os-release", () => {
    const env = makeEnv({
      osRelease: () => 'NAME="Alpine Linux"\nID=alpine\nVERSION_ID=3.19.0',
    });
    expect(detectMusl(env)).toBe(true);
  });

  it("returns false for non-Alpine os-release", () => {
    const env = makeEnv({
      osRelease: () => 'NAME="Ubuntu"\nID=ubuntu\nVERSION_ID="24.04"',
    });
    expect(detectMusl(env)).toBe(false);
  });

  it("returns false when nothing matches", () => {
    expect(detectMusl(makeEnv())).toBe(false);
  });

  // ── Priority order ──

  it("linker check takes priority over ldd", () => {
    const env = makeEnv({
      arch: "x64",
      linkerExists: (p) => p === "/lib/ld-musl-x86_64.so.1",
      lddOutput: () => "ldd (GNU libc) 2.39", // would say glibc
    });
    expect(detectMusl(env)).toBe(true);
  });

  it("glibc version takes priority over ldd musl", () => {
    const env = makeEnv({
      glibcVersion: () => "2.39",
      lddOutput: () => "musl libc", // contradicts
    });
    expect(detectMusl(env)).toBe(false);
  });
});

describe("platformPackage", () => {
  it("returns darwin-arm64 package on macOS ARM", () => {
    expect(platformPackage(makeEnv({ platform: "darwin", arch: "arm64" }))).toBe(
      "@zerobox/cli-darwin-arm64",
    );
  });

  it("returns darwin-x64 package on macOS Intel", () => {
    expect(platformPackage(makeEnv({ platform: "darwin", arch: "x64" }))).toBe(
      "@zerobox/cli-darwin-x64",
    );
  });

  it("returns glibc linux package on standard Linux", () => {
    expect(platformPackage(makeEnv({ lddOutput: () => "ldd (GNU libc) 2.39" }))).toBe(
      "@zerobox/cli-linux-x64",
    );
  });

  it("returns musl linux package on Alpine", () => {
    expect(
      platformPackage(
        makeEnv({
          arch: "x64",
          linkerExists: (p) => p === "/lib/ld-musl-x86_64.so.1",
        }),
      ),
    ).toBe("@zerobox/cli-linux-x64-musl");
  });

  it("returns musl linux arm64 package on Alpine ARM", () => {
    expect(
      platformPackage(
        makeEnv({
          arch: "arm64",
          linkerExists: (p) => p === "/lib/ld-musl-aarch64.so.1",
        }),
      ),
    ).toBe("@zerobox/cli-linux-arm64-musl");
  });

  it("returns undefined for unsupported platform", () => {
    expect(platformPackage(makeEnv({ platform: "freebsd" }))).toBeUndefined();
  });

  it("returns undefined for unsupported arch", () => {
    expect(platformPackage(makeEnv({ platform: "darwin", arch: "s390x" }))).toBeUndefined();
  });

  it("returns undefined when musl is detected but arch is unsupported", () => {
    // Exercises the musl → glibc fallback (`MUSL_PLATFORMS[key] ?? GLIBC_PLATFORMS[key]`).
    // With today's dicts the fallback can never produce a value because no linux
    // arch is glibc-only, but this guards the branch if they ever diverge.
    expect(
      platformPackage(
        makeEnv({
          arch: "s390x",
          linkerExists: () => true,
        }),
      ),
    ).toBeUndefined();
  });
});
