import { describe, it, expect } from "vitest";
import { buildFlags } from "./flags.js";

describe("buildFlags", () => {
  it("returns empty array for default options", () => {
    expect(buildFlags({})).toEqual([]);
  });

  it("returns --allow-all and nothing else", () => {
    expect(buildFlags({ allowAll: true, allowWrite: ["/tmp"] })).toEqual(["--allow-all"]);
  });

  it("returns --no-sandbox and nothing else", () => {
    expect(buildFlags({ noSandbox: true, allowWrite: ["/tmp"] })).toEqual(["--no-sandbox"]);
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
});
