/** Options for creating a Sandbox instance. Maps to zerobox CLI flags. */
export interface SandboxOptions {
  /** Restrict readable paths. System libraries remain accessible. */
  allowRead?: string[];
  /** Block reading from these paths. Takes precedence over allowRead. */
  denyRead?: string[];
  /** Allow writing to these paths. Empty array = allow all writes. */
  allowWrite?: string[];
  /** Block writing to these paths. Takes precedence over allowWrite. */
  denyWrite?: string[];
  /** Allow network. true = all, string[] = specific domains. */
  allowNet?: boolean | string[];
  /** Block network to these domains. Takes precedence over allowNet. */
  denyNet?: string[];
  /** Grant all permissions (no sandbox). */
  allowAll?: boolean;
  /** Working directory for sandboxed commands. */
  cwd?: string;
  /** Disable the sandbox entirely. */
  noSandbox?: boolean;
}

/** Raw output from a sandboxed command. */
export interface CommandOutput {
  /** Exit code of the process. */
  code: number;
  /** Captured stdout as string. */
  stdout: string;
  /** Captured stderr as string. */
  stderr: string;
}

/** Options for command execution. */
export interface CommandOptions {
  /** Abort signal for cancellation. */
  signal?: AbortSignal;
}
