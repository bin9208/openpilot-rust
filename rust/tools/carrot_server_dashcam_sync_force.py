# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Owned startup Git interposer observes native group cleanup; no external requests.
from __future__ import annotations

from dataclasses import replace
import json
import os
from pathlib import Path
import select
import sys
import time

import anyio
from carrot_server_dashcam_sync_fixtures import Call, Config, Fixture, Kind, children, close, exited
from carrot_server_dashcam_sync_peer import HeldReceiver
from carrot_server_dashcam_sync_probe import files
from carrot_server_dashcam_upload import save


async def startup_force(config: Config, action: str) -> None:
    output = config.output/action; output.mkdir()
    shim = output/'owned-bin'; shim.mkdir()
    capture = output/'git-child.json'
    text = '#!'+sys.executable+'\nimport json,os,threading\nfrom pathlib import Path\n'
    text += f"Path({str(capture)!r}).write_text(json.dumps({{'pid':os.getpid(),'parent':os.getppid(),'group':os.getpgrp()}}))\nthreading.Event().wait()\n"
    (shim/'git').write_text(text); (shim/'git').chmod(0o700)
    root, segment = files(output)
    fixture = Fixture(Kind.NATIVE, replace(config, output=output, git_shim=shim))
    receiver = HeldReceiver(False); stream = None; fds = []; observation = {}
    try:
        await fixture.start(root, receiver)
        stream = await fixture.connect(Call(body={'segment': segment}))
        with anyio.fail_after(3):
            while not capture.exists(): await anyio.sleep(.01)
        git = json.loads(capture.read_text())
        worker = children(fixture.peer.process.pid)
        assert len(worker) == 1 and git['parent'] in worker and git['group'] == git['parent']
        git_identity = {git['pid']: Path(f"/proc/{git['pid']}/stat").read_text().split(') ', 1)[1].split()[19]}
        fds = [os.pidfd_open(pid) for pid in [*worker, git['pid']]]
        before = time.monotonic(); await fixture.force(action)
        with anyio.fail_after(3): await fixture.peer.process.wait()
        with anyio.fail_after(3):
            while not exited({**worker, **git_identity}): await anyio.sleep(.01)
        observation = {'action': action, 'worker': worker, 'git': git, 'git_identity': git_identity,
                       'server_exit': fixture.peer.process.returncode, 'seconds': time.monotonic()-before,
                       'worker_reaped': exited(worker), 'git_reaped': exited(git_identity),
                       'pidfds_readable': [bool(select.select([fd], [], [], 0)[0]) for fd in fds],
                       'recipient_requests': len(receiver.captures)}
        assert observation['server_exit'] == 0 and observation['worker_reaped'] and observation['git_reaped']
        assert all(observation['pidfds_readable']) and not receiver.captures
    finally:
        for fd in fds: os.close(fd)
        if stream: await stream.aclose()
        save(output/'result.json', observation)
        await close([fixture], [receiver])
