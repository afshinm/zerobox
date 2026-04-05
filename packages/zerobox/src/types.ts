/** Options for creating a Sandbox instance. Maps to zerobox CLI flags. */
export interface SandboxOptions {
  /** Named profile to use. Defaults to "workspace" (CWD read/write, no network). */
  profile?: string;
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
  /** Require full sandbox. Fail instead of falling back to weaker isolation. */
  strictSandbox?: boolean;
  /** Show sandbox configuration and runtime decisions on stderr. */
  debug?: boolean;
  /** Record filesystem changes during execution. */
  snapshot?: boolean;
  /** Record and restore tracked files to pre-execution state after exit. */
  restore?: boolean;
  /** Paths to track for snapshots (default: cwd). */
  snapshotPaths?: string[];
  /** Exclude patterns from snapshots. */
  snapshotExclude?: string[];
  /** Explicit environment variables for the sandbox. */
  env?: Record<string, string>;
  /** Inherit parent env vars. true = all (default), string[] = only listed keys. */
  allowEnv?: boolean | string[];
  /** Drop these parent env vars. Takes precedence over allowEnv. */
  denyEnv?: string[];
  /**
   * Secrets with host-scoped network access. Each key becomes an env var
   * in the sandbox. The hosts are merged into network permissions.
   */
  secrets?: Record<string, SecretConfig>;
}

/** Configuration for a secret passed to the sandbox. */
export interface SecretConfig {
  /** The secret value (e.g., an API key). */
  value: string;
  /** Domains where this secret is needed. Merged into allowNet. */
  hosts: string[];
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
