import type { CommandOutput } from "./types.js";

/** Thrown when a sandboxed command exits with a non-zero code. */
export class SandboxCommandError extends Error {
  readonly code: number;
  readonly stdout: string;
  readonly stderr: string;

  constructor(output: CommandOutput) {
    const message =
      output.stderr.trim() || output.stdout.trim() || `command exited with code ${output.code}`;
    super(message);
    this.name = "SandboxCommandError";
    this.code = output.code;
    this.stdout = output.stdout;
    this.stderr = output.stderr;
  }
}
