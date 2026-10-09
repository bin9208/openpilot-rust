from __future__ import annotations

import json
from pathlib import Path
import sys

from pytest import MonkeyPatch

from original_params_binding import load


def main() -> None:
  request = json.loads(sys.stdin.read())
  root = Path(request["root"])
  load(request["binding"], f"ipc://{root}/owned-log", root / "logs")
  from openpilot.selfdrive.modeld import big_model, helpers, precompiled_model

  with MonkeyPatch.context() as monkey:
    monkey.setattr(big_model, "model_cache_dir", lambda: root / "cache")
    monkey.setattr(precompiled_model, "model_cache_dir", lambda: root / "cache")
    monkey.setattr(helpers, "MODELS_DIR", root / "openpilot/selfdrive/modeld/models")
    monkey.setattr(helpers, "Path", lambda name: root / "devices" if name == "/sys/bus/usb/devices" else Path(name))
    monkey.setattr(sys, "argv", ["big_model", "--ensure-if-egpu", "--manifest-url", request["url"], "--network-wait-seconds", str(request["wait"])])
    raise SystemExit(big_model.main())


if __name__ == "__main__":
  main()
