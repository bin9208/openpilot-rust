from __future__ import annotations

import argparse
import ast
from collections.abc import Callable
import hashlib
import json
from pathlib import Path
import subprocess
from typing import TypedDict, cast
from urllib.parse import urlsplit


class GitInput(TypedDict):
  remote: str | None
  branch: str | None


def run(binary: Path, output: Path) -> None:
  source = Path("opendbc_repo/opendbc/car/car_helpers.py")
  module = ast.parse(source.read_text())
  function = next(node for node in module.body if isinstance(node, ast.FunctionDef) and node.name == "format_git_source")
  isolated = ast.Module(body=[function], type_ignores=[])
  scope: dict[str, object] = {"urlsplit": urlsplit}
  exec(compile(isolated, str(source), "exec"), scope)
  formatter = cast(Callable[[str | None, str | None], str], scope["format_git_source"])
  remotes = [None, "", "git@github.com:owner/repo.git", "https://user:secret@github.com/owner/repo", "ssh://host/owner/repo",
             "file:///owner/repo", "host/owner/repo", "https://host/one", "https://host//owner//repo", "https://host/owner;name/repo",
             "https://[::1]/owner/repo", "https://[::1]suffix/owner/repo", "https://[::1]]/owner/repo",
             "https://prefix[::1]/owner/repo", "https://[127.0.0.1]/owner/repo", "https://[vF.test]/owner/repo",
             "https://[vF.test]:nonnumeric/owner/repo", "https://[vF.test]suffix/owner/repo", "https://[V1.test]/owner/repo",
             "https://[fe80::1%eth0]/owner/repo", "https://[fe80::1%]/owner/repo", "https://[fe80::1%a%b]/owner/repo",
             "https://host/owner/repo?token=secret", "https://host/owner/repo#token=secret", "ht+tp://host/owner/repo",
             "1https://host/owner/repo", "https://host:/owner/repo", "https://:443/owner/repo", "https://user@/owner/repo"]
  remotes.extend(f"{chr(code)}https://host/owner/repo" for code in range(33))
  remotes.extend(f"https://ho{char}st/owner/repo" for char in "\t\r\n\u2047\u2100\ufe13\ufe5f\uff0f\uff1a\uff20")
  cases: list[GitInput] = [{"remote": remote, "branch": branch} for remote in remotes for branch in [None, "", "dev", "feature/branch", "한글"]]
  expected = [formatter(case["remote"], case["branch"]) for case in cases]
  output.mkdir(parents=True, exist_ok=True)
  (output / "inputs.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n")
  (output / "source.json").write_text(json.dumps(expected, ensure_ascii=False, indent=2) + "\n")
  process = subprocess.run([str(binary)], input=json.dumps(cases).encode(), capture_output=True, check=False)
  (output / "native.json").write_bytes(process.stdout)
  (output / "native.stderr").write_bytes(process.stderr)
  assert process.returncode == 0, process.returncode
  actual = json.loads(process.stdout)
  differences = [{"case": case, "source": original, "native": native}
                 for case, original, native in zip(cases, expected, actual, strict=True) if original != native]
  result = {"cases": len(cases), "differences": differences, "source_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
  (output / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
  assert not differences, differences[:4]
  print(f"PASS {len(cases)} exact original Git banner cases")


if __name__ == "__main__":
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  args = parser.parse_args()
  run(args.binary.resolve(), args.output.resolve())
