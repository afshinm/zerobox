import { describe, it, expect } from "vitest";
import { buildFlags } from "./flags.js";

describe("buildFlags", () => {
  it("defaults to workspace profile", () => {
    expect(buildFlags({})).toEqual(["--profile", "workspace"]);
  });

  it("uses custom profile when specified", () => {
    expect(buildFlags({ profile: "claude" })).toEqual(["--profile", "claude"]);
  });

  it("emits one --profile per entry for an array", () => {
    expect(buildFlags({ profile: ["workspace", "git-config"] })).toEqual([
      "--profile",
      "workspace",
      "--profile",
      "git-config",
    ]);
  });

  it("single-element array emits the same flag as a string", () => {
    expect(buildFlags({ profile: ["claude"] })).toEqual(buildFlags({ profile: "claude" }));
  });

  it("falls back to workspace for an empty profile array", () => {
    expect(buildFlags({ profile: [] })).toEqual(["--profile", "workspace"]);
  });

  it("returns --allow-all without fs/net/profile flags", () => {
    expect(buildFlags({ allowAll: true, allowWrite: ["/tmp"] })).toEqual(["--allow-all"]);
  });

  it("returns --no-sandbox without profile flags", () => {
    expect(buildFlags({ noSandbox: true, allowWrite: ["/tmp"] })).toEqual(["--no-sandbox"]);
  });

  it("builds --strict-sandbox flag", () => {
    const flags = buildFlags({ strictSandbox: true, allowWrite: ["/tmp"] });
    expect(flags).toContain("--strict-sandbox");
    expect(flags).toContain("--allow-write=/tmp");
  });

  it("builds --allow-read with comma-separated paths", () => {
    const flags = buildFlags({ allowRead: ["/tmp", "/data"] });
    expect(flags).toContain("--allow-read=/tmp,/data");
  });

  it("builds --deny-read", () => {
    const flags = buildFlags({ denyRead: ["/secret"] });
    expect(flags).toContain("--deny-read=/secret");
  });

  it("builds --allow-write with paths", () => {
    const flags = buildFlags({ allowWrite: ["/tmp"] });
    expect(flags).toContain("--allow-write=/tmp");
  });

  it("builds --deny-write", () => {
    const flags = buildFlags({ denyWrite: [".git"] });
    expect(flags).toContain("--deny-write=.git");
  });

  it("builds --allow-net as boolean", () => {
    const flags = buildFlags({ allowNet: true });
    expect(flags).toContain("--allow-net");
  });

  it("builds --allow-net with domains", () => {
    const flags = buildFlags({ allowNet: ["example.com", "api.example.com"] });
    expect(flags).toContain("--allow-net=example.com,api.example.com");
  });

  it("ignores allowNet: false", () => {
    const flags = buildFlags({ allowNet: false });
    expect(flags).not.toContain("--allow-net");
  });

  it("ignores empty allowNet array", () => {
    const flags = buildFlags({ allowNet: [] });
    expect(flags.filter((f) => f.startsWith("--allow-net")).length).toBe(0);
  });

  it("builds --deny-net", () => {
    const flags = buildFlags({ denyNet: ["evil.com"] });
    expect(flags).toContain("--deny-net=evil.com");
  });

  it("builds -C for cwd", () => {
    const flags = buildFlags({ cwd: "/workspace" });
    expect(flags).toContain("-C");
    expect(flags).toContain("/workspace");
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
    expect(flags).toContain("--profile");
    expect(flags).toContain("--allow-read=/tmp");
    expect(flags).toContain("--deny-read=/tmp/secret");
    expect(flags).toContain("--allow-write=/tmp");
    expect(flags).toContain("--deny-write=/tmp/.git");
    expect(flags).toContain("--allow-net=example.com");
    expect(flags).toContain("--deny-net=evil.com");
    expect(flags).toContain("-C");
    expect(flags).toContain("/workspace");
  });

  it("skips empty arrays", () => {
    const flags = buildFlags({ allowRead: [], denyRead: [], allowWrite: [], denyWrite: [] });
    expect(flags).toEqual(["--profile", "workspace"]);
  });

  // env

  it("builds --env flags", () => {
    const flags = buildFlags({ env: { FOO: "bar" } });
    expect(flags).toContain("--env");
    expect(flags).toContain("FOO=bar");
  });

  it("builds multiple --env flags", () => {
    const flags = buildFlags({ env: { A: "1", B: "2" } });
    expect(flags).toContain("--env");
    expect(flags).toContain("A=1");
    expect(flags).toContain("B=2");
  });

  it("builds --allow-env as boolean", () => {
    const flags = buildFlags({ allowEnv: true });
    expect(flags).toContain("--allow-env");
  });

  it("builds --allow-env with keys", () => {
    const flags = buildFlags({ allowEnv: ["PATH", "HOME"] });
    expect(flags).toContain("--allow-env=PATH,HOME");
  });

  it("builds --deny-env", () => {
    const flags = buildFlags({ denyEnv: ["SECRET"] });
    expect(flags).toContain("--deny-env=SECRET");
  });

  // secrets

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
    expect(flags.filter((f) => f.startsWith("--allow-net")).length).toBe(0);
  });

  it("allowEnv: false does not emit flag", () => {
    const flags = buildFlags({ allowEnv: false });
    expect(flags).not.toContain("--allow-env");
  });

  it("allowEnv: [] does not emit flag", () => {
    const flags = buildFlags({ allowEnv: [] });
    expect(flags).not.toContain("--allow-env");
  });

  // bind mounts

  it("emits a single --bind-mount entry", () => {
    const flags = buildFlags({
      bindMounts: [{ host: "/tmp/proj-abc", sandbox: "/tmp" }],
    });
    expect(flags).toContain("--bind-mount");
    expect(flags).toContain("/tmp/proj-abc:/tmp");
  });

  it("appends :ro for read-only mounts", () => {
    const flags = buildFlags({
      bindMounts: [{ host: "/var/cache/pkg", sandbox: "/var/cache/pkg", readOnly: true }],
    });
    expect(flags).toContain("--bind-mount");
    expect(flags).toContain("/var/cache/pkg:/var/cache/pkg:ro");
  });

  it("preserves Windows drive-letter paths", () => {
    const flags = buildFlags({
      bindMounts: [
        {
          host: String.raw`C:\host\a`,
          sandbox: String.raw`D:\sandbox\a`,
          readOnly: true,
        },
      ],
    });
    expect(flags).toContain(String.raw`C:\host\a:D:\sandbox\a:ro`);
  });

  it("preserves argv order across multiple mounts", () => {
    const flags = buildFlags({
      bindMounts: [
        { host: "/host/a", sandbox: "/a" },
        { host: "/host/b", sandbox: "/a/b", readOnly: true },
        { host: "/host/c", sandbox: "/c" },
      ],
    });
    // Collect the spec values that follow each --bind-mount flag.
    const specs: string[] = [];
    for (let i = 0; i < flags.length; i++) {
      if (flags[i] === "--bind-mount") {
        const next = flags[i + 1];
        if (next !== undefined) {
          specs.push(next);
        }
      }
    }
    expect(specs).toEqual(["/host/a:/a", "/host/b:/a/b:ro", "/host/c:/c"]);
  });

  it("omits --bind-mount when bindMounts is missing or empty", () => {
    expect(buildFlags({}).filter((f) => f === "--bind-mount").length).toBe(0);
    expect(buildFlags({ bindMounts: [] }).filter((f) => f === "--bind-mount").length).toBe(0);
  });

  it("bind mounts coexist with allowAll", () => {
    const flags = buildFlags({
      allowAll: true,
      bindMounts: [{ host: "/host", sandbox: "/sandbox" }],
    });
    expect(flags).toContain("--allow-all");
    expect(flags).toContain("--bind-mount");
    expect(flags).toContain("/host:/sandbox");
  });

  it("bind mounts coexist with noSandbox", () => {
    const flags = buildFlags({
      noSandbox: true,
      bindMounts: [{ host: "/host", sandbox: "/sandbox", readOnly: true }],
    });
    expect(flags).toContain("--no-sandbox");
    expect(flags).toContain("--bind-mount");
    expect(flags).toContain("/host:/sandbox:ro");
  });
});
