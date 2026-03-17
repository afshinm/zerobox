import type { SnapshotInfo } from './types.js';

export class Snapshot {
  private _info: SnapshotInfo;

  constructor(info: SnapshotInfo) {
    this._info = info;
  }

  static async get(_opts: { snapshotId: string }): Promise<Snapshot> {
    throw new Error('TODO: Phase 2');
  }

  get snapshotId(): string { return this._info.snapshotId; }
  get sourceSandboxId(): string { return this._info.sourceSandboxId; }
  get status(): SnapshotInfo['status'] { return this._info.status; }

  async delete(): Promise<void> {
    throw new Error('TODO: Phase 2');
  }
}
