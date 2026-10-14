# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# How to run: python3 rust/tools/carrot_recovery_actions_control.py --binary rust/target/debug/examples/recovery_actions --output .omo/evidence/carrot-recovery-263/actions-pairs --tmux PATH
from __future__ import annotations

import argparse
from contextlib import nullcontext
import hashlib
import os
from pathlib import Path
import shlex
import subprocess
import tempfile
import time
from typing import assert_never

import carrot_recovery_actions_fixtures as fixture
import carrot_recovery_actions_tools as tools


def prepare(repo: Path, scenario: str) -> None:
  match scenario:
    case "git_reset" | "git_pull" | "git_reset_repo":
      (repo / "tracked").write_text("locally modified\n")
    case "git_sync":
      fixture.git(repo, "branch", "spare")
    case "git_checkout_local":
      fixture.git(repo, "branch", "local")
    case "git_pull_repair":
      fixture.git(repo, "config", "--replace-all", "remote.origin.fetch", "+refs/heads/gone:refs/remotes/origin/gone")
      fixture.git(repo, "config", "branch.main.merge", "refs/heads/gone")
    case "git_pull_preserve":
      fixture.git(repo, "config", "branch.main.merge", "refs/heads/topic")
    case "git_pull_multiple":
      fixture.git(repo, "config", "--add", "branch.main.merge", "refs/heads/gone")
    case "git_pull_missing":
      fixture.git(repo, "switch", "-c", "absent")
    case "stale_index" | "fresh_index" | "symlink_index":
      index = repo / ".git/index.lock"
      if scenario == "symlink_index":
        index.symlink_to(repo / "tracked")
      else:
        index.write_text("owned fixture lock\n")
        if scenario == "stale_index":
          os.utime(index, (time.time() - 120, time.time() - 120))
    case "git_branches" | "git_log" | "git_rebuild" | "rebuild_all" | "git_checkout" | "git_checkout_commit" | "quoted_branch" | "busy_lock" | "unknown" | "missing_branch" | "false_branch" | "number_commit":
      return
    case unreachable:
      assert_never(unreachable)


def command_body(command: str) -> str:
  args = shlex.split(command)
  if args[-1] == "__git-pull":
    return "git_pull"
  return args[-1].replace("python3 -m openpilot.selfdrive.carrot.server.services.git_config", "git_pull")


def pair(binary: Path, output: Path, bare: Path, scenario: str, payload: dict[str, str | int | bool], execute: bool = True) -> None:
  root = output / scenario
  root.mkdir()
  repos = {name: fixture.clone(root / name, bare) for name in ("source", "native")}
  for repo in repos.values():
    prepare(repo, scenario)
    fixture.git(repo, "remote", "set-url", "origin", "https://github.com/bin9208/openpilot-rust.git")
  native_helper = repos["native"] / "rust/bin/openpilot-runtime-install"
  native_helper.parent.mkdir(parents=True)
  native_helper.write_text('#!/bin/sh\nprintf \'%s\\n\' "$@" > package-attempt\necho "Owned matching release unavailable"\nexit 17\n')
  native_helper.chmod(0o755)
  with (repos["native"] / ".git/info/exclude").open("a") as excluded:
    excluded.write("\n/rust/\n/package-attempt\n")
  before = {owner: fixture.snapshot(repo) for owner, repo in repos.items()}
  if scenario == "git_pull":
    seed = bare.parent / "seed"
    (seed / "advanced").write_text("remote update\n")
    fixture.git(seed, "add", "advanced")
    fixture.git(seed, "commit", "-m", "advance selected upstream")
    fixture.git(seed, "push", str(bare), "main")
  results = {}
  states = {}
  exits = {}
  module = fixture.source()
  global_config = root / "global.gitconfig"
  global_config.write_text(f'[url "{bare}"]\n\tinsteadOf = https://github.com/ajouatom/openpilot.git\n\tinsteadOf = https://github.com/bin9208/openpilot-rust.git\n')
  env = {"CARROT_REPO_LOCK_PATH": str(root / "transaction.lock"), "GIT_CONFIG_GLOBAL": str(global_config), "GIT_CONFIG_NOSYSTEM": "1", "GIT_ALLOW_PROTOCOL": "file", "PYTHONPATH": str(fixture.ROOT)}
  with fixture.environment(env):
    for owner, repo in repos.items():
      captured = root / (owner + "-evidence")
      captured.mkdir()
      lock = fixture.held_lock(root / "transaction.lock") if scenario == "busy_lock" else nullcontext()
      with lock:
        if owner == "source":
          module.REPO_ROOT = repo / "openpilot"
          module.TMUX_LOG_PATH = str(root / "source-tmux.log")
          handler = module._tool_action if payload["action"] in module.TOOL_ACTIONS else module._git_action
          result = handler(payload["action"], payload)
        else:
          with fixture.native(binary, repo, root / "native-tmux.log", captured) as port:
            result = fixture.request(port, payload)
        results[owner] = result
        fixture.emit(captured, "reply.json", result)
        if execute and "command" in result:
          if scenario == "stale_index":
            fixture.wait_git_quiet(captured)
          exits[owner] = fixture.execute(result["command"], repo, captured)
      states[owner] = fixture.snapshot(repo)
      fixture.emit(captured, "repository.json", states[owner])
  normalized = {owner: ({**result, "command": command_body(result["command"])} if "command" in result else result) for owner, result in results.items()}
  typed_actions = {"git_pull": "__git-pull", "git_sync": "__fetch-native", "git_reset": "__stage-repair", "git_rebuild": "__stage-repair", "rebuild_all": "__stage-repair", "git_checkout": "__stage-checkout", "git_checkout_commit": "__stage-commit", "git_reset_repo": "__stage-reset-repo"}
  corrected = payload["action"] in typed_actions and "command" in results["native"]
  if corrected:
    command = results["native"]["command"]
    args = shlex.split(command)
    assert typed_actions[payload["action"]] in args
    assert not any(unsafe in command for unsafe in ("scons", "git clean", "git reset --hard", "ajouatom"))
    assert states["native"]["head"] == before["native"]["head"]
    assert states["native"]["tracked"] == before["native"]["tracked"]
    if execute:
      if scenario in {"fresh_index", "symlink_index", "busy_lock"}:
        assert exits["native"] == 75
      elif scenario == "git_sync":
        assert exits["native"] == 0 and "refs/heads/spare " in states["native"]["refs"]
        assert "refs/heads/spare " not in states["source"]["refs"], "original deletion negative control failed"
      elif scenario in {"quoted_branch", "git_pull_missing"}:
        assert exits["native"] != 0
      else:
        assert exits["native"] == 17, exits
        attempted = (repos["native"] / "package-attempt").read_text().splitlines()
        assert attempted[:3] == ["--stage-update", "--root", str(repos["native"])]
    if scenario == "git_reset_repo":
      assert states["native"]["remote"] == "https://github.com/bin9208/openpilot-rust.git"
      assert states["source"]["remote"] == "https://github.com/ajouatom/openpilot.git"
    if scenario in {"git_reset", "git_pull", "git_reset_repo"}:
      assert states["source"]["tracked"] == "initial\n"
      assert states["native"]["tracked"] == "locally modified\n", "original behavior passed staged native contract"
  else:
    assert normalized["source"] == normalized["native"], normalized
    assert states["source"] == states["native"], states
    assert exits.get("source") == exits.get("native"), exits
  assert not (repos["native"] / "INJECTED").exists()
  fixture.emit(root, "comparison.json", {"scenario": scenario, "payload": payload, "response_equal": normalized["source"] == normalized["native"], "repository_equal": states["source"] == states["native"], "staged_contract": corrected, "exits": exits, "pass": True})


def tmux_pair(binary: Path, output: Path, bare: Path, tmux: Path) -> None:
  root = output / "tmux_capture"
  root.mkdir()
  repo = fixture.clone(root / "repo", bare)
  tools = root / "tools"
  tools.mkdir()
  (tools / "tmux").symlink_to(tmux)
  socket_directory = tempfile.TemporaryDirectory(prefix="op-rec-tmux-")
  socket = Path(socket_directory.name) / "tmux.sock"
  prefix = [str(tmux), "-S", str(socket)]
  shell = f"printf 'owned recovery capture\\n'; {shlex.join(prefix)} wait-for -S ready; exec /bin/cat"
  subprocess.run([*prefix, "new-session", "-d", "-s", "owned", shell], check=True)
  try:
    subprocess.run([*prefix, "wait-for", "ready"], check=True, timeout=5)
    server = subprocess.check_output([*prefix, "display-message", "-p", "#{pid},#{pane_id}"], text=True).strip()
    tmux_environment = str(socket) + "," + server
    module = fixture.source()
    source_log = root / "source.log"
    native_log = root / "native.log"
    module.TMUX_LOG_PATH = str(source_log)
    native_evidence = root / "native-evidence"
    native_evidence.mkdir()
    with fixture.environment({"TMUX": tmux_environment, "PATH": str(tools) + ":" + os.environ["PATH"]}):
      source_result = module._tool_action("send_tmux_log", {})
      with fixture.native(binary, repo, native_log, native_evidence) as port:
        native_result = fixture.request(port, {"action": "send_tmux_log"})
      assert source_result == native_result == {"ok": True, "file": "/download/tmux.log"}
      assert source_log.read_bytes() == native_log.read_bytes()
      assert b"owned recovery capture" in native_log.read_bytes()
      fixture.emit(root, "comparison.json", {"scenario": "real owned tmux capture", "source": source_result, "native": native_result, "captured_sha256": hashlib.sha256(native_log.read_bytes()).hexdigest(), "captured_bytes": native_log.stat().st_size, "pass": True})
  finally:
    try:
      subprocess.run([*prefix, "kill-server"], check=True)
    finally:
      socket_directory.cleanup()


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument("--binary", type=Path, required=True)
  parser.add_argument("--output", type=Path, required=True)
  parser.add_argument("--tmux", type=Path, required=True)
  parser.add_argument("--phase", choices=("all", "remaining"), default="all")
  args = parser.parse_args()
  binary, output = args.binary.resolve(), args.output.resolve()
  output.mkdir(parents=True)
  bare = fixture.remote(output)
  scenarios = [(action, {"action": action}, action != "git_rebuild") for action in ("git_branches", "git_log", "git_reset", "git_sync", "git_pull", "git_rebuild", "rebuild_all")]
  scenarios[-1] = ("rebuild_all", {"action": "rebuild_all"}, False)
  scenarios += [(name, {"action": "git_pull"}, True) for name in ("git_pull_repair", "git_pull_preserve", "git_pull_multiple", "git_pull_missing")]
  scenarios += [("git_checkout", {"action": "git_checkout", "branch": "origin/topic"}, True), ("git_checkout_local", {"action": "git_checkout", "branch": "local"}, True), ("git_checkout_commit", {"action": "git_checkout_commit", "commit": fixture.git(output / "seed", "rev-parse", "HEAD")}, True), ("git_reset_repo", {"action": "git_reset_repo", "branch": "main"}, True), ("quoted_branch", {"action": "git_checkout", "branch": "topic; touch INJECTED"}, True)]
  scenarios += [(name, {"action": "git_reset"}, True) for name in ("stale_index", "fresh_index", "symlink_index", "busy_lock")]
  scenarios += [("unknown", {"action": "unknown"}, False), ("missing_branch", {"action": "git_checkout"}, False), ("false_branch", {"action": "git_checkout", "branch": False}, False), ("number_commit", {"action": "git_checkout_commit", "commit": 17}, False)]
  if args.phase == "remaining":
    scenarios = scenarios[16:]
  for scenario, payload, execute in scenarios:
    pair(binary, output, bare, scenario, payload, execute)
    print("PASS", scenario, flush=True)
  tmux_pair(binary, output, bare, args.tmux.resolve())
  tools.capture_errors(binary, output, bare)
  tools.reboot(binary, output, bare)
  tools.reboot_cleanup(binary, output, bare)
  tools.rebuild(binary, output, bare)
  tools.terminal_stdio(binary, output, bare)
  fixture.emit(output, "summary.json", {"source": str(fixture.SOURCE), "source_sha256": hashlib.sha256(fixture.SOURCE.read_bytes()).hexdigest(), "binary": str(binary), "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "phase": args.phase, "paired_scenarios": len(scenarios) + 12, "native_cleanup_scenarios": 1, "pass": True})


if __name__ == "__main__":
  main()
