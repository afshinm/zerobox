import { describe, it, expect } from "vitest";
import { mkdirSync, rmSync } from "node:fs";
import { Sandbox } from "./sandbox.js";
import { SandboxCommandError } from "./errors.js";

const skip = !process.env.ZEROBOX_BIN;

describe.skipIf(skip)("Sandbox (e2e)", () => {
  // ── sh: text ──

  it("sh`...`.text() returns stdout", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox.sh`echo hello`.text();
    expect(output.trim()).toBe("hello");
  });

  it("sh`...`.text() throws SandboxCommandError on non-zero exit", async () => {
    const sandbox = Sandbox.create();
    await expect(sandbox.sh`exit 42`.text()).rejects.toThrow(SandboxCommandError);
    try {
      await sandbox.sh`exit 42`.text();
    } catch (e) {
      expect(e).toBeInstanceOf(SandboxCommandError);
      expect((e as SandboxCommandError).code).toBe(42);
    }
  });

  // ── sh: json ──

  it("sh`...`.json() parses stdout as JSON", async () => {
    const sandbox = Sandbox.create();
    const data = await sandbox.sh`echo '{"key":"value"}'`.json<{ key: string }>();
    expect(data.key).toBe("value");
  });

  // ── sh: output ──

  it("sh`...`.output() returns raw result without throwing", async () => {
    const sandbox = Sandbox.create();
    const result = await sandbox.sh`exit 42`.output();
    expect(result.code).toBe(42);
  });

  it("sh`...`.output() captures stdout and stderr", async () => {
    const sandbox = Sandbox.create();
    const result = await sandbox.sh`echo out && echo err >&2`.output();
    expect(result.code).toBe(0);
    expect(result.stdout.trim()).toBe("out");
    expect(result.stderr.trim()).toBe("err");
  });

  // ── sh: interpolation ──

  it("sh interpolates template values", async () => {
    const sandbox = Sandbox.create();
    const name = "world";
    const output = await sandbox.sh`echo hello ${name}`.text();
    expect(output.trim()).toBe("hello world");
  });

  // ── exec ──

  it("exec() runs a command with args", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox.exec("echo", ["hello"]).text();
    expect(output.trim()).toBe("hello");
  });

  // ── write enforcement ──

  it("blocks writes by default", async () => {
    const sandbox = Sandbox.create();
    const result =
      await sandbox.sh`sh -c "echo x > /tmp/zerobox-sdk-write-test" 2>&1 || echo BLOCKED`.output();
    expect(result.stdout + result.stderr).toMatch(
      /BLOCKED|Read-only|Permission denied|Operation not permitted/i,
    );
  });

  it("allows writes with allowWrite", async () => {
    const sandbox = Sandbox.create({ allowWrite: ["/tmp"] });
    const output =
      await sandbox.sh`echo ok > /tmp/zerobox-sdk-aw && cat /tmp/zerobox-sdk-aw`.text();
    expect(output.trim()).toBe("ok");
  });

  it("denies writes to specific paths via denyWrite", async () => {
    const dir = "/tmp/zerobox-sdk-dw";
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(`${dir}/.git`, { recursive: true });

    const sandbox = Sandbox.create({
      allowWrite: [dir],
      denyWrite: [`${dir}/.git`],
    });

    const result = await sandbox
      .exec("node", [
        "-e",
        `const fs=require('fs');
let r=[];
try{fs.writeFileSync('${dir}/ok.txt','x');r.push('file:ok')}catch(e){r.push('file:blocked')}
try{fs.writeFileSync('${dir}/.git/evil','x');r.push('git:ok')}catch(e){r.push('git:blocked')}
console.log(r.join(','))`,
      ])
      .text();

    expect(result).toContain("file:ok");
    expect(result).toContain("git:blocked");
    rmSync(dir, { recursive: true, force: true });
  });

  // ── network enforcement ──

  it("blocks network by default", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox
      .exec("node", [
        "-e",
        "fetch('https://example.com').then(()=>console.log('OK')).catch(()=>console.log('BLOCKED'))",
      ])
      .text();
    expect(output.trim()).toBe("BLOCKED");
  });

  it("allows network with allowNet: true", async () => {
    const sandbox = Sandbox.create({ allowNet: true });
    const output = await sandbox
      .exec("curl", ["-s", "-o", "/dev/null", "-w", "%{http_code}", "https://example.com"])
      .text();
    expect(output.trim()).toBe("200");
  });

  it("allows specific domain via allowNet", async () => {
    const sandbox = Sandbox.create({ allowNet: ["example.com"] });
    const output = await sandbox
      .exec("curl", ["-s", "-o", "/dev/null", "-w", "%{http_code}", "https://example.com"])
      .text();
    expect(output.trim()).toBe("200");
  });

  it("blocks unlisted domain", async () => {
    const sandbox = Sandbox.create({ allowNet: ["example.com"] });
    const result = await sandbox
      .exec("curl", [
        "-s",
        "--max-time",
        "5",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "https://google.com",
      ])
      .output();
    expect(result.stdout.trim()).not.toBe("200");
  });

  // ── cancellation ──

  it("supports cancellation via AbortSignal", async () => {
    const sandbox = Sandbox.create();
    const controller = new AbortController();
    controller.abort();
    await expect(sandbox.sh`sleep 60`.text({ signal: controller.signal })).rejects.toThrow(
      "aborted",
    );
  });

  // ── allow-all ──

  it("allows everything with allowAll", async () => {
    const sandbox = Sandbox.create({ allowAll: true });
    const output =
      await sandbox.sh`echo ok > /tmp/zerobox-sdk-aa && cat /tmp/zerobox-sdk-aa`.text();
    expect(output.trim()).toBe("ok");
  });
});
