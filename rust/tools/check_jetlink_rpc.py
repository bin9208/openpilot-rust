# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy"]
# ///
# Run: PYTHONPATH=. uv run rust/tools/check_jetlink_rpc.py <rpc_probe> <evidence>
"""Bidirectional compatibility with the actual original RPC client and owner."""
import json
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import numpy as np

from openpilot.selfdrive.modeld.jetlink.client import CONTRACT, JetlinkClient
from openpilot.selfdrive.modeld.jetlink.daemon import InferenceServer
from openpilot.selfdrive.modeld.jetlink.rpc import ProxyClient
from third_party.jetlink.spec import ModelSpec  # noqa: TID251 - pinned Jetlink is vendored at the repository root.


class HardwarePeer:
    """Hardware-only fixture; original contract, RPC, validation and deadlines run unchanged."""
    seq = 0
    def hello(self):
        return {"protocol": 2, "backend": "ort", "runtime_version": "1.22.0",
                "telemetry": {"artifact_sha256": CONTRACT["sha256"], "source_sha256": CONTRACT["sha256"]}}
    def ensure_engine(self, *_args, **_kwargs):
        return ModelSpec.from_dict(CONTRACT)
    def infer(self, warped, packed, frame_id, reset, deadline):
        assert len(warped) == 393216 and warped == bytes([13]) * 393216
        np.testing.assert_array_equal(packed, np.full(12, .125, np.float32))
        assert frame_id == 42 and reset and 0 < deadline <= .050
        return np.full(18452, .375, np.float32)
    def close(self):
        return None


def main() -> None:
    binary, evidence = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="jetlink-rpc-") as directory:
        path = str(Path(directory) / "owner.sock")
        native = subprocess.Popen([binary, "server", path], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        assert native.stdout.readline().strip() == "READY"
        client = ProxyClient(path)
        try:
            client.connect()
            output = client.infer(42, bytes(393216), np.zeros(12, np.float32), time.monotonic_ns() + 50_000_000, True)
            np.testing.assert_array_equal(output, np.full(18452, .25, np.float32))
        finally:
            client.close()
            stdout, stderr = native.communicate("x", timeout=2)
        assert native.returncode == 0 and "STOPPED" in stdout
        (evidence / "rust-owner-python-proxy.log").write_text(stdout + stderr)
        original = InferenceServer(JetlinkClient(HardwarePeer()), path)
        original.worker.start()
        deadline = time.monotonic() + 2
        while not original.ready:
            assert time.monotonic() < deadline, original.error
            time.sleep(.001)
        try:
            result = subprocess.run([binary, "client", path], text=True, capture_output=True, timeout=2, check=True)
            (evidence / "python-owner-rust-proxy.log").write_text(result.stdout + result.stderr)
            assert json.loads(result.stdout) == {"count": 18452, "first": .375, "last": .375, "dead": False}
        finally:
            original.stop()
            original.worker.join(timeout=2)
        assert not original.worker.is_alive()
    report = {"result": "PASS", "scenarios": ["original ProxyClient to native Server", "native ProxyClient to original InferenceServer/JetlinkClient"],
              "comparison": "exact identity/dimensions/float32", "hardware_seam": "synthetic inference only", "device_access": False}
    (evidence / "rpc-summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
