import { execFile } from "node:child_process";
import { SandboxCommandError } from "./errors.js";
import type { CommandOptions, CommandOutput } from "./types.js";

/** Execute a command and return a promise of the raw output. */
function run(bin: string, argv: string[], signal?: AbortSignal): Promise<CommandOutput> {
  return new Promise((resolve, reject) => {
    let settled = false;

    const settle = <T>(fn: (value: T) => void, value: T) => {
      if (!settled) {
        settled = true;
        fn(value);
      }
    };

    if (signal?.aborted) {
      reject(new DOMException("The operation was aborted.", "AbortError"));
      return;
    }

    const child = execFile(bin, argv, { maxBuffer: 50 * 1024 * 1024 }, (error, stdout, stderr) => {
      if (error && (error as NodeJS.ErrnoException).code === "ENOENT") {
        settle(
          reject,
          new Error(`zerobox binary not found at "${bin}". Set ZEROBOX_BIN or install it.`),
        );
        return;
      }
      let code = 0;
      if (error) {
        code = typeof error.code === "number" ? error.code : 1;
      }
      settle(resolve, { code, stdout, stderr });
    });

    if (signal) {
      const onAbort = () => {
        child.kill();
        settle(reject, new DOMException("The operation was aborted.", "AbortError"));
      };
      signal.addEventListener("abort", onAbort, { once: true });
      child.on("close", () => {
        signal.removeEventListener("abort", onAbort);
      });
    }
  });
}

/**
 * A builder returned by `sandbox.sh` or `sandbox.exec`.
 * Chain with `.text()`, `.json()`, or `.output()` to execute.
 */
export class ShellCommand {
  private readonly bin: string;
  private readonly argv: string[];

  constructor(bin: string, flags: string[], cmd: string, args: string[]) {
    this.bin = bin;
    this.argv = [...flags, "--", cmd, ...args];
  }

  /** Execute and return stdout as a string. Throws on non-zero exit. */
  async text(options?: CommandOptions): Promise<string> {
    const result = await run(this.bin, this.argv, options?.signal);
    if (result.code !== 0) {
      throw new SandboxCommandError(result);
    }
    return result.stdout;
  }

  /** Execute and parse stdout as JSON. Throws on non-zero exit. */
  async json<T = unknown>(options?: CommandOptions): Promise<T> {
    const text = await this.text(options);
    return JSON.parse(text) as T;
  }

  /** Execute and return raw output. Does NOT throw on non-zero exit. */
  async output(options?: CommandOptions): Promise<CommandOutput> {
    return run(this.bin, this.argv, options?.signal);
  }
}
