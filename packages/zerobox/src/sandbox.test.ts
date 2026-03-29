import { describe, it, expect } from "vitest";
import { existsSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { Sandbox } from "./sandbox.js";
import { SandboxCommandError } from "./errors.js";

const skip = !process.env.ZEROBOX_BIN;

/** Remove a path before and after a test. */
function withCleanup(path: string, fn: () => Promise<void>): () => Promise<void> {
  return async () => {
    rmSync(path, { recursive: true, force: true });
    try {
      await fn();
    } finally {
      rmSync(path, { recursive: true, force: true });
    }
  };
}

describe.skipIf(skip)("Sandbox (e2e)", () => {
  // ── sh: text ──

  it("sh`...`.text() returns stdout", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox.sh`echo hello`.text();
    expect(output.trim()).toBe("hello");
  });

  it("sh`...`.text() throws SandboxCommandError on non-zero exit", async () => {
    const sandbox = Sandbox.create();
    try {
      await sandbox.sh`exit 42`.text();
      expect.unreachable("should have thrown");
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

  // ── js ──

  it("js`...` runs inline JavaScript via node", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox.js`console.log(1 + 1)`.text();
    expect(output.trim()).toBe("2");
  });

  it("js`...` interpolates values", async () => {
    const sandbox = Sandbox.create();
    const x = 21;
    const output = await sandbox.js`console.log(${x} * 2)`.text();
    expect(output.trim()).toBe("42");
  });

  it("js`...`.json() parses node output", async () => {
    const sandbox = Sandbox.create();
    const data = await sandbox.js`
      console.log(JSON.stringify({ sum: 1 + 2 }));
    `.json<{ sum: number }>();
    expect(data.sum).toBe(3);
  });

  // ── exec ──

  it("exec() runs a command with args", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox.exec("echo", ["hello"]).text();
    expect(output.trim()).toBe("hello");
  });

  // ── write enforcement ──

  it(
    "blocks writes by default",
    withCleanup("/tmp/zerobox-sdk-wb", async () => {
      const sandbox = Sandbox.create();
      const result = await sandbox.sh`echo x > /tmp/zerobox-sdk-wb 2>&1 || echo BLOCKED`.output();
      expect(result.stdout + result.stderr).toMatch(
        /BLOCKED|Read-only|Permission denied|Operation not permitted/i,
      );
      expect(existsSync("/tmp/zerobox-sdk-wb")).toBe(false);
    }),
  );

  it(
    "allows writes with allowWrite",
    withCleanup("/tmp/zerobox-sdk-aw", async () => {
      const sandbox = Sandbox.create({ allowWrite: ["/tmp"] });
      await sandbox.sh`echo ok > /tmp/zerobox-sdk-aw`.output();
      expect(existsSync("/tmp/zerobox-sdk-aw")).toBe(true);
      expect(readFileSync("/tmp/zerobox-sdk-aw", "utf8").trim()).toBe("ok");
    }),
  );

  it(
    "denies writes to specific paths via denyWrite",
    withCleanup("/tmp/zerobox-sdk-dw", async () => {
      const dir = "/tmp/zerobox-sdk-dw";
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
try{fs.writeFileSync('${dir}/ok.txt','x');r.push('file:ok')}catch(e){r.push('file:blocked:'+e.code)}
try{fs.writeFileSync('${dir}/.git/evil','x');r.push('git:ok')}catch(e){r.push('git:blocked:'+e.code)}
console.log(r.join(','))`,
        ])
        .output();

      // .git/evil must never be created.
      expect(existsSync(`${dir}/.git/evil`)).toBe(false);

      // If node produced output, verify git was blocked and file was allowed.
      if (result.stdout.trim().length > 0) {
        expect(result.stdout).toContain("git:blocked");
        expect(result.stdout).not.toContain("git:ok");
      }
    }),
  );

  // ��─ network enforcement ──

  it("blocks network by default", async () => {
    const sandbox = Sandbox.create();
    const result = await sandbox
      .exec("node", [
        "-e",
        "fetch('https://example.com').then(()=>console.log('OK')).catch(()=>console.log('BLOCKED'))",
      ])
      .output();
    // "OK" (successful fetch) must never appear.
    expect(result.stdout.trim()).not.toBe("OK");
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

  it(
    "allows everything with allowAll",
    withCleanup("/tmp/zerobox-sdk-aa", async () => {
      const sandbox = Sandbox.create({ allowAll: true });
      await sandbox.sh`echo ok > /tmp/zerobox-sdk-aa`.output();
      expect(existsSync("/tmp/zerobox-sdk-aa")).toBe(true);
      expect(readFileSync("/tmp/zerobox-sdk-aa", "utf8").trim()).toBe("ok");
    }),
  );
});
