# /// script
# requires-python = ">=3.12"
# dependencies = ["pygame==2.6.1", "pillow"]
# ///
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def source(output: Path, root: Path) -> None:
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import pygame
  from cluster_replay_tools_source import render_cases
  cases = json.loads((root / ".omo/evidence/253-cluster-replay-tools/blocked-font/cases.json").read_text())
  render_cases(cases, output)
  assert pygame.font.match_font("consolas").endswith(".ttc")
  (output / "catalog.json").write_text(json.dumps({"consolas": pygame.font.match_font("consolas"), "segoeui": pygame.font.match_font("segoeui")}))


def main() -> None:
  root = Path(__file__).resolve().parents[2]
  if sys.argv[1] == "--source":
    source(Path(sys.argv[2]), root)
    return
  native, output = (Path(value).resolve() for value in sys.argv[1:])
  output.mkdir(parents=True, exist_ok=True)
  font = Path("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc")
  assert font.is_file()
  provider = output / "fc-list"
  provider.write_text(f"#!/bin/sh\nprintf '%s\\n' '{font}: Consolas: Regular' '{font}: SegoeUI: Regular'\n")
  provider.chmod(0o755)
  environment = dict(os.environ, PATH=f"{output}:{os.environ.get('PATH', '')}")
  commands = [[sys.executable, str(Path(__file__).resolve()), "--source", str(output / "source")], [str(native), "render", str(root / ".omo/evidence/253-cluster-replay-tools/blocked-font/cases.json"), str(output / "native"), str(root / ".omo/evidence/253-cluster-replay-tools/gui-config.json")]]
  for name, command in zip(("source", "native"), commands, strict=True):
    with (output / f"{name}.log").open("wb") as log:
      subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=20, check=True)
  frames = []
  for path in (output / "source").glob("*.rgba"):
    actual = output / "native" / path.name
    expected = path.read_bytes()
    observed = actual.read_bytes()
    frames.append(dict(name=path.name, exact=expected == observed, different_channels=sum(a != b for a, b in zip(expected, observed, strict=True))))
  assert frames
  report = dict(commands=commands, provider=str(provider), font=str(font), font_sha256=hashlib.sha256(font.read_bytes()).hexdigest(), native_sha256=hashlib.sha256(native.read_bytes()).hexdigest(), frames=frames)
  (output / "result.json").write_text(json.dumps(report, indent=2))
  print(json.dumps(report))
  if any(not row["exact"] for row in frames):
    raise SystemExit(1)


if __name__ == "__main__":
  main()
