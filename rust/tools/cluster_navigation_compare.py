from __future__ import annotations

import copy
from dataclasses import asdict
import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys


def run(root: Path, binary: Path, output: Path) -> None:
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  from check_message_state import compare, source
  from cluster_navi import fresh_carrot_navi, parse_carrot_navi, resolve_navi_speed_limit

  messaging, _ = source()
  output.mkdir(parents=True, exist_ok=True)
  steps, expected = [], []
  state = None

  def step(now, fields=None, base=30, base_source="c"):
    nonlocal state
    path = None
    if fields is not None:
      packet = messaging["new_message"]("carrotNavi")
      packet.carrotNavi.from_dict(fields)
      path = output / f"{len(steps):04}.capnp"
      path.write_bytes(packet.to_bytes())
      state = parse_carrot_navi(packet.carrotNavi, now, state)
    state, deadline = fresh_carrot_navi(state, now)
    limit = resolve_navi_speed_limit(base, base_source, state)
    expected.append({"state": asdict(state) if state else None, "deadline": deadline if math.isfinite(deadline) else None, "limit": limit})
    steps.append({"message": str(path) if path else None, "now": now, "base": base, "source": base_source})

  def meta(now, sequence=1):
    return {"present": True, "sequence": sequence, "sourceTimestampMillis": 2**64 - 1, "receivedMonoTimeNanos": int(now * 1e9)}

  def fields(now, sequence=1):
    m = meta(now, sequence)
    return {
      "schemaVersion": 1,
      "connected": True,
      "generation": 2**64 - 1,
      "sessionId": "first",
      "vehicle": {"meta": m, "latitude": 37.5, "longitude": 127.1, "headingDeg": 271.5, "speedKph": 91.25, "roadName": "서울", "virtualGps": True},
      "guidanceCurrent": {
        "meta": m,
        "distanceM": 312,
        "timeSec": 20,
        "turnType": 4,
        "roadName": "road",
        "mainText": "turn",
        "nearDirection": "near",
        "midDirection": "mid",
        "farDirection": "far",
      },
      "guidanceNext": {"meta": m, "distanceM": 913, "turnType": -1},
      "laneCurrent": {
        "meta": m,
        "count": 4,
        "distanceM": 199,
        "visible": True,
        "lanePlay": True,
        "currentLane": 2,
        "turnCode": 5,
        "turnInfo": list(range(22)),
        "available": [1, 0, 1],
      },
      "laneAhead": [{"meta": m, "count": 3}, {"meta": {"present": False}}, {"meta": m, "count": 5}],
      "speed": {
        "meta": m,
        "currentKph": 90.5,
        "roadLimitValid": True,
        "roadLimitKph": 80,
        "sdiPresent": True,
        "sdiType": 2,
        "sdiDistanceM": 122,
        "sdiSpeedLimitKph": 60,
        "secondarySdiPresent": True,
        "secondarySdiType": 7,
        "secondarySdiDistanceM": 322,
        "secondarySdiSpeedLimitKph": 50,
        "sectionPresent": True,
        "sectionActive": True,
        "sectionAverageKph": 69.5,
        "sectionRemainingDistanceM": 601.5,
        "sectionRemainingTimeSec": 70,
        "sectionProgress": 0.4,
        "sectionSpeedLimitKph": 70,
      },
      "trafficSignal": {
        "meta": m,
        "visible": True,
        "distanceM": 99,
        "redValid": True,
        "redOn": True,
        "redRemainSec": 12,
        "greenValid": True,
        "greenRemainSec": 31,
        "leftValid": True,
        "leftOn": True,
        "leftRemainSec": 10,
        "rightValid": True,
        "rightRemainSec": 15,
        "uturnValid": True,
        "uturnRemainSec": 21,
      },
      "crossroad": {"meta": m, "visible": True, "distanceM": 321, "imageCode": 73},
      "route": {
        "meta": m,
        "remainingDistanceM": 12345,
        "remainingTimeSec": 600,
        "movedDistanceM": 1000,
        "movedTimeSec": 80,
        "totalDistanceM": 13345,
        "polyline": [{"latitude": 37 + i / 1000, "longitude": 127 + i / 1000} for i in range(280)],
      },
      "navigationStatus": {"meta": m, "mode": "guidance", "guidanceActive": True, "offRoute": True, "routePresent": True},
    }

  step(100.0, {})
  step(100.0, {"schemaVersion": 2, "connected": True})
  initial = fields(100.0, 2**64 - 1)
  step(100.0, initial)
  repeated = copy.deepcopy(initial)
  repeated["speed"]["roadLimitKph"] = 100
  repeated["laneAhead"][2]["count"] = 9
  step(101.0, repeated)
  repeated["sessionId"] = "second"
  step(102.0, repeated, base_source="v")
  for now in [105.999, 106.0, 109.999, 110.0, 129.999, 130.0, 159.999, 160.0]:
    step(now)
  for index, limit in enumerate([-1, 0, 1, 9, 10, 30, 85, 190, 200, 210, 32767]):
    value = fields(200.0 + index)
    value["speed"]["roadLimitKph"] = limit
    step(200.0 + index, value)
  for offset, nanos in enumerate([0, 360_000_000_000, 360_000_000_001, 2**64 - 1]):
    value = fields(300.0)
    value["speed"]["meta"] = {**meta(300.0, offset + 1), "receivedMonoTimeNanos": nanos}
    step(300.0, value)
  value = fields(400.0)
  value["vehicle"].update(latitude=math.nan, longitude=math.inf, speedKph=-20.0, headingDeg=-math.inf)
  value["guidanceCurrent"].update(distanceM=-10, timeSec=-10)
  value["laneCurrent"].update(count=30, currentLane=-3, distanceM=-2)
  value["speed"].update(
    currentKph=-1.0,
    sdiDistanceM=-1,
    sdiSpeedLimitKph=-1,
    secondarySdiDistanceM=-1,
    secondarySdiSpeedLimitKph=-1,
    sectionProgress=math.nan,
    sectionRemainingDistanceM=math.inf,
    sectionAverageKph=-1.0,
    sectionRemainingTimeSec=-4,
    sectionSpeedLimitKph=-2,
  )
  value["trafficSignal"].update(distanceM=-1, redRemainSec=-3, rightValid=False, uturnValid=False)
  value["route"]["polyline"][0]["latitude"] = math.nan
  value["route"]["polyline"][1]["longitude"] = math.inf
  value["crossroad"].update(distanceM=-1, imageCode=-1)
  step(400.0, value)
  value = fields(401.0)
  value["trafficSignal"]["visible"] = False
  value["speed"].update(sdiPresent=False, secondarySdiPresent=False, sectionPresent=False, roadLimitValid=False)
  step(401.0, value)
  for progress in [-0.1, 0.0, 0.5, 1.0, 1.1, math.inf]:
    value = fields(500.0, len(steps))
    value["speed"]["sectionProgress"] = progress
    step(500.0, value)
  step(501.0, {"schemaVersion": 1, "connected": False})
  request = output / "input.json"
  request.write_text(json.dumps(steps))
  native = json.loads(subprocess.check_output([str(binary), str(request)], text=True))
  expected = json.loads(json.dumps(expected))
  (output / "expected.json").write_text(json.dumps(expected, indent=2))
  (output / "native.json").write_text(json.dumps(native, indent=2))
  failures = []
  for index, (actual, reference) in enumerate(zip(native, expected, strict=True)):
    try:
      compare(reference, actual, str(index))
    except AssertionError as error:
      failures.append(str(error))
  receipt = {
    "cases": len(expected),
    "failures": failures,
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "source_sha256": hashlib.sha256((root / "openpilot/selfdrive/carrot/cluster/cluster_navi.py").read_bytes()).hexdigest(),
  }
  (output / "result.json").write_text(json.dumps(receipt, indent=2))
  print(json.dumps(receipt))
  if failures:
    raise SystemExit(1)


if __name__ == "__main__":
  run(*(Path(arg).resolve() for arg in sys.argv[1:]))
