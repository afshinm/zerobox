from __future__ import annotations

import contextlib
import os
import signal
from typing import Any, Protocol


class KillableProcess(Protocol):
    pid: int

    def kill(self) -> None: ...


def new_process_group_kwargs() -> dict[str, Any]:
    if os.name == "posix":
        return {"start_new_session": True}
    # Windows is not a supported zerobox runtime yet. The equivalent forced
    # process-tree primitive there is a Job Object, not CREATE_NEW_PROCESS_GROUP.
    return {}


def kill_process(proc: KillableProcess) -> None:
    with contextlib.suppress(ProcessLookupError):
        proc.kill()


def kill_process_group(proc: KillableProcess) -> None:
    if os.name == "posix":
        with contextlib.suppress(ProcessLookupError):
            os.killpg(proc.pid, signal.SIGKILL)
        return

    # Unsupported-platform fallback: kill the wrapper process only.
    kill_process(proc)
