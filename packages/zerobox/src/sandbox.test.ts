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

  it("blocks writes outside allowed paths", async () => {
    const sandbox = Sandbox.create();
    const result = await sandbox.sh`echo x > /var/zerobox-sdk-wb 2>&1 || echo BLOCKED`.output();
    expect(result.stdout + result.stderr).toMatch(
      /BLOCKED|Read-only|Permission denied|Operation not permitted/i,
    );
  });

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
        cwd: dir,
        allowRead: [dir],
        allowWrite: [dir],
        denyWrite: [`${dir}/.git`],
      });

      const output = await sandbox
        .exec("node", [
          "-e",
          `const fs=require('fs');
let r=[];
try{fs.writeFileSync('${dir}/ok.txt','x');r.push('file:ok')}catch(e){r.push('file:blocked:'+e.code)}
try{fs.writeFileSync('${dir}/.git/evil','x');r.push('git:ok')}catch(e){r.push('git:blocked:'+e.code)}
console.log(r.join(','))`,
        ])
        .text();

      // .git/evil must never be created.
      expect(existsSync(`${dir}/.git/evil`)).toBe(false);
      expect(output).toContain("git:blocked");
      expect(output).not.toContain("git:ok");
    }),
  );

  // ��─ network enforcement ──

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

  it(
    "allows everything with allowAll",
    withCleanup("/tmp/zerobox-sdk-aa", async () => {
      const sandbox = Sandbox.create({ allowAll: true });
      await sandbox.sh`echo ok > /tmp/zerobox-sdk-aa`.output();
      expect(existsSync("/tmp/zerobox-sdk-aa")).toBe(true);
      expect(readFileSync("/tmp/zerobox-sdk-aa", "utf8").trim()).toBe("ok");
    }),
  );

  // ── env vars ──

  it("default env excludes custom parent vars", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox.sh`echo $ZEROBOX_TEST_CUSTOM`.text();
    expect(output.trim()).toBe("");
  });

  it("default env includes PATH", async () => {
    const sandbox = Sandbox.create();
    const output = await sandbox.sh`echo $PATH`.text();
    expect(output.trim()).not.toBe("");
  });

  it("env option sets explicit vars", async () => {
    const sandbox = Sandbox.create({ env: { MY_VAR: "hello" } });
    const output = await sandbox.sh`echo $MY_VAR`.text();
    expect(output.trim()).toBe("hello");
  });

  it("env option with multiple vars", async () => {
    const sandbox = Sandbox.create({ env: { A: "1", B: "2" } });
    const output = await sandbox.sh`echo $A $B`.text();
    expect(output.trim()).toBe("1 2");
  });

  it("allowEnv: true inherits all parent vars", async () => {
    const sandbox = Sandbox.create({ allowEnv: true });
    const output = await sandbox.sh`env`.text();
    const count = output.trim().split("\n").length;
    expect(count).toBeGreaterThan(10);
  });

  it("allowEnv with specific keys inherits only those", async () => {
    const sandbox = Sandbox.create({ allowEnv: ["PATH"] });
    const output = await sandbox.sh`env`.text();
    const lines = output.trim().split("\n");
    expect(lines.some((l) => l.startsWith("PATH="))).toBe(true);
    // HOME should not be its own env var (CODEX_HOME is different and OK).
    expect(lines.some((l) => l.startsWith("HOME="))).toBe(false);
  });

  it("denyEnv removes vars", async () => {
    const sandbox = Sandbox.create({ allowEnv: true, denyEnv: ["HOME"] });
    const output = await sandbox.sh`echo "HOME=$HOME"`.text();
    expect(output.trim()).toBe("HOME=");
  });

  it("denyEnv does not block explicit env", async () => {
    const sandbox = Sandbox.create({ denyEnv: ["FOO"], env: { FOO: "override" } });
    const output = await sandbox.sh`echo $FOO`.text();
    expect(output.trim()).toBe("override");
  });

  it("env value with equals sign", async () => {
    const sandbox = Sandbox.create({ env: { DATA: "a=b=c" } });
    const output = await sandbox.sh`echo $DATA`.text();
    expect(output.trim()).toBe("a=b=c");
  });

  // ── secrets ──

  it("secret env var contains placeholder, not real value", async () => {
    const sandbox = Sandbox.create({
      secrets: {
        API_KEY: { value: "sk-test-123", hosts: ["example.com"] },
      },
    });
    const output = await sandbox.sh`echo $API_KEY`.text();
    expect(output.trim()).toMatch(/^ZEROBOX_SECRET_[0-9a-f]{64}$/);
    expect(output.trim()).not.toBe("sk-test-123");
  });

  it("secrets auto-enable network for their hosts", async () => {
    const sandbox = Sandbox.create({
      secrets: {
        TOKEN: { value: "t", hosts: ["httpbin.org"] },
      },
    });
    // -k: accept MITM proxy cert (secrets enable MITM for header substitution)
    const result = await sandbox
      .exec("curl", ["-sk", "-o", "/dev/null", "-w", "%{http_code}", "https://httpbin.org/get"])
      .text();
    expect(result.trim()).toBe("200");
  });

  it("secret header substituted for matching host", async () => {
    const sandbox = Sandbox.create({
      secrets: {
        MY_SECRET: { value: "real-value", hosts: ["httpbin.org"] },
      },
    });
    const output =
      await sandbox.sh`curl -sk -H "X-Test: $MY_SECRET" https://httpbin.org/headers`.json<{
        headers: Record<string, string>;
      }>();
    expect(output.headers["X-Test"]).toBe("real-value");
  });

  it("secret NOT substituted for wrong host", async () => {
    const sandbox = Sandbox.create({
      allowNet: true,
      secrets: {
        MY_SECRET: { value: "real-value", hosts: ["other.com"] },
      },
    });
    const output =
      await sandbox.sh`curl -sk -H "X-Test: $MY_SECRET" https://httpbin.org/headers`.json<{
        headers: Record<string, string>;
      }>();
    expect(output.headers["X-Test"]).toMatch(/^ZEROBOX_SECRET_/);
  });

  it("multiple secrets with different hosts", async () => {
    const sandbox = Sandbox.create({
      secrets: {
        SECRET_A: { value: "value-a", hosts: ["httpbin.org"] },
        SECRET_B: { value: "value-b", hosts: ["other.com"] },
      },
      allowNet: true,
    });
    const output =
      await sandbox.sh`curl -sk -H "X-A: $SECRET_A" -H "X-B: $SECRET_B" https://httpbin.org/headers`.json<{
        headers: Record<string, string>;
      }>();
    // A is for httpbin.org → substituted. B is for other.com → placeholder.
    expect(output.headers["X-A"]).toBe("value-a");
    expect(output.headers["X-B"]).toMatch(/^ZEROBOX_SECRET_/);
  });

  it("secret host restriction blocks other hosts", async () => {
    const sandbox = Sandbox.create({
      secrets: {
        TOKEN: { value: "t", hosts: ["httpbin.org"] },
      },
    });
    // httpbin.org should work (secret host), but example.com should be blocked.
    const result = await sandbox
      .exec("curl", [
        "-sk",
        "--max-time",
        "3",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "https://example.com",
      ])
      .output();
    expect(result.stdout.trim()).not.toBe("200");
  });

  it("env and secrets work together", async () => {
    const sandbox = Sandbox.create({
      env: { MY_VAR: "env-val" },
      secrets: {
        MY_SECRET: { value: "secret-val", hosts: ["httpbin.org"] },
      },
    });
    const envOut = await sandbox.sh`echo $MY_VAR`.text();
    expect(envOut.trim()).toBe("env-val");
    const secretOut = await sandbox.sh`echo $MY_SECRET`.text();
    expect(secretOut.trim()).toMatch(/^ZEROBOX_SECRET_/);
  });
});
