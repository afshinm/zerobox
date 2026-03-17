export interface SandboxCreateOptions {
  source?: {
    type: 'git' | 'snapshot' | 'image';
    url?: string;
    snapshotId?: string;
    image?: string;
  };
  resources?: {
    vcpus?: number;
    memoryMib?: number;
    diskMib?: number;
  };
  ports?: number[];
  timeout?: number;
  env?: Record<string, string>;
  networkPolicy?: {
    outbound: 'allow' | 'deny';
  };
}

export interface SandboxInfo {
  sandboxId: string;
  status: 'pending' | 'running' | 'stopping' | 'stopped' | 'failed';
  ip?: string;
  createdAt: string;
  timeout: number;
  ports: Record<string, string>;
}

export interface SandboxSummary {
  sandboxId: string;
  status: string;
  createdAt: string;
}

export interface SandboxListOptions {
  limit?: number;
  offset?: number;
}

export interface RunCommandOptions {
  cmd: string;
  args?: string[];
  cwd?: string;
  env?: Record<string, string>;
  sudo?: boolean;
  detached?: boolean;
}

export interface CommandInfo {
  cmdId: string;
  exitCode: number | null;
  startedAt: number;
  cwd: string;
}

export interface CommandOpts {
  cwd?: string;
  env?: Record<string, string>;
  sudo?: boolean;
}

export interface FilePath {
  path: string;
}

export interface FileWrite {
  path: string;
  content: Buffer;
}

export interface SnapshotInfo {
  snapshotId: string;
  sourceSandboxId: string;
  status: 'created' | 'deleted' | 'failed';
  createdAt: string;
}

export interface ClientConfig {
  endpoint: string;
  token?: string;
}
