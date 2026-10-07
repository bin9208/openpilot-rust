from __future__ import annotations

import json
import sys

from opendbc.car.carlog import carlog
from openpilot.common.swaglog import cloudlog, ForwardingHandler


def main() -> None:
  carlog.addHandler(ForwardingHandler(cloudlog))
  inputs: object = json.load(sys.stdin)
  if not isinstance(inputs, list):
    raise ValueError("expected a list of source log inputs")
  for case in inputs:
    match case:
      case {"op": "text", "level": "warning", "message": str() as message}:
        carlog.warning(message)
      case {"op": "text", "level": "error", "message": str() as message}:
        carlog.error(message)
      case {"op": "malformed_vin", "vin": str() as vin}:
        carlog.error({"event": "Malformed VIN", "vin": vin})
      case {"op": "unmatched", "fingerprints": str() as fingerprints}:
        carlog.error({"event": "car doesn't match any fingerprints", "fingerprints": fingerprints})
      case {"op": "fingerprinted", "time": float() as time}:
        carlog.error({"event": "fingerprinted", "car_fingerprint": "MOCK", "source": 2, "fuzzy": False, "cached": True,
                      "fw_count": 1, "ecu_responses": [(123, None, 0), (456, 8, 2)], "vin_rx_addr": -1, "vin_rx_bus": -1,
                      "fingerprints": "{0: {123: 8}}", "fw_query_time": time})
      case _:
        raise ValueError("unknown source log input")


if __name__ == "__main__":
  main()
