#!/usr/bin/env python3
from __future__ import annotations

import argparse
from pathlib import Path
from card_runtime_source import load_binding


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--binding", type=Path, required=True)
  parser.add_argument("--assets", type=Path, required=True)
  parser.add_argument("--config", type=Path, required=True)
  parser.add_argument("--log-root", type=Path, required=True)
  parser.add_argument("--tcp-port", type=int, required=True)
  parser.add_argument("--http-port", type=int, required=True)
  args = parser.parse_args()
  load_binding(args.binding)
  from openpilot.system.hardware.hw import Paths
  Paths.swaglog_root = staticmethod(lambda: str(args.log_root))
  from openpilot.selfdrive.carrot import xiaoge_data
  from openpilot.selfdrive.carrot.xiaoge import v_asm_server
  v_asm_server.CONFIG_PATH = args.config
  v_asm_server.DEFAULT_LANE_MODEL_PATH = args.assets / "lane.onnx"

  def vision_server(owner: xiaoge_data.XiaogeDataBroadcaster) -> None:
    owner.vision_service, owner.vision_server = v_asm_server.create_server("127.0.0.1", args.http_port, args.assets / "v_asm_model.onnx")
    owner.vision_server.serve_forever()

  xiaoge_data.XiaogeDataBroadcaster.get_ip_address = staticmethod(lambda: "127.0.0.9")
  xiaoge_data.XiaogeDataBroadcaster.start_vision_server = vision_server
  owner = xiaoge_data.XiaogeDataBroadcaster()
  owner.tcp_port = args.tcp_port
  owner.broadcast_data()


if __name__ == "__main__":
  main()
