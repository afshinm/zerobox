import type { SandboxCreateOptions, SandboxInfo, SandboxListOptions, SandboxSummary, RunCommandOptions, CommandOpts, FileWrite, ClientConfig } from './types.js';
import type { Command, CommandFinished } from './command.js';
import type { Snapshot } from './snapshot.js';

export class Sandbox {
  private _info: SandboxInfo;

  private static _config: ClientConfig = {
    endpoint: process.env.ZEROBOX_ENDPOINT ?? 'http://localhost:7000',
    token: process.env.ZEROBOX_TOKEN,
  };

  private constructor(info: SandboxInfo) {
    this._info = info;
  }

  static configure(config: Partial<ClientConfig>): void {
    if (config.endpoint) Sandbox._config.endpoint = config.endpoint;
    if (config.token) Sandbox._config.token = config.token;
  }

  // Static methods
  static async create(_opts?: SandboxCreateOptions): Promise<Sandbox> {
    throw new Error('TODO: Phase 2');
  }

  static async get(_opts: { sandboxId: string }): Promise<Sandbox> {
    throw new Error('TODO: Phase 2');
  }

  static async list(_opts?: SandboxListOptions): Promise<{ sandboxes: SandboxSummary[] }> {
    throw new Error('TODO: Phase 2');
  }

  // Accessors
  get sandboxId(): string { return this._info.sandboxId; }
  get status(): SandboxInfo['status'] { return this._info.status; }
  get timeout(): number { return this._info.timeout; }
  get createdAt(): Date { return new Date(this._info.createdAt); }

  // Instance methods
  async runCommand(_cmd: string, _args?: string[], _opts?: CommandOpts): Promise<CommandFinished>;
  async runCommand(_opts: RunCommandOptions): Promise<CommandFinished | Command>;
  async runCommand(): Promise<any> {
    throw new Error('TODO: Phase 2');
  }

  async mkDir(_path: string): Promise<void> {
    throw new Error('TODO: Phase 2');
  }

  async readFile(_opts: { path: string }): Promise<ReadableStream | null> {
    throw new Error('TODO: Phase 2');
  }

  async readFileToBuffer(_opts: { path: string }): Promise<Buffer | null> {
    throw new Error('TODO: Phase 2');
  }

  async downloadFile(_src: { path: string }, _dst: { path: string }): Promise<string | null> {
    throw new Error('TODO: Phase 2');
  }

  async writeFiles(_files: FileWrite[]): Promise<void> {
    throw new Error('TODO: Phase 2');
  }

  domain(_port: number): string {
    throw new Error('TODO: Phase 2');
  }

  async stop(): Promise<void> {
    throw new Error('TODO: Phase 2');
  }

  async extendTimeout(_durationMs: number): Promise<void> {
    throw new Error('TODO: Phase 2');
  }

  async snapshot(): Promise<Snapshot> {
    throw new Error('TODO: Phase 2');
  }

  async getCommand(_cmdId: string): Promise<Command> {
    throw new Error('TODO: Phase 2');
  }
}
