from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess


def elf(path: Path):
  result = {}
  for option, key in [("-d", "dynamic"), ("--version-info", "versions"), ("--dyn-syms", "symbols")]:
    result[key] = subprocess.run(["readelf", "--wide", option, str(path)], check=True, capture_output=True, text=True).stdout
  result["needed"] = re.findall(r"\(NEEDED\).*?\[(.*?)\]", result["dynamic"])
  result["definitions"] = re.findall(r"Name: (\S+)", result["versions"].split("Version definition section")[-1].split("Version needs section")[0])
  needs, provider = {}, None
  for line in result["versions"].split("Version needs section")[-1].splitlines():
    if match := re.search(r"File: (\S+)", line):
      provider = match[1]
      needs[provider] = []
    if provider and (match := re.search(r"Name: (\S+)", line)):
      needs[provider].append(match[1])
  result["needs"] = needs
  exports, imports = set(), set()
  for line in result["symbols"].splitlines():
    fields = line.split()
    if len(fields) < 8 or not fields[0].endswith(":") or not fields[0][:-1].isdigit():
      continue
    name = fields[7].replace("@@", "@")
    if fields[6] != "UND":
      exports.add(name)
      if "@@" in fields[7]:
        exports.add(name.split("@", 1)[0])
    elif fields[4] != "WEAK":
      imports.add(name)
  result["exports"], result["imports"] = exports, imports
  return result


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument("--qemu", type=Path, required=True)
  cli.add_argument("--loader", type=Path, required=True)
  cli.add_argument("--prefix", type=Path, required=True)
  cli.add_argument("--library", type=Path, action="append", required=True)
  cli.add_argument("--output", type=Path, required=True)
  args = cli.parse_args()
  args.output.mkdir(parents=True, exist_ok=True)
  paths = [args.prefix / "bin/ffmpeg", args.prefix / "bin/ffprobe", args.loader]
  objects, providers = {}, {}
  while paths:
    path = paths.pop().resolve()
    if path in objects:
      continue
    objects[path] = elf(path)
    for needed in objects[path]["needed"]:
      found = next((folder / needed for folder in args.library if (folder / needed).is_file()), None)
      if found is None:
        raise RuntimeError(f"missing target provider {needed} for {path}")
      providers[needed] = found.resolve()
      paths.append(found)
  exports = set().union(*(obj["exports"] for obj in objects.values()))
  report = {"objects": [], "runs": [], "scope": "AArch64 QEMU execution with pinned AGNOS loader and explicit provider paths; no vehicle execution"}
  for path, obj in objects.items():
    missing_versions = {provider: sorted(set(versions) - set(objects[providers[provider]]["definitions"])) for provider, versions in obj["needs"].items()}
    missing_versions = {key: value for key, value in missing_versions.items() if value}
    missing_symbols = sorted(obj["imports"] - exports)
    report["objects"].append({"path": str(path), "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                              "needed": obj["needed"], "missing_versions": missing_versions, "missing_strong_symbols": missing_symbols})
  report["abi_compatible"] = all(not row["missing_versions"] and not row["missing_strong_symbols"] for row in report["objects"])
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  if not report["abi_compatible"]:
    raise RuntimeError("target provider ABI closure failed")
  prefix = [str(args.qemu), str(args.loader), "--library-path", ":".join(map(str, args.library))]

  def run(name, program, argv):
    command = prefix + [str(args.prefix / "bin" / program)] + argv
    result = subprocess.run(command, capture_output=True, timeout=120, check=False)
    (args.output / f"{name}.stdout").write_bytes(result.stdout)
    (args.output / f"{name}.stderr").write_bytes(result.stderr)
    report["runs"].append({"name": name, "command": command, "exit_code": result.returncode})
    (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    result.check_returncode()
    return result.stdout

  run("version", "ffmpeg", ["-version"])
  for codec, extension in [("libx264", "mp4"), ("libvpx", "webm"), ("libvpx-vp9", "webm")]:
    output = args.output / f"{codec}.{extension}"
    run(codec + "-encode", "ffmpeg", ["-nostdin", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=96x64:rate=4",
                                     "-frames:v", "4", "-threads", "1", "-c:v", codec, "-y", str(output)])
    data = run(codec + "-probe", "ffprobe", ["-v", "error", "-show_streams", "-of", "json", str(output)])
    stream = json.loads(data)["streams"][0]
    if (stream["width"], stream["height"]) != (96, 64):
      raise RuntimeError("encoded dimensions differ")
    decoded = run(codec + "-decode", "ffmpeg", ["-nostdin", "-v", "error", "-i", str(output), "-frames:v", "4",
                                               "-threads", "1", "-pix_fmt", "rgba", "-f", "rawvideo", "pipe:1"])
    if len(decoded) != 4 * 96 * 64 * 4:
      raise RuntimeError("decoded frame count differs")
  report["passed"] = True
  (args.output / "report.json").write_text(json.dumps(report, indent=2) + "\n")
  print(json.dumps({"abi_objects": len(objects), "execution_cases": len(report["runs"]), "passed": True}))


if __name__ == "__main__":
  main()
