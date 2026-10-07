from __future__ import annotations

from http.client import HTTPResponse
import json
import socket
import struct
import time

import httpx2


def available_port() -> int:
  with socket.socket() as server:
    server.bind(("127.0.0.1", 0))
    return server.getsockname()[1]


def client(port: int) -> httpx2.Client:
  limits = httpx2.Limits(max_connections=200, max_keepalive_connections=40, keepalive_expiry=30.0)
  transport = httpx2.HTTPTransport(http2=True, retries=3, limits=limits, socket_options=[(socket.IPPROTO_TCP, socket.TCP_NODELAY, 1)])
  return httpx2.Client(transport=transport, timeout=httpx2.Timeout(connect=5.0, read=30.0, write=10.0, pool=10.0),
                      follow_redirects=True, base_url=f"http://127.0.0.1:{port}", trust_env=False)


def read_exact(stream: socket.socket, length: int) -> bytes:
  output = bytearray()
  while len(output) < length:
    data = stream.recv(length - len(output))
    if not data:
      raise EOFError("Xiaoge TCP closed before full packet")
    output.extend(data)
  return bytes(output)


def raw_http(port: int, request: bytes) -> httpx2.Response:
  with socket.create_connection(("127.0.0.1", port), timeout=5) as stream:
    stream.sendall(request)
    stream.shutdown(socket.SHUT_WR)
    with HTTPResponse(stream) as response:
      response.begin()
      return httpx2.Response(response.status, headers=response.getheaders(), content=response.read())


def telemetry(port: int):
  packets = []
  heartbeat = False
  with socket.create_connection(("127.0.0.1", port), timeout=5) as stream:
    stream.sendall(struct.pack("!I", 2))
    while len(packets) < 8 or not heartbeat:
      length = struct.unpack("!I", read_exact(stream, 4))[0]
      if length == 0:
        heartbeat = True
        continue
      assert 0 < length < 65536
      packet = json.loads(read_exact(stream, length))
      assert packet["version"] == 1 and packet["ip"] == "127.0.0.9"
      assert packet["data"] == {}
      packets.append(packet)
  for first, second in zip(packets[:-1], packets[1:], strict=True):
    assert second["sequence"] == first["sequence"] + 1
    assert second["timestamp"] > first["timestamp"]
  intervals = sorted(second["timestamp"] - first["timestamp"] for first, second in zip(packets[:-1], packets[1:], strict=True))
  assert .04 < intervals[len(intervals) // 2] < .06, intervals
  return {"heartbeat": heartbeat, "packets": packets, "median_seconds": intervals[len(intervals) // 2]}


def wait_ready(connection: httpx2.Client, process, timeout: float = 20.0) -> None:
  deadline = time.monotonic() + timeout
  while time.monotonic() < deadline:
    assert process.poll() is None, process.returncode
    try:
      response = connection.get("/api/status")
    except httpx2.ConnectError:
      time.sleep(.02)
      continue
    response.raise_for_status()
    status = response.json()
    if status["model"]["loaded"] and status["lane"]["loaded"] and "unavailable" in status["camera"]["error"] and "unavailable" in status["lane"]["cameraError"]:
      return
    time.sleep(.02)
  raise TimeoutError("Xiaoge normal startup did not expose ready HTTP state")
