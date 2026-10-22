"""Hosted regression: run the real JPEG Cargo build script with tiny CMake roots.

Pass the build-script-build executable produced by building openpilot-jpeg.
No vendor/provider download, Cargo clean, or source rewriting is performed here.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
  cli = argparse.ArgumentParser(description=__doc__)
  cli.add_argument("--build-script", type=Path, required=True)
  cli.add_argument("--target", default="x86_64-unknown-linux-gnu")
  args = cli.parse_args()
  executable = args.build_script.resolve(strict=True)
  host = subprocess.run(["rustc", "-vV"], check=True, capture_output=True, text=True).stdout
  host = next(line.removeprefix("host: ") for line in host.splitlines() if line.startswith("host: "))
  with tempfile.TemporaryDirectory(prefix="jpeg-cache-345-") as temporary:
    root = Path(temporary)
    output = root / "shared-out"
    output.mkdir()
    unrelated = output / "unrelated.keep"
    unrelated.write_bytes(b"outside owned JPEG cache")
    env = dict(os.environ, CARGO_FEATURE_NATIVE_SKIP_MIRI="1", OUT_DIR=str(output),
               HOST=host, TARGET=args.target, CARGO_CFG_TARGET_ARCH=args.target.split("-")[0],
               OPT_LEVEL="0", DEBUG="false", PROFILE="debug", NUM_JOBS="2",
               CARGO_PKG_NAME="openpilot-jpeg", CARGO_MANIFEST_LINKS="openpilot_jpeg",
               CARGO_ENCODED_RUSTFLAGS="")
    for checkout in ("first checkout", "second checkout"):
      source = root / checkout
      vendor = source / "native/vendor"
      vendor.mkdir(parents=True)
      (vendor / "src").mkdir()
      (source / "src").mkdir()
      (vendor / "CMakeLists.txt").write_text(
        "cmake_minimum_required(VERSION 3.16)\nproject(cache_fixture C)\n"
        "add_library(jpeg-static STATIC fixture.c)\n"
        "set_target_properties(jpeg-static PROPERTIES OUTPUT_NAME jpeg)\n")
      (vendor / "fixture.c").write_text("int jpeg_cache_fixture(void) { return 345; }\n")
      (source / "src/bridge.rs").write_text("#[cxx::bridge] mod ffi {}\n")
      for name in ("bridge.cc", "turbo.cc", "encode.c"):
        (source / "native" / name).write_text("/* build-cache fixture */\n")
      env["CARGO_MANIFEST_DIR"] = str(source)

      def build(expect_success=True):
        result = subprocess.run([str(executable)], cwd=source, env=env, capture_output=True, text=True)
        if expect_success:
          assert result.returncode == 0, result.stdout + result.stderr
        else:
          assert result.returncode != 0, "invalid CMake input was silently accepted"
        return result

      build()
      cache = output / "jpeg/CMakeCache.txt"
      assert f"CMAKE_HOME_DIRECTORY:INTERNAL={vendor}" in cache.read_text()
      assert (output / "jpeg/libjpeg.a").is_file()
      marker = output / "jpeg/reuse.keep"
      marker.write_bytes(checkout.encode())
      build()
      assert marker.read_bytes() == checkout.encode(), "same-source cache was discarded"
      assert unrelated.read_bytes() == b"outside owned JPEG cache"
      if checkout == "first checkout":
        # Source identity must work even when the former checkout has disappeared.
        shutil.rmtree(source)
      else:
        (vendor / "CMakeLists.txt").write_text("message(FATAL_ERROR \"fixture configure failure\")\n")
        result = build(expect_success=False)
        assert "fixture configure failure" in result.stdout + result.stderr
        assert marker.read_bytes() == checkout.encode()
    assert unrelated.read_bytes() == b"outside owned JPEG cache"
  print(f"JPEG CMake source switch, same-source reuse, error propagation: {args.target} PASS")


if __name__ == "__main__":
  main()
