export class Command {
  private _cmdId: string;
  private _exitCode: number | null;
  private _cwd: string;
  private _startedAt: number;

  constructor(data: { cmdId: string; exitCode: number | null; cwd: string; startedAt: number }) {
    this._cmdId = data.cmdId;
    this._exitCode = data.exitCode;
    this._cwd = data.cwd;
    this._startedAt = data.startedAt;
  }

  get exitCode(): number | null { return this._exitCode; }
  get cmdId(): string { return this._cmdId; }
  get cwd(): string { return this._cwd; }
  get startedAt(): number { return this._startedAt; }

  async *logs(): AsyncGenerator<{ stream: 'stdout' | 'stderr'; data: string }> {
    throw new Error('TODO: Phase 2');
  }

  async wait(): Promise<CommandFinished> {
    throw new Error('TODO: Phase 2');
  }

  async output(_stream: 'stdout' | 'stderr' | 'both'): Promise<string> {
    throw new Error('TODO: Phase 2');
  }

  async stdout(): Promise<string> {
    return this.output('stdout');
  }

  async stderr(): Promise<string> {
    return this.output('stderr');
  }

  async kill(_signal?: string): Promise<void> {
    throw new Error('TODO: Phase 2');
  }
}

export class CommandFinished extends Command {
  override get exitCode(): number {
    return super.exitCode!;
  }
}
