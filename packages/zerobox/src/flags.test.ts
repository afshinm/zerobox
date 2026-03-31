import { describe, it, expect } from "vitest";
import { buildFlags } from "./flags.js";

describe("buildFlags", () => {
  it("returns empty array for default options", () => {
    expect(buildFlags({})).toEqual([]);
  });

  it("returns --allow-all without fs/net flags", () => {
    expect(buildFlags({ allowAll: true, allowWrite: ["/tmp"] })).toEqual(["--allow-all"]);
  });

  it("returns --no-sandbox and nothing else", () => {
    expect(buildFlags({ noSandbox: true, allowWrite: ["/tmp"] })).toEqual(["--no-sandbox"]);
  });

  it("builds --strict-sandbox flag", () => {
    const flags = buildFlags({ strictSandbox: true, allowWrite: ["/tmp"] });
    expect(flags).toContain("--strict-sandbox");
    expect(flags).toContain("--allow-write=/tmp");
  });

  it("builds --allow-read with comma-separated paths", () => {
    expect(buildFlags({ allowRead: ["/tmp", "/data"] })).toEqual(["--allow-read=/tmp,/data"]);
  });

  it("builds --deny-read", () => {
    expect(buildFlags({ denyRead: ["/secret"] })).toEqual(["--deny-read=/secret"]);
  });

  it("builds --allow-write with paths", () => {
    expect(buildFlags({ allowWrite: ["/tmp"] })).toEqual(["--allow-write=/tmp"]);
  });

  it("builds --deny-write", () => {
    expect(buildFlags({ denyWrite: [".git"] })).toEqual(["--deny-write=.git"]);
  });

  it("builds --allow-net as boolean", () => {
    expect(buildFlags({ allowNet: true })).toEqual(["--allow-net"]);
  });

  it("builds --allow-net with domains", () => {
    expect(buildFlags({ allowNet: ["example.com", "api.example.com"] })).toEqual([
      "--allow-net=example.com,api.example.com",
    ]);
  });

  it("ignores allowNet: false", () => {
    expect(buildFlags({ allowNet: false })).toEqual([]);
  });

  it("ignores empty allowNet array", () => {
    expect(buildFlags({ allowNet: [] })).toEqual([]);
  });

  it("builds --deny-net", () => {
    expect(buildFlags({ denyNet: ["evil.com"] })).toEqual(["--deny-net=evil.com"]);
  });

  it("builds -C for cwd", () => {
    expect(buildFlags({ cwd: "/workspace" })).toEqual(["-C", "/workspace"]);
  });

  it("combines multiple flags", () => {
    const flags = buildFlags({
      allowRead: ["/tmp"],
      denyRead: ["/tmp/secret"],
      allowWrite: ["/tmp"],
      denyWrite: ["/tmp/.git"],
      allowNet: ["example.com"],
      denyNet: ["evil.com"],
      cwd: "/workspace",
    });
    expect(flags).toEqual([
      "--allow-read=/tmp",
      "--deny-read=/tmp/secret",
      "--allow-write=/tmp",
      "--deny-write=/tmp/.git",
      "--allow-net=example.com",
      "--deny-net=evil.com",
      "-C",
      "/workspace",
    ]);
  });

  it("skips empty arrays", () => {
    expect(buildFlags({ allowRead: [], denyRead: [], allowWrite: [], denyWrite: [] })).toEqual([]);
  });

  // ── env ──

  it("builds --env flags", () => {
    expect(buildFlags({ env: { FOO: "bar" } })).toEqual(["--env", "FOO=bar"]);
  });

  it("builds multiple --env flags", () => {
    const flags = buildFlags({ env: { A: "1", B: "2" } });
    expect(flags).toContain("--env");
    expect(flags).toContain("A=1");
    expect(flags).toContain("B=2");
  });

  it("builds --allow-env as boolean", () => {
    expect(buildFlags({ allowEnv: true })).toEqual(["--allow-env"]);
  });

  it("builds --allow-env with keys", () => {
    expect(buildFlags({ allowEnv: ["PATH", "HOME"] })).toEqual(["--allow-env=PATH,HOME"]);
  });

  it("builds --deny-env", () => {
    expect(buildFlags({ denyEnv: ["SECRET"] })).toEqual(["--deny-env=SECRET"]);
  });

  // ── secrets ──

  it("emits --secret and --secret-host flags", () => {
    const flags = buildFlags({
      secrets: { API_KEY: { value: "sk-123", hosts: ["api.example.com"] } },
    });
    expect(flags).toContain("--secret");
    expect(flags).toContain("API_KEY=sk-123");
    expect(flags).toContain("--secret-host");
    expect(flags).toContain("API_KEY=api.example.com");
  });

  it("secret without hosts emits only --secret", () => {
    const flags = buildFlags({
      secrets: { TOKEN: { value: "abc", hosts: [] } },
    });
    expect(flags).toContain("--secret");
    expect(flags).toContain("TOKEN=abc");
    expect(flags).not.toContain("--secret-host");
  });

  it("merges secret hosts with existing allowNet domains", () => {
    const flags = buildFlags({
      allowNet: ["other.com"],
      secrets: { KEY: { value: "v", hosts: ["api.com"] } },
    });
    expect(flags).toContain("--allow-net=other.com,api.com");
  });

  it("secrets with allowNet: true do not duplicate net flag", () => {
    const flags = buildFlags({
      allowNet: true,
      secrets: { KEY: { value: "v", hosts: ["api.com"] } },
    });
    expect(flags).toContain("--allow-net");
    expect(flags.filter((f) => f.startsWith("--allow-net")).length).toBe(1);
  });

  it("multiple secrets produce multiple --secret flags", () => {
    const flags = buildFlags({
      secrets: {
        A: { value: "v1", hosts: ["h1.com"] },
        B: { value: "v2", hosts: ["h2.com"] },
      },
    });
    expect(flags.filter((f) => f === "--secret").length).toBe(2);
    expect(flags).toContain("A=v1");
    expect(flags).toContain("B=v2");
  });

  it("env flags are emitted even with allowAll", () => {
    const flags = buildFlags({ allowAll: true, env: { FOO: "bar" } });
    expect(flags).toContain("--allow-all");
    expect(flags).toContain("--env");
    expect(flags).toContain("FOO=bar");
  });

  it("secrets are emitted even with allowAll", () => {
    const flags = buildFlags({
      allowAll: true,
      secrets: { KEY: { value: "v", hosts: ["h.com"] } },
    });
    expect(flags).toContain("--allow-all");
    expect(flags).toContain("--secret");
    expect(flags).toContain("KEY=v");
  });

  it("denyEnv combined with secrets", () => {
    const flags = buildFlags({
      denyEnv: ["HOME"],
      secrets: { KEY: { value: "v", hosts: ["h.com"] } },
    });
    expect(flags).toContain("--deny-env=HOME");
    expect(flags).toContain("--secret");
    expect(flags).toContain("KEY=v");
  });

  it("noSandbox still emits secret flags", () => {
    const flags = buildFlags({
      noSandbox: true,
      secrets: { KEY: { value: "v", hosts: ["h.com"] } },
    });
    expect(flags).toContain("--no-sandbox");
    expect(flags).toContain("--secret");
    expect(flags).toContain("KEY=v");
  });

  it("secrets without allowNet do not emit --allow-net", () => {
    const flags = buildFlags({
      secrets: { KEY: { value: "v", hosts: ["h.com"] } },
    });
    expect(flags).toContain("--secret");
    expect(flags).toContain("--secret-host");
    // No --allow-net — CLI handles network implicitly for secret hosts.
    expect(flags.filter((f) => f.startsWith("--allow-net")).length).toBe(0);
  });

  it("allowEnv: false does not emit flag", () => {
    expect(buildFlags({ allowEnv: false })).toEqual([]);
  });

  it("allowEnv: [] does not emit flag", () => {
    expect(buildFlags({ allowEnv: [] })).toEqual([]);
  });
});
