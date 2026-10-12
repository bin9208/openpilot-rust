from __future__ import annotations

import argparse
import asyncio
import base64
from dataclasses import asdict
import hashlib
import json
from pathlib import Path
import socket
import struct
import sys
import time

import aiohttp

from cluster_navi_simulator_compare import normalize


async def read_json(child):
  deadline = time.monotonic() + 8
  while time.monotonic() < deadline:
    line = await asyncio.wait_for(child.stdout.readline(), max(0, deadline - time.monotonic()))
    if not line:
      raise RuntimeError(f"fixture exited {child.returncode}")
    try:
      return json.loads(line)
    except json.JSONDecodeError:
      continue
  raise TimeoutError("fixture stdout")


async def command(child, word):
  child.stdin.write((word + "\n").encode())
  await child.stdin.drain()
  return await read_json(child)


def view(result):
  state = result["state"]
  dashboard = state["navi_dashboard"]
  return {
    "onroad": state["onroad"],
    "speed": state["speed_kph"],
    "limit": state["speed_limit_kph"],
    "limit_source": state["speed_limit_source"],
    "connected": dashboard["connected"],
    "received": dashboard["received_count"],
    "app": dashboard["app_version"],
    "items": dashboard["items"],
    "media": dashboard["media"],
    "app_status": dashboard["app_status"],
    "camera_status": dashboard["camera_status"],
    "composition_status": dashboard["composition_status"],
  }


async def exercise(command_line, catalog, output):
  with socket.socket() as reserved:
    reserved.bind(("127.0.0.1", 0))
    port = reserved.getsockname()[1]
  with (output / "stderr.log").open("w") as errors:
    child = await asyncio.create_subprocess_exec(*command_line, str(port), stdin=asyncio.subprocess.PIPE, stdout=asyncio.subprocess.PIPE, stderr=errors)
    raw = []
    try:
      assert await read_json(child) == {"ready": True, "port": port}
      raw.append(await command(child, "update"))
      endpoint = f"http://127.0.0.1:{port}"
      async with aiohttp.ClientSession(timeout=aiohttp.ClientTimeout(total=5)) as client:
        async with client.get(endpoint + "/health") as response:
          assert response.status == 200
          assert (await response.json())["map_theme"] == "dark"
        control = await client.ws_connect(endpoint + "/api/navi/ws/v2/control/owned-app")
        await control.send_json(
          {
            "type": "requirements_query",
            "protocol_version": 2,
            "catalog_revision": 1,
            "streams": [{"kind": kind, "name": name, "schema_version": 1} for kind, name in catalog],
          }
        )
        manifest = await control.receive_json()
        assert manifest["type"] == "subscription_manifest"
        session = manifest["session_id"]
        streams = []
        for name, value in [("vehicle", {"speed_kph": 45.5, "road_name": "owned-road"}), ("speed", {"current_kph": 46, "road_limit_kph": 60})]:
          stream = await client.ws_connect(endpoint + f"/api/navi/ws/v2/json/{session}/{name}")
          streams.append(stream)
          await stream.send_json(
            {
              "type": "item_update",
              "protocol_version": 2,
              "session_id": session,
              "kind": "json",
              "name": name,
              "sequence": 1,
              "source_timestamp_ms": 100,
              "sent_at_ms": 100,
              "present": True,
              "value": value,
              "manifest_revision": 1,
              "schema_version": 1,
              "stream_handle": catalog.index(("json", name)) + 1,
            }
          )
        for _ in range(100):
          async with client.get(endpoint + "/health") as response:
            health = await response.json()
          if health["received_count"] == 2:
            break
          await asyncio.sleep(0.01)
        assert health["received_count"] == 2
        raw.append(await command(child, "update"))
        assert raw[-1]["state"]["speed_kph"] == 45.5
        assert raw[-1]["state"]["speed_limit_kph"] == 60
        image = await client.ws_connect(endpoint + f"/api/navi/ws/v2/image/{session}/tbt_current_full")
        streams.append(image)
        payload = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jR3sAAAAASUVORK5CYII=")
        handle = catalog.index(("image", "tbt_current_full")) + 1
        await image.send_bytes(struct.pack("!4sBBBBIIQQIHH", b"CNV2", 2, 1, 1, 0, handle, 1, 1, 100, len(payload), 1, 1) + payload)
        for _ in range(100):
          image_state = await command(child, "update")
          if image_state["state"]["navi_dashboard"]["media"]:
            break
          await asyncio.sleep(0.01)
        assert image_state["state"]["navi_dashboard"]["media"][0]["data"] == list(payload)
        raw.append(image_state)
        await image.send_bytes(struct.pack("!4sBBBBIIQQIHH", b"CNV2", 2, 4, 1, 0, handle, 1, 2, 100, 0, 0, 0))
        for _ in range(100):
          clear_state = await command(child, "update")
          if not clear_state["state"]["navi_dashboard"]["media"][0]["present"]:
            break
          await asyncio.sleep(0.01)
        assert not clear_state["state"]["navi_dashboard"]["media"][0]["present"]
        raw.append(clear_state)
        for stream in streams:
          await stream.close()
        await control.close()
        for _ in range(100):
          raw_state = await command(child, "update")
          if not raw_state["state"]["onroad"]:
            break
          await asyncio.sleep(0.01)
        assert not raw_state["state"]["onroad"]
        raw.append(raw_state)
      start = time.monotonic()
      assert await command(child, "close") == {"closed": True}
      await asyncio.wait_for(child.wait(), 5)
      assert child.returncode == 0
      elapsed = time.monotonic() - start
      with socket.socket() as released:
        released.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        released.bind(("127.0.0.1", port))
      (output / "raw.json").write_text(json.dumps(raw))
      return {"samples": [view(item) for item in raw], "close_seconds": elapsed, "exit_code": child.returncode, "port_released": True}
    finally:
      if child.returncode is None:
        child.kill()
        await asyncio.wait_for(child.wait(), 5)


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--root", type=Path, required=True)
  cli.add_argument("--native", type=Path)
  cli.add_argument("--output", type=Path)
  cli.add_argument("--serve-original", type=int)
  args = cli.parse_args()
  root = args.root.resolve()
  sys.path.insert(0, str(root))
  sys.path.insert(0, str(root / "openpilot/selfdrive/carrot/cluster"))
  import cluster_navi_source as source

  if args.serve_original is not None:
    original = source.NaviSimulatorSource(host="127.0.0.1", port=args.serve_original, beacon_enabled=False)
    try:
      print(json.dumps({"ready": True, "port": args.serve_original}), flush=True)
      for line in sys.stdin:
        if line.strip() == "update":
          print(json.dumps({"state": normalize(asdict(original.update())), "status": original.status_text()}), flush=True)
        elif line.strip() == "close":
          original.close()
          print(json.dumps({"closed": True}), flush=True)
          break
    finally:
      original.close()
    return
  args.output = args.output.resolve()
  for kind in ["source", "native"]:
    (args.output / kind).mkdir(parents=True, exist_ok=True)
  original = asyncio.run(
    exercise([sys.executable, str(Path(__file__).resolve()), "--root", str(root), "--serve-original"], list(source.CATALOG), args.output / "source")
  )
  (args.output / "source.json").write_text(json.dumps(original, indent=2))
  if args.native is None:
    print(json.dumps({"samples": len(original["samples"]), "source_close": original["close_seconds"], "port_released": True}))
    return
  native = asyncio.run(exercise([str(args.native.resolve()), "--server"], list(source.CATALOG), args.output / "native"))
  assert original["samples"] == native["samples"], (original, native)
  report = {"source": original, "native": native, "native_sha256": hashlib.sha256(args.native.read_bytes()).hexdigest()}
  (args.output / "result.json").write_text(json.dumps(report, indent=2))
  print(
    json.dumps({"samples": len(native["samples"]), "source_close": original["close_seconds"], "native_close": native["close_seconds"], "port_released": True})
  )


if __name__ == "__main__":
  main()
