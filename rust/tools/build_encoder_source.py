#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def main() -> None:
  parser = argparse.ArgumentParser(description="Build the unchanged original software encoderd for host runtime comparison")
  for name in ["output", "native-prefix", "capnp-prefix", "json11-prefix", "zmq-include", "jpeg-build"]:
    parser.add_argument("--" + name, type=Path, required=True)
  arguments = parser.parse_args()
  root = Path(__file__).resolve().parents[2]
  output = arguments.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  guard = {"free": shutil.disk_usage(output).free, "growth": 512 * 1024**2, "floor": 25 * 1024**3}
  output.joinpath("space.json").write_text(json.dumps(guard, indent=2) + "\n")
  if guard["free"] < guard["floor"] + guard["growth"]:
    raise RuntimeError(f"insufficient disk headroom: {guard}")
  schema = output / "schema"
  generated = output / "cereal/gen/cpp"
  schema.mkdir()
  generated.mkdir(parents=True)
  for name in ["log", "custom", "deprecated"]:
    shutil.copyfile(root / f"openpilot/cereal/{name}.capnp", schema / f"{name}.capnp")
  shutil.copyfile(root / "opendbc_repo/opendbc/car/car.capnp", schema / "car.capnp")
  shutil.copytree(root / "openpilot/cereal/include", schema / "include")
  subprocess.run(["capnp", "compile", f"-I{schema}", f"--src-prefix={schema}", f"-oc++:{generated}", *map(str, schema.glob("*.capnp"))], check=True)
  with output.joinpath("cereal/services.h").open("wb") as services:
    subprocess.run(["python3", str(root / "openpilot/cereal/services.py")], stdout=services, check=True)
  source_names = [
    "openpilot/system/loggerd/encoderd.cc",
    "openpilot/system/loggerd/encoder/encoder.cc",
    "openpilot/system/loggerd/encoder/ffmpeg_encoder.cc",
    "openpilot/system/loggerd/encoder/jpeg_encoder.cc",
    "openpilot/cereal/messaging/socketmaster.cc",
    "openpilot/common/params.cc",
    "openpilot/common/util.cc",
    "openpilot/common/swaglog.cc",
    "msgq_repo/msgq/ipc.cc",
    "msgq_repo/msgq/event.cc",
    "msgq_repo/msgq/impl_msgq.cc",
    "msgq_repo/msgq/impl_fake.cc",
    "msgq_repo/msgq/msgq.cc",
    "msgq_repo/msgq/visionipc/visionipc.cc",
    "msgq_repo/msgq/visionipc/visionipc_client.cc",
    "msgq_repo/msgq/visionipc/visionipc_server.cc",
    "msgq_repo/msgq/visionipc/visionbuf.cc",
  ]
  native = arguments.native_prefix.resolve()
  capnp = arguments.capnp_prefix.resolve()
  json11 = arguments.json11_prefix.resolve()
  jpeg = arguments.jpeg_build.resolve()
  command = ["g++", "-std=c++17", "-O1", "-g0", "-pthread"]
  command += [
    "-I" + str(path)
    for path in [
      root,
      root / "openpilot",
      root / "msgq_repo",
      output,
      generated,
      json11 / "include",
      arguments.zmq_include.resolve(),
      capnp / "include",
      native / "include",
      native / "include/x86_64-linux-gnu",
      root / "rust/crates/jpeg/native/vendor/src",
      jpeg,
    ]
  ]
  command += [
    "-L" + str(capnp / "lib/x86_64-linux-gnu"),
    "-L" + str(native / "lib/x86_64-linux-gnu"),
    f"-Wl,--disable-new-dtags,-rpath,{capnp / 'lib/x86_64-linux-gnu'}",
  ]
  command += [str(root / name) for name in source_names]
  command += [str(path) for path in generated.glob("*.c++")]
  command += [
    str(json11 / "lib/libjson11.a"),
    str(native / "lib/x86_64-linux-gnu/libyuv.a"),
    str(jpeg / "libjpeg.a"),
    "-l:libzmq.so.5",
    "-lavformat",
    "-lavcodec",
    "-lavutil",
    "-lcapnp",
    "-lkj",
    "-o",
    str(output / "original-encoderd"),
  ]
  output.joinpath("build-command.json").write_text(json.dumps(command, indent=2) + "\n")
  with output.joinpath("build.log").open("wb") as log:
    subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=180)
  receipt = {
    "status": "PASS",
    "sources": {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in source_names},
    "binary_sha256": hashlib.sha256(output.joinpath("original-encoderd").read_bytes()).hexdigest(),
  }
  output.joinpath("receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
  print(output / "original-encoderd")


if __name__ == "__main__":
  main()
