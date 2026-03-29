import { verifyBinary } from "./binary.js";
import { ShellCommand } from "./command.js";
import { buildFlags } from "./flags.js";
import type { SandboxOptions } from "./types.js";

export class Sandbox {
  readonly options: Readonly<SandboxOptions>;
  private readonly bin: string;
  private readonly flags: string[];

  private constructor(options: SandboxOptions, bin: string) {
    this.options = Object.freeze({ ...options });
    this.bin = bin;
    this.flags = buildFlags(this.options);
  }

  /** Create a new Sandbox instance. Verifies the binary is reachable. */
  static create(options: SandboxOptions = {}): Sandbox {
    const bin = verifyBinary();
    return new Sandbox(options, bin);
  }

  /**
   * Execute a shell command via tagged template literal.
   *
   * Returns a `ShellCommand` builder. Chain with:
   * - `.text()` — stdout as string (throws on non-zero exit)
   * - `.json()` — parse stdout as JSON (throws on non-zero exit)
   * - `.output()` — raw `{ code, stdout, stderr }` (never throws)
   *
   * @example
   * ```ts
   * const output = await sandbox.sh`echo hello`.text();
   * const data = await sandbox.sh`cat data.json`.json();
   * const raw = await sandbox.sh`ls`.output();
   * ```
   */
  sh(strings: TemplateStringsArray, ...values: unknown[]): ShellCommand {
    const command = strings.reduce(
      (acc, str, i) => acc + str + (i < values.length ? String(values[i]) : ""),
      "",
    );
    return new ShellCommand(this.bin, this.flags, "sh", ["-c", command]);
  }

  /**
   * Execute a command with explicit arguments.
   *
   * Returns a `ShellCommand` builder (same `.text()`, `.json()`, `.output()` API).
   *
   * @example
   * ```ts
   * const result = await sandbox.exec("node", ["-e", "console.log('hi')"]).text();
   * ```
   */
  exec(cmd: string, args: string[] = []): ShellCommand {
    return new ShellCommand(this.bin, this.flags, cmd, args);
  }

  /** Support `await using sandbox = Sandbox.create()`. */
  async [Symbol.asyncDispose](): Promise<void> {
    // No-op. Each command is a fresh process, nothing to clean up.
  }
}
