"""Hosted-only regressions using owned ext4 images and real debugfs boundaries."""
from __future__ import annotations

import hashlib
import itertools
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

TOOL = Path(__file__).resolve().parents[1] / "extract_runtime_sdk.py"


class ExtractRuntimeSdkTest(unittest.TestCase):
  def setUp(self):
    self.temporary = tempfile.TemporaryDirectory(prefix="sdk-extract-292-")
    self.addCleanup(self.temporary.cleanup)
    self.root = Path(self.temporary.name)
    self.tree = self.root / "image-tree"
    headers = self.tree / "usr/include/real"
    headers.mkdir(parents=True)
    (headers / "header.h").write_bytes(b"/* original SDK header */\n")
    (headers / "nested").mkdir()
    (headers / "nested/child.h").write_bytes(b"/* nested header */\n")
    (headers / "file-alias.h").symlink_to("header.h")
    self.image = self.root / "sdk.ext4"
    self.output = self.root / "sdk"
    self.record = self.root / "record.json"
    self.identity = self.root / "identity.json"

  def make_image(self):
    with self.image.open("wb") as stream:
      stream.truncate(16 * 1024 * 1024)
    subprocess.run(["mke2fs", "-q", "-t", "ext4", "-F", "-d", str(self.tree), str(self.image)], check=True)
    self.identity.write_text(json.dumps({"raw_size": self.image.stat().st_size,
                                        "raw_sha256": hashlib.sha256(self.image.read_bytes()).hexdigest()}))

  def run_tool(self, *arguments, success=True):
    result = subprocess.run([sys.executable, str(TOOL), "--image", str(self.image),
                             "--identity", str(self.identity), "--output", str(self.output),
                             "--record", str(self.record), *arguments], capture_output=True, text=True, timeout=30)
    if success:
      self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
    else:
      self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
    return result

  def test_plain_file_alias_keeps_exact_bytes_and_original_provenance(self):
    self.make_image()
    self.run_tool("--file", "/usr/include/real/file-alias.h")
    output = self.output / "usr/include/real/file-alias.h"
    self.assertEqual(output.read_bytes(), b"/* original SDK header */\n")
    record = json.loads(self.record.read_text())
    self.assertEqual(record["files"], [{
      "requested": "/usr/include/real/file-alias.h",
      "image_path": "/usr/include/real/header.h",
      "alias_chain": ["/usr/include/real/file-alias.h", "/usr/include/real/header.h"],
      "output": str(output),
      "sha256": hashlib.sha256(b"/* original SDK header */\n").hexdigest(),
    }])
    self.assertEqual(record["directory_aliases"], [])

  def test_directory_aliases_preserve_headers_without_duplicate_tree_traversal(self):
    headers = self.tree / "usr/include"
    (headers / "relative").symlink_to("real", target_is_directory=True)
    (headers / "absolute").symlink_to("/usr/include/real", target_is_directory=True)
    (headers / "chain").symlink_to("relative", target_is_directory=True)
    self.make_image()
    self.run_tool("--tree", "/usr/include", "--tree", "/usr/include", "--tree", "/usr/include/relative",
                  "--tree", "/usr/include/real/nested")
    for directory in ("real", "relative", "absolute", "chain"):
      for header in ("header.h", "file-alias.h"):
        self.assertEqual((self.output / "usr/include" / directory / header).read_bytes(), b"/* original SDK header */\n")
    record = json.loads(self.record.read_text())
    self.assertEqual(record["raw_sha256"], hashlib.sha256(self.image.read_bytes()).hexdigest())
    self.assertEqual(len(record["files"]), 3)
    for file in record["files"]:
      self.assertEqual(hashlib.sha256(Path(file["output"]).read_bytes()).hexdigest(), file["sha256"])
    self.assertEqual(len(record["directory_aliases"]), 3)
    for alias in record["directory_aliases"]:
      link = Path(alias["output"])
      self.assertTrue(link.is_symlink())
      self.assertTrue(link.resolve().is_relative_to(self.output.resolve()))

  def test_child_first_overlapping_roots_and_all_permutations_preserve_materialized_files(self):
    (self.tree / "usr/include/relative").symlink_to("real", target_is_directory=True)
    self.make_image()
    # First permutation is Harvey's exact failure: a child creates the real
    # parent before the relative alias is requested, then the real root follows.
    roots = ("/usr/include/real/nested", "/usr/include/relative", "/usr/include/real")
    expected = {
      "nested/child.h": b"/* nested header */\n",
      "header.h": b"/* original SDK header */\n",
      "file-alias.h": b"/* original SDK header */\n",
    }
    for index, order in enumerate(itertools.permutations(roots)):
      with self.subTest(order=order):
        self.output = self.root / f"sdk-order-{index}"
        self.record = self.root / f"record-order-{index}.json"
        arguments = [argument for path in order for argument in ("--tree", path)]
        # Repeating all roots also checks that aliases are not rebuilt into cycles.
        self.run_tool(*arguments, *arguments)
        canonical = self.output / "usr/include/real"
        alias = self.output / "usr/include/relative"
        self.assertTrue(canonical.is_dir())
        self.assertFalse(canonical.is_symlink())
        self.assertTrue(alias.is_symlink())
        self.assertEqual(alias.resolve(strict=True), canonical)
        for name, data in expected.items():
          self.assertEqual((canonical / name).read_bytes(), data)
          self.assertEqual((alias / name).read_bytes(), data)
          self.assertTrue((canonical / name).samefile(alias / name))
        record = json.loads(self.record.read_text())
        self.assertEqual(record["raw_sha256"], hashlib.sha256(self.image.read_bytes()).hexdigest())
        self.assertEqual(len(record["files"]), len(expected), "overlapping roots re-extracted files")
        self.assertEqual({row["output"] for row in record["files"]},
                         {str(canonical / name) for name in expected})
        for row in record["files"]:
          name = str(Path(row["output"]).relative_to(canonical))
          self.assertEqual(row["sha256"], hashlib.sha256(expected[name]).hexdigest())
        self.assertEqual(len(record["directory_aliases"]), 1)
        self.assertEqual(record["directory_aliases"][0]["output"], str(alias))
        self.assertEqual(record["directory_aliases"][0]["target"], str(canonical))

  def test_outside_tree_image_directory_is_materialized_inside_output(self):
    external = self.tree / "opt/sdk-headers"
    external.mkdir(parents=True)
    (external / "external.h").write_bytes(b"/* image-only header */\n")
    (self.tree / "usr/include/external").symlink_to("/opt/sdk-headers", target_is_directory=True)
    self.make_image()
    self.run_tool("--tree", "/usr/include")
    self.assertEqual((self.output / "usr/include/external/external.h").read_bytes(), b"/* image-only header */\n")
    record = json.loads(self.record.read_text())
    row = next(row for row in record["files"] if row["requested"].endswith("/external.h"))
    self.assertEqual(row["image_path"], "/opt/sdk-headers/external.h")

  def test_excessive_file_symlink_chain_is_bounded(self):
    headers = self.tree / "usr/include"
    for index in range(65):
      (headers / f"link-{index}").symlink_to(f"link-{index + 1}" if index < 64 else "real/header.h")
    self.make_image()
    result = self.run_tool("--file", "/usr/include/link-0", success=False)
    self.assertIn("image symlink loop or depth limit", result.stderr)
    self.assertFalse(self.record.exists())

  def test_directory_cycle_is_rejected_without_unbounded_recursion(self):
    (self.tree / "usr/include/real/back").symlink_to("..", target_is_directory=True)
    self.make_image()
    result = self.run_tool("--tree", "/usr/include", success=False)
    self.assertIn("image directory symlink loop", result.stderr)
    self.assertFalse(self.record.exists())

  def test_missing_file_and_dangling_alias_remain_errors(self):
    self.make_image()
    result = self.run_tool("--file", "/usr/include/missing.h", success=False)
    self.assertIn("missing image file", result.stderr)
    (self.tree / "usr/include/broken").symlink_to("absent", target_is_directory=True)
    self.make_image()
    result = self.run_tool("--tree", "/usr/include", success=False)
    self.assertIn("missing image file", result.stderr)

  def test_output_alias_cannot_escape_destination(self):
    self.make_image()
    outside = self.root / "outside"
    outside.mkdir()
    self.output.mkdir()
    (self.output / "usr").symlink_to(outside, target_is_directory=True)
    result = self.run_tool("--tree", "/usr/include", success=False)
    self.assertIn("SDK output escapes destination", result.stderr)
    self.assertEqual(list(outside.iterdir()), [])

  def test_image_size_and_digest_remain_required(self):
    self.make_image()
    identity = json.loads(self.identity.read_text())
    self.identity.write_text(json.dumps(dict(identity, raw_size=identity["raw_size"] + 1)))
    result = self.run_tool("--tree", "/usr/include", success=False)
    self.assertIn("pinned image size differs", result.stderr)
    self.identity.write_text(json.dumps(dict(identity, raw_sha256="0" * 64)))
    result = self.run_tool("--tree", "/usr/include", success=False)
    self.assertIn("pinned image digest differs", result.stderr)
    self.assertFalse(self.record.exists())


if __name__ == "__main__":
  unittest.main()
