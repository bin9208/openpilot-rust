import argparse
import copy
import dataclasses
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(Path(__file__).parent))
from carrot_man_compare import compare
from openpilot.selfdrive.carrot.navigation_ingress import strict_json_loads, _legacy_json_loads, NavigationIngressError
from openpilot.selfdrive.carrot.naver_navigation_protocol import parse_naver_navigation_v1, NaverProtocolError


def envelope():
  return dict(schema="naver.navigation.v1", sessionId="44f2db8e-1edc-4f2f-8bc4-d1033d6106c7", sequence=1,
    sentMonotonicMs=100000, lifecycle="guiding",
    guidance=dict(current=dict(present=True, maneuver="left", distanceM=100., roadName="도로", mainText="좌회전"), next=dict(present=False)),
    safety=dict(present=True, kind="fixed_camera", speedKph=50., distanceM=150., revision=1),
    road=dict(limitValid=True, limitKph=60., categoryValid=True, category=2),
    route=dict(present=True, remainingDistanceM=1000., remainingTimeSec=120., offRoute=False,
      destinationValid=True, destinationLatitude=37.1, destinationLongitude=127.1, points=[[37., 127.], [37.1, 127.1]], revision=2))


def cases():
  requests = []
  for label, frame in [
    ("legacy_object", b'{"rgdata":{"nRoadLimitSpeed":60},"timestamp_ms":10}'),
    ("escaped_duplicate", b'{"a":1,"\\u0061":2}'),
    ("nested_duplicate", b'{"rgdata":{"x":1,"x":2}}'),
    ("nonfinite_constant", b'{"x":NaN}'),
    ("numeric_overflow_is_float", b'{"x":1e400}'),
    ("nonobject", b'[]'),
    ("bom", b'\xef\xbb\xbf{}'),
    ("invalid_utf8", b'{"x":"\xff"}'),
    ("empty", b' \r\n'),
    ("trailing", b'{} {}'),
    ("size_limit", b' ' * 262145),
    ("nesting_limit", b'{"x":' + b'[' * 128 + b'0' + b']' * 128 + b'}'),
  ]:
    requests.append(dict(label=label, op="ingress", bytes=list(frame), now=100., strict=True))
  requests.append(dict(label="legacy_nonfinite", op="ingress", bytes=list(b'{"x":NaN}'), now=100., strict=False))
  for label, change in [
    ("naver_guiding", lambda value: None),
    ("naver_bump", lambda value: value.update(safety=dict(present=True, kind="speed_bump", distanceM=30.))),
    ("naver_section", lambda value: value.update(safety=dict(present=True, kind="section_camera", distanceM=900., speedKph=60., revision=4))),
    ("naver_bad_sequence", lambda value: value.update(sequence=True)),
    ("naver_terminal_controls", lambda value: value.update(lifecycle="arrived")),
    ("naver_bad_coordinate", lambda value: value["route"].update(points=[[91., 127.]])),
  ]:
    value = copy.deepcopy(envelope())
    change(value)
    requests.append(dict(label=label, op="ingress", bytes=list(json.dumps(value, ensure_ascii=False).encode()), now=100., strict=True))
  return requests


def source(request):
  try:
    value = (strict_json_loads if request["strict"] else _legacy_json_loads)(bytes(request["bytes"]))
    if "schema" in value:
      return dict(snapshot=dataclasses.asdict(parse_naver_navigation_v1(value, request["now"])))
    try:
      json.dumps(value, allow_nan=False)
    except ValueError:
      return dict(value=None)
    return dict(value=value)
  except (NavigationIngressError, NaverProtocolError) as error:
    return dict(error=error.code)


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", required=True)
  parser.add_argument("--evidence", required=True, type=Path)
  args = parser.parse_args()
  requests = cases()
  result = subprocess.run([args.binary], input="".join(json.dumps(r) + "\n" for r in requests), text=True, capture_output=True)
  if result.returncode:
    raise RuntimeError(result.stderr)
  actual = [json.loads(line) for line in result.stdout.splitlines()]
  expected = [json.loads(json.dumps(source(request))) for request in requests]
  assert len(actual) == len(expected)
  args.evidence.mkdir(parents=True, exist_ok=True)
  (args.evidence / "ingress-inputs.jsonl").write_text("".join(json.dumps(r) + "\n" for r in requests))
  (args.evidence / "ingress-source.jsonl").write_text("".join(json.dumps(r) + "\n" for r in expected))
  (args.evidence / "ingress-rust.jsonl").write_text(result.stdout)
  for request, observed, wanted in zip(requests, actual, expected):
    compare(observed, wanted, request["label"])
  (args.evidence / "ingress-summary.json").write_text(json.dumps(dict(passed=True, scenarios=len(requests),
    surface="original ingress/protocol decoder and Rust parser; strict and legacy source paths"), indent=2) + "\n")
  print(f"PASS {len(requests)} original ingress/protocol scenarios")


if __name__ == "__main__":
  main()
