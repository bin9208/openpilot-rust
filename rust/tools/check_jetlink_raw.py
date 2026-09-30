# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run: PYTHONPATH=. uv run rust/tools/check_jetlink_raw.py <jetlink_raw_probe> <evidence>
"""Preserve the original active-Jetlink raw output failure during the parity port."""
import json
import subprocess
import sys
import time
import traceback
from pathlib import Path

import numpy as np
from openpilot.cereal import log
from openpilot.selfdrive.modeld import fill_model_msg
from openpilot.selfdrive.modeld.jetlink.client import CONTRACT
from openpilot.selfdrive.modeld.jetlink.model_state import JetlinkModelState
from third_party.jetlink.spec import ModelSpec  # noqa: TID251 - pinned Jetlink is vendored at the repository root.


class HardwarePeer:
    """Only the remote inference array is synthetic; original adapter/parser/publication execute."""
    def infer(self, *_args):
        return np.zeros(18452, np.float32)


def main() -> None:
    binary, evidence = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    model = JetlinkModelState(1928, 1208, HardwarePeer(), ModelSpec.from_dict(CONTRACT), None)
    parsed = model.infer_prepared(42, (bytes(393216), np.zeros(12, np.float32)), time.monotonic_ns() + 50_000_000)
    event = log.Event.new_message()
    event.init("modelV2")
    action = log.ModelDataV2.Action.new_message()
    fill_model_msg.SEND_RAW_PRED = True
    try:
        fill_model_msg.fill_model_msg(event, parsed, action, fill_model_msg.PublishState(), 42, 42, 42, 0.0, 1000, 0.001, True)
    except KeyError as error:
        assert error.args == ("raw_pred",)
        (evidence / "original-raw-boundary.log").write_text(traceback.format_exc())
    else:
        raise AssertionError("original missing raw_pred boundary disappeared")
    fill_model_msg.SEND_RAW_PRED = False
    fill_model_msg.fill_model_msg(event, parsed, action, fill_model_msg.PublishState(), 42, 42, 42, 0.0, 1000, 0.001, True)
    cases = []
    for mode in ("raw", "normal"):
        result = subprocess.run([binary, mode], text=True, capture_output=True, timeout=2, check=False)
        (evidence / f"rust-{mode}-boundary.log").write_text(result.stdout + result.stderr)
        if mode == "raw":
            assert result.returncode != 0 and "JetlinkRawPredictionsUnavailable" in result.stderr
        else:
            assert result.returncode == 0
            assert json.loads(result.stdout) == {"source": "jetlink", "raw_requested": False, "raw_present": False}
        cases.append({"mode": mode, "invocation": [str(binary), mode], "exit_code": result.returncode})
    report = {"result": "PASS", "original_raw_error": {"type": "KeyError", "key": "raw_pred"}, "original_non_raw": "PASS", "native": cases}
    (evidence / "raw-boundary-summary.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
