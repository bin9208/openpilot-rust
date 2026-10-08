import argparse
from contextlib import ExitStack
from email.parser import BytesParser
from email.policy import default
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import urllib.error
import urllib.request
import msgq

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(Path(__file__).parent))
from carrot_man_serv_compare import setup_values
from carrot_man_ingress_compare import envelope
from carrot_man_compare import compare
from carrot_man_fixture_ports import reserve_ports
from carrot_man_fixture_inputs import InputPublisher
from openpilot.cereal import log, messaging
import zmq


class Receiver:
  def __init__(self):
    self.rows = []
    owner = self
    class Handler(BaseHTTPRequestHandler):
      def log_message(self, *args):
        pass
      def do_POST(self):
        data = self.rfile.read(int(self.headers["Content-Length"]))
        content_type = self.headers.get("Content-Type", "")
        row = dict(path=self.path, authorization=self.headers.get("Authorization"), fields={}, files={})
        if content_type.startswith("multipart/"):
          parsed = BytesParser(policy=default).parsebytes(f"Content-Type: {content_type}\r\n\r\n".encode() + data)
          for part in parsed.iter_parts():
            name = part.get_param("name", header="content-disposition")
            body = part.get_payload(decode=True)
            filename = part.get_filename()
            if filename:
              row["files"][name] = dict(name=re.sub(r"\d{8}-\d{6}", "STAMP", filename), type=part.get_content_type(),
                data=json.loads(body) if filename.endswith(".json") else body.decode())
            else:
              value = body.decode()
              if name == "payload_json":
                value = json.loads(value)
                value["content"] = re.sub(r"- Time: [^\n]*", "- Time: STAMP", value["content"])
              row["fields"][name] = value
        else:
          row["fields"] = json.loads(data)
          if self.path == "/discord":
            row["fields"]["content"] = re.sub(r"- Time: [^\n]*", "- Time: STAMP", row["fields"]["content"])
        owner.rows.append(row)
        response = json.dumps(dict(ok=True, token="owned-session")).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(response)))
        self.end_headers()
        self.wfile.write(response)
    self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    self.url = f"http://127.0.0.1:{self.server.server_port}"
    threading.Thread(target=self.server.serve_forever, daemon=True).start()
  def close(self):
    self.server.shutdown()
    self.server.server_close()


class Peer:
  def __init__(self, implementation, args):
    self.root = args.evidence / implementation
    self.root.mkdir(parents=True, exist_ok=False)
    self.stack = ExitStack()
    prefix = Path(self.stack.enter_context(tempfile.TemporaryDirectory(prefix="msgq_carrot219_", dir="/dev/shm"))).name.removeprefix("msgq_")
    os.environ["OPENPILOT_PREFIX"] = prefix
    self.prefix = prefix
    self.params = self.root / "params" / prefix
    self.params.mkdir(parents=True)
    self.memory = self.root / "params/memory" / prefix
    self.publisher = messaging.PubMaster(["deviceState", "carState", "selfdriveState", "carControl", "gpsLocationExternal", "modelV2", "navRouteNavd", "carrotNavi"])
    self.subs = {name: messaging.sub_sock(name, conflate=False, timeout=20) for name in ("carrotMan", "navRoute", "navInstructionCarrot")}
    self.receiver = Receiver()
    reservations = self.stack.enter_context(ExitStack())
    self.ports = reservations.enter_context(reserve_ports())
    self.network = "none"
    self.can_error = False
    self.rows = []
    self.selected = []
    self.http_rows = []
    self.connections = []
    fixture_values = setup_values() | dict(IsOnroad="0", UbloxAvailable="1", Version="fixture", CarName="OWNED", GitBranch="fixture", GitRemote="https://github.com/owned/openpilot.git", GitCommit="a" * 40,
      GitCommitDate="2000-01-01", GithubUsername="owned", DongleId="owned", HardwareSerial="owned")
    for key, value in fixture_values.items():
      self.put(key, value)
    (self.root / "web_settings.json").write_text(json.dumps(dict(web_upload_url=self.receiver.url)))
    data = self.root / "data"
    (data / "media").mkdir(parents=True)
    (data / "params/d").mkdir(parents=True)
    (data / "params/d/small").write_text("owned")
    bindir = self.root / "bin"
    bindir.mkdir()
    tmux = bindir / "tmux"
    tmux.write_text("#!/bin/sh\nprintf 'owned tmux fixture\\n'\n")
    tmux.chmod(0o700)
    env = dict(os.environ, PARAMS_ROOT=str(self.params.parent), PATH=f"{bindir.resolve()}:{os.environ['PATH']}",
      CARROT_AUTO_ONROAD_DIAGNOSTICS="1", CARROT_AUTO_ONROAD_TMUX_DELAY_SECONDS="0.3", CARROT_CAN_ERROR_TMUX_DELAY_SECONDS="0.15",
      CARROT_WEB_UPLOAD_URL=self.receiver.url, CARROT_TMUX_WEB_UPLOAD_URL=self.receiver.url + "/carrot-logs",
      CARROT_EXCEPTION_DISCORD_WEBHOOK_DISABLE="0", CARROT_EXCEPTION_DISCORD_WEBHOOK_URL=self.receiver.url + "/discord")
    command = [str(args.binary.resolve())] if implementation == "native" else [sys.executable, str(ROOT / "rust/tools/carrot_man_runtime_source.py"), "--binding", str(args.params_binding.resolve())]
    self.out = (self.root / "stdout.log").open("w")
    self.err = (self.root / "stderr.log").open("w")
    assert len(self.ports) == len(set(self.ports)) == 7
    fixture = dict(root=str(self.root.resolve()), ports=self.ports, values=fixture_values)
    payload = json.dumps(fixture)
    roles = ("navigation_udp", "navigation_tcp", "navigation_http", "route_tcp", "kisa_udp", "command_zmq", "broadcast_udp")
    (self.root / "fixture-configuration.json").write_text(json.dumps(dict(fixture, port_roles=dict(zip(roles, self.ports))), indent=2) + "\n")
    reservations.close()
    self.process = subprocess.Popen(command, env=env, stdin=subprocess.PIPE, stdout=self.out, stderr=self.err, text=True)
    self.process.stdin.write(payload)
    self.process.stdin.close()
    self.zmq = zmq.Context()
    self.command = self.zmq.socket(zmq.REQ)
    self.command.setsockopt(zmq.RCVTIMEO, 8000)
    self.command.setsockopt(zmq.LINGER, 0)
    self.command.connect(f"tcp://127.0.0.1:{self.ports[5]}")
    self.inputs = InputPublisher(self.publisher, self, self.root)
  def put(self, key, value):
    temporary = self.params / (key + ".new")
    temporary.write_text(value)
    temporary.replace(self.params / key)
  def pump(self):
    assert self.process.poll() is None, ("daemon exited", self.process.returncode, self.root)
    self.inputs.check()
    for name, stream in self.subs.items():
      while (raw := stream.receive(non_blocking=True)) is not None:
        with (self.root / (name + ".bin")).open("ab") as output:
          output.write(raw)
        with log.Event.from_bytes(raw) as event:
          self.rows.append(dict(service=name, valid=event.valid, data=getattr(event, name).to_dict()))
    time.sleep(.01)
  def wait(self, predicate, timeout=8):
    start = len(self.rows)
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      self.pump()
      for row in self.rows[start:]:
        if predicate(row):
          self.selected.append(row)
          return row
    raise AssertionError(("deadline", self.root, self.rows[-3:]))
  def until(self, predicate, timeout=8):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      self.pump()
      if predicate():
        return
    raise AssertionError(("side effect deadline", self.root))
  def http(self, path, value=None):
    body = json.dumps(value).encode() if value is not None else None
    request = urllib.request.Request(f"http://127.0.0.1:{self.ports[2]}{path}", data=body, headers={"Content-Type": "application/json"})
    try:
      response = urllib.request.urlopen(request, timeout=3)
    except urllib.error.HTTPError as error:
      response = error
    row = dict(status=response.status, body=json.load(response))
    if row["body"].get("lastEvent"):
      row["body"]["lastEvent"].pop("receivedAt", None)
    self.http_rows.append(row)
    return row
  def tcp(self, address):
    stream = socket.socket()
    stream.bind((address, 0))
    stream.settimeout(2)
    stream.connect(("127.0.0.1", self.ports[1]))
    self.connections.append(stream)
    return stream
  def route(self, payload):
    with socket.create_connection(("127.0.0.1", self.ports[3]), timeout=2) as stream:
      stream.sendall(struct.pack("!I", len(payload)) + payload)
  def close(self):
    try:
      self.inputs.close()
    finally:
      for stream in self.connections:
        stream.close()
      if self.process.poll() is None:
        self.process.send_signal(signal.SIGTERM)
        try:
          self.process.wait(timeout=3)
        except subprocess.TimeoutExpired:
          self.process.kill()
          self.process.wait()
      self.out.close()
      self.err.close()
      self.command.close()
      self.zmq.term()
      self.receiver.close()
      (self.root / "publications.json").write_text(json.dumps(self.rows, indent=2) + "\n")
      (self.root / "upload-requests.json").write_text(json.dumps(self.receiver.rows, indent=2) + "\n")
      (self.root / "http-responses.json").write_text(json.dumps(self.http_rows, indent=2) + "\n")
      self.subs.clear()
      self.publisher = None
      self.stack.close()


def run(implementation, args):
  peer = Peer(implementation, args)
  result = {}
  try:
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["xPosLat"] > 36.
      and row["data"]["carrotCmdIndex"] == 100 and row["data"]["carrotCmd"] == "DISPLAY" and row["data"]["carrotArg"] == "MAP")
    maps = Path(f"/proc/{peer.process.pid}/maps").read_text()
    identity = dict(executable=os.readlink(f"/proc/{peer.process.pid}/exe"), libpython="libpython" in maps)
    if implementation == "native":
      assert identity["executable"] == str(args.binary.resolve()) and not identity["libpython"]
    (peer.root / "identity.json").write_text(json.dumps(identity, indent=2) + "\n")
    stream = peer.tcp("127.0.0.2")
    naver = envelope()
    stream.sendall(json.dumps(naver).encode() + b"\n")
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["naviOwner"] == "naver_v1" and row["data"]["remote"] == "127.0.0.2")
    legacy = dict(timestamp_ms=20, rgdata=dict(nRoadLimitSpeed=60, nSdiType=1, nSdiSpeedLimit=40, nSdiDist=100),
      sinf=dict(redLightOn=True, redLightRemainTime=15, distance=70), complexCrossroad=dict(show=True, imageBase64="b3duZWQ=", imageMime="image/png", imageWidth=1, imageHeight=1))
    assert peer.http("/api/navi/fixture", {key: value for key, value in legacy.items() if key != "rgdata"})["status"] == 200
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["naviOwner"] == "naver_v1")
    assert not (peer.memory / "CarrotNaviImage").exists()
    terminal = dict(naver, sequence=2, lifecycle="arrived", guidance=dict(current=dict(present=False), next=dict(present=False)),
      safety=dict(present=False), road=dict(limitValid=False, categoryValid=False), route=dict(present=False))
    stream.sendall(json.dumps(terminal).encode() + b"\n")
    assert stream.recv(1) == b""
    legacy["timestamp_ms"] = 21
    assert peer.http("/api/navi/fixture", legacy)["status"] == 200
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["naviOwner"] == "tmap_legacy")
    peer.until(lambda: (peer.memory / "TrafficLight").exists() and (peer.memory / "CarrotNaviImage").exists())
    result["traffic"] = json.loads((peer.memory / "TrafficLight").read_text())
    result["traffic"].pop("ts")
    result["image"] = json.loads((peer.memory / "CarrotNaviImage").read_text())
    result["image"].pop("receivedMono")
    bad = peer.tcp("127.0.0.3")
    bad.sendall(b'{"a":1,"a":2}\n')
    assert bad.recv(1) == b""
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["remote"] == "127.0.0.1")
    result["health"] = peer.http("/health")
    peer.route(struct.pack("!ffff", 127., 37., 127.002, 37.001))
    peer.wait(lambda row: row["service"] == "navRoute" and len(row["data"]["coordinates"]) == 2)
    peer.until(lambda: (peer.params / "NavDestination").exists())
    result["destination"] = json.loads((peer.params / "NavDestination").read_text())
    peer.route(struct.pack("!ff", 127.1, 37.1) + b"xx")
    for _ in range(10):
      peer.pump()
    assert json.loads((peer.params / "NavDestination").read_text()) == result["destination"]
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["naviOwner"] == "", timeout=6)
    with socket.socket(type=socket.SOCK_DGRAM) as udp:
      udp.sendto(b"kisawazeroadspdlimit:50/kisawazeroadname:owned", ("127.0.0.1", peer.ports[4]))
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["nRoadLimitSpeed"] == 50)
    peer.command.send_json(dict(echo_cmd="printf owned; printf err >&2; exit 7"))
    result["echo"] = peer.command.recv_json()
    peer.command.send_json(dict(tmux_send=True))
    result["manual"] = peer.command.recv_json()
    assert result["manual"] == dict(tmux_send=True, result="success", web_ok=True, carrot_logs_ok=True, discord_ok=True)
    uploads = len(peer.receiver.rows)
    peer.put("IsOnroad", "1")
    peer.until(lambda: (peer.root / "data/media/tmux.log").exists())
    for _ in range(45):
      peer.pump()
    assert len(peer.receiver.rows) == uploads
    peer.network = "wifi"
    peer.until(lambda: len(peer.receiver.rows) >= uploads + 3)
    uploads = len(peer.receiver.rows)
    peer.network = "none"
    peer.can_error = True
    peer.until(lambda: (peer.params / "CarrotException").exists() and (peer.params / "CarrotException").read_text() == "can_error")
    peer.put("IsOnroad", "0")
    peer.until(lambda: (peer.params / "CarrotException").read_text() == "")
    assert len(peer.receiver.rows) == uploads
    peer.can_error = False
    peer.put("CarrotException", "exception")
    for _ in range(20):
      peer.pump()
    peer.network = "wifi"
    peer.until(lambda: (peer.params / "CarrotExceptionSent").exists() and (peer.params / "CarrotExceptionSent").read_text() == "1")
    peer.until(lambda: (peer.params / "CarrotException").read_text() == "")
    uploads = len(peer.receiver.rows)
    peer.network = "none"
    for _ in range(20):
      peer.pump()
    fault = peer.tcp("127.0.0.4")
    invalid_text = envelope()
    invalid_text.update(sessionId="315561fa-8e04-44dc-9ed4-e62df1f4b5d6", route=dict(present=False))
    invalid_text["guidance"]["current"]["mainText"] = "\ud800"
    fault.sendall(json.dumps(invalid_text).encode() + b"\n")
    peer.until(lambda: (peer.params / "CarrotException").read_text() == "tmux_send")
    for _ in range(20):
      peer.pump()
    car_count = sum(row["service"] == "carrotMan" for row in peer.rows)
    deadline = time.monotonic() + 2.8
    while time.monotonic() < deadline:
      peer.pump()
    assert sum(row["service"] == "carrotMan" for row in peer.rows) == car_count
    cleared = dict(terminal, sessionId=invalid_text["sessionId"], sequence=2)
    fault.sendall(json.dumps(cleared).encode() + b"\n")
    assert fault.recv(1) == b""
    for _ in range(20):
      peer.pump()
    assert sum(row["service"] == "carrotMan" for row in peer.rows) == car_count
    recovery = peer.tcp("127.0.0.5")
    recovered = envelope()
    recovered.update(sessionId="112a9765-1fae-43d8-9bc1-b13846546525", route=dict(present=False))
    recovered["guidance"]["current"]["mainText"] = "recovered"
    recovery.sendall(json.dumps(recovered).encode() + b"\n")
    peer.wait(lambda row: row["service"] == "carrotMan" and row["data"]["naviOwner"] == "naver_v1" and row["data"]["naviSequence"] == 1 and row["data"]["szTBTMainText"] == "recovered")
    result["late_utf8"] = dict(exception="tmux_send", persisted_through_expiry=True, persisted_through_clear=True, recovered=True)
    assert len(peer.receiver.rows) == uploads
    result.update(selected=peer.selected, uploads=peer.receiver.rows, backup=json.loads((peer.root / "data/backup_params.json").read_text()), exception_sent=True)
    (peer.root / "selected.json").write_text(json.dumps(result, indent=2) + "\n")
    return result
  finally:
    peer.close()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", required=True, type=Path)
  parser.add_argument("--params-binding", required=True, type=Path)
  parser.add_argument("--evidence", required=True, type=Path)
  parser.add_argument("--implementation", choices=("both", "source", "native"), default="both")
  args = parser.parse_args()
  args.evidence.mkdir(parents=True, exist_ok=True)
  results = {implementation: run(implementation, args) for implementation in (("source", "native") if args.implementation == "both" else (args.implementation,))}
  if args.implementation == "both":
    normalized=json.loads(json.dumps(results))
    for result in normalized.values():
      for row in result["selected"]:
        if row["service"]=="carrotMan":
          row["data"].pop("naviOwnerAgeMs",None)
          row["data"].pop("naviSafetyAgeMs",None)
    compare(normalized["native"], normalized["source"], "owned source/native boundary")
  (args.evidence / "owned-summary.json").write_text(json.dumps(dict(passed=True, compared=args.implementation == "both",
    scenarios=["real-params-ipc", "tcp-nav-session-terminal", "http-aux-owner", "duplicate-client-close", "route-framing-partial-rejection", "kisa-udp", "zmq-echo", "manual-two-target-discord", "onroad-network-wait", "can-offroad-cancel", "exception-clear-and-sent", "late-utf8-expiry-clear-recovery"],
    files={str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in (Path(__file__), ROOT / "rust/tools/carrot_man_runtime_source.py", ROOT / "rust/tools/carrot_man_fixture_inputs.py")}), indent=2) + "\n")
  print(f"PASS owned CarrotMan {args.implementation} boundaries")


if __name__ == "__main__":
  main()
