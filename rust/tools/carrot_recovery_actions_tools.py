# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# How to run: imported by carrot_recovery_actions_control.py.
from __future__ import annotations

import os
from pathlib import Path
import pty
import select
import shlex
import stat
import subprocess
import sys
import time

import carrot_recovery_actions_fixtures as fixture


def terminal_stdio(binary: Path, output: Path, bare: Path) -> None:
  root = output / "inherited_pty"
  root.mkdir()
  observations = {}
  for owner in ("source", "native"):
    repo = fixture.clone(root / owner, bare)
    captured = root / (owner + "-evidence")
    captured.mkdir()
    script = "test -t 0 && test -t 1 && test -t 2 && printf tty-ready; read -r line; printf '%s' \"$line\" > tty-input"
    argv = [str(binary), "__repo-update", "--repo", str(repo), "bash", "-c", script] if owner == "native" else [sys.executable, str(repo / "openpilot/common/repo_update.py"), "--repo", str(repo), "bash", "-c", script]
    master, slave = pty.openpty()
    process = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave, start_new_session=True, env={**os.environ, "CARROT_REPO_LOCK_PATH": str(captured / "lock")})
    os.close(slave)
    try:
      data = b""
      deadline = time.monotonic() + 5
      while b"tty-ready" not in data:
        assert time.monotonic() < deadline, "inherited descriptors did not remain terminals"
        readable, _, _ = select.select([master], [], [], 1)
        if readable:
          data += os.read(master, 8192)
      os.write(master, b"owned-terminal-input\n")
      assert process.wait(timeout=5) == 0
      assert (repo / "tty-input").read_bytes() == b"owned-terminal-input"
      observations[owner] = {"exit": process.returncode, "all_stdio_tty": True, "stdin_observed": "owned-terminal-input"}
      fixture.emit(captured, "terminal.json", {"argv": argv, "pid": process.pid, "stdout_before_input": data.decode(), **observations[owner]})
    finally:
      if process.poll() is None:
        process.kill()
        process.wait(timeout=5)
      os.close(master)
  assert observations["source"] == observations["native"]
  fixture.emit(root, "comparison.json", {"scenario": "repository transaction real PTY descriptors", "source": observations["source"], "native": observations["native"], "pass": True})


def capture_errors(binary: Path, output: Path, bare: Path) -> None:
  scripts = {
    "missing_tmux": None,
    "permission_tmux": "#!/bin/sh\nexit 0\n",
    "nonzero_tmux": "#!/bin/sh\nprintf 'owned stderr\\n' >&2; printf ignored; exit 7\n",
    "whitespace_tmux": "#!/bin/sh\nprintf ' \\n' >&2; exit 7\n",
    "replacement_tmux": "#!/bin/sh\nprintf 'owned\\377\\r\\n'\n",
  }
  for scenario, script in scripts.items():
    root = output / scenario
    root.mkdir()
    repo = fixture.clone(root / "repo", bare)
    tools = root / "tools"
    tools.mkdir()
    if script is not None:
      program = tools / "tmux"
      program.write_text(script)
      program.chmod(0o644 if scenario == "permission_tmux" else 0o755)
    tmux_log = root / "captured.log"
    tmux_log.write_text("old capture must disappear\n")
    module = fixture.source()
    module.TMUX_LOG_PATH = str(tmux_log)
    native_evidence = root / "native-evidence"
    native_evidence.mkdir()
    with fixture.environment({"PATH": str(tools)}):
      source_result = module._tool_action("send_tmux_log", {})
      source_data = tmux_log.read_bytes() if tmux_log.exists() else None
      tmux_log.write_text("old capture must disappear\n")
      with fixture.native(binary, repo, tmux_log, native_evidence) as port:
        native_result = fixture.request(port, {"action": "send_tmux_log"})
      native_data = tmux_log.read_bytes() if tmux_log.exists() else None
    fixture.emit(root, "observed.json", {"source": source_result, "native": native_result, "source_bytes": list(source_data) if source_data else [], "native_bytes": list(native_data) if native_data else []})
    assert source_result == native_result, (scenario, source_result, native_result)
    assert source_data == native_data
    if scenario == "replacement_tmux":
      assert native_data == b"owned\xef\xbf\xbd\n"
    else:
      assert native_result["ok"] is False
      assert not tmux_log.exists()
    fixture.emit(root, "comparison.json", {"scenario": scenario, "response_equal": True, "capture_equal": True, "old_capture_removed": True, "pass": True})


def reboot(binary: Path, output: Path, bare: Path) -> None:
  for scenario in ("sudo_reboot", "fallback_reboot", "missing_reboot"):
    root = output / scenario
    root.mkdir()
    repo = fixture.clone(root / "repo", bare)
    tools = root / "tools"
    tools.mkdir()
    names = {"sudo_reboot": ("sudo",), "fallback_reboot": ("reboot",), "missing_reboot": ()}[scenario]
    for name in names:
      program = tools / name
      program.write_text('#!/bin/sh\nprintf "%s\\n" "$$" > "$CARROT_FIXTURE_REBOOT_PID"\nprintf "%s\\n" "$0 $*" >> "$CARROT_FIXTURE_REBOOT_RECORD"\n')
      program.chmod(0o755)
    results = {}
    for owner in ("source", "native"):
      captured = root / (owner + "-evidence")
      captured.mkdir()
      record = captured / "reboot.argv"
      pid_record = captured / "reboot.pid"
      with fixture.environment({"PATH": str(tools), "CARROT_FIXTURE_REBOOT_RECORD": str(record), "CARROT_FIXTURE_REBOOT_PID": str(pid_record)}):
        if owner == "source":
          result = fixture.source()._git_action("git_reboot", {})
        else:
          with fixture.native(binary, repo, root / "tmux.log", captured) as port:
            result = fixture.request(port, {"action": "git_reboot"})
            if names:
              deadline = time.monotonic() + 3
              while not pid_record.exists() or not record.exists():
                assert time.monotonic() < deadline, "reboot child did not execute"
                time.sleep(0.01)
              pid = int(pid_record.read_text())
              deadline = time.monotonic() + 3
              while Path(f"/proc/{pid}").exists():
                state = Path(f"/proc/{pid}/stat").read_text().rsplit(")", 1)[1].split()[0]
                fixture.emit(captured, "reap.json", {"pid": pid, "state": state, "reaped_while_service_alive": False})
                assert time.monotonic() < deadline, ("unreaped reboot child", pid, state)
                time.sleep(0.01)
              fixture.emit(captured, "reap.json", {"pid": pid, "reaped_while_service_alive": True})
        results[owner] = result
        fixture.emit(captured, "reply.json", result)
        if names:
          deadline = time.monotonic() + 3
          while not record.exists():
            assert time.monotonic() < deadline, "reboot subprocess not observed"
            time.sleep(0.01)
          assert record.read_text().strip() == str(tools / names[0]) + (" reboot" if names[0] == "sudo" else "")
    assert results["source"] == results["native"]
    fixture.emit(root, "comparison.json", {"scenario": scenario, "reboot_program_boundary": "owned recording executable; no OS reboot invoked", "source": results["source"], "native": results["native"], "pass": True})


def reboot_cleanup(binary: Path, output: Path, bare: Path) -> None:
  root = output / "reboot_shutdown"
  root.mkdir()
  repo = fixture.clone(root / "repo", bare)
  tools = root / "tools"
  tools.mkdir()
  (tools / "sudo").write_text('#!/bin/sh\nprintf "%s\\n" "$$" > "$CARROT_FIXTURE_REBOOT_PID"\n/bin/sleep 60 &\nprintf "%s\\n" "$!" > "$CARROT_FIXTURE_REBOOT_DESCENDANT"\nwait\n')
  (tools / "sudo").chmod(0o755)
  pid_record, descendant_record = root / "child.pid", root / "descendant.pid"
  unrelated = subprocess.Popen(["/bin/sleep", "60"])
  captured = root / "native-evidence"
  captured.mkdir()
  try:
    with fixture.environment({"PATH": str(tools), "CARROT_FIXTURE_REBOOT_PID": str(pid_record), "CARROT_FIXTURE_REBOOT_DESCENDANT": str(descendant_record)}):
      with fixture.native(binary, repo, root / "tmux.log", captured) as port:
        start = time.monotonic()
        result = fixture.request(port, {"action": "git_reboot"})
        response_seconds = time.monotonic() - start
        assert result == {"ok": True, "rebooting": True}
        assert response_seconds < 2, "reboot response blocked on owned child"
        deadline = time.monotonic() + 3
        while not pid_record.exists() or not descendant_record.exists():
          assert time.monotonic() < deadline, "owned reboot fixture did not start"
          time.sleep(0.01)
        pid, descendant = int(pid_record.read_text()), int(descendant_record.read_text())
        assert Path(f"/proc/{pid}").exists() and Path(f"/proc/{descendant}").exists()
        assert unrelated.poll() is None
    assert not Path(f"/proc/{pid}").exists(), "reboot leader survived recovery shutdown"
    assert not Path(f"/proc/{descendant}").exists(), "reboot descendant survived recovery shutdown"
    assert unrelated.poll() is None, "shutdown affected an unrelated child"
    fixture.emit(root, "cleanup.json", {"scenario": "native owned asynchronous reboot cleanup", "pid": pid, "descendant": descendant, "unrelated_pid": unrelated.pid, "response_seconds": response_seconds, "reply": result, "leader_reaped": True, "descendant_reaped": True, "unrelated_survived": True, "native_lifecycle_adaptation": True, "pass": True})
  finally:
    unrelated.kill()
    unrelated.wait(timeout=5)


def rebuild(binary: Path, output: Path, bare: Path) -> None:
  for action in ("git_rebuild", "rebuild_all"):
    root = output / (action + "_effects")
    root.mkdir()
    results = {}
    for owner in ("source", "native"):
      repo = fixture.clone(root / owner, bare)
      captured = root / (owner + "-evidence")
      captured.mkdir()
      if owner == "native":
        fixture.git(repo, "remote", "set-url", "origin", "https://github.com/bin9208/openpilot-rust.git")
        helper = repo / "rust/bin/openpilot-runtime-install"
        helper.parent.mkdir(parents=True)
        helper.write_text('#!/bin/sh\nprintf \'%s\\n\' "$@" > package-attempt\necho "Owned matching release unavailable"\nexit 17\n')
        helper.chmod(0o755)
        (repo / "rust/runtime-manifest.json").write_text("old package")
      cache = captured / "cache"
      cache.mkdir()
      (cache / "old").write_text("remove me")
      for filename in ("prebuilt", ".sconsign.dblite"):
        (repo / filename).write_text("remove me")
      tools = captured / "tools"
      tools.mkdir()
      (tools / "scons").write_text('#!/bin/sh\nprintf "%s\\n" "$*" > scons.argv\n')
      (tools / "sudo").write_text('#!/bin/sh\nprintf "%s\\n" "$*" > sudo.argv\n')
      for program in tools.iterdir():
        program.chmod(stat.S_IRUSR | stat.S_IWUSR | stat.S_IXUSR)
      bash_env = captured / "bash-env"
      bash_env.write_text('cd() { if [ "$1" = /data/openpilot ]; then builtin cd "$CARROT_FIXTURE_REPO"; else builtin cd "$@"; fi; }\nrm() { if [ "$*" = "-rf /tmp/scons_cache" ]; then /bin/rm -rf "$CARROT_FIXTURE_CACHE"; else /bin/rm "$@"; fi; }\n')
      env = {"PATH": str(tools) + ":" + os.environ["PATH"], "BASH_ENV": str(bash_env), "CARROT_FIXTURE_REPO": str(repo), "CARROT_FIXTURE_CACHE": str(cache), "CARROT_REPO_LOCK_PATH": str(captured / "lock")}
      with fixture.environment(env):
        if owner == "source":
          module = fixture.source()
          module.REPO_ROOT = repo / "openpilot"
          result = module._tool_action(action, {}) if action == "rebuild_all" else module._git_action(action, {})
        else:
          with fixture.native(binary, repo, captured / "tmux.log", captured) as port:
            result = fixture.request(port, {"action": action})
        fixture.emit(captured, "reply.json", result)
        assert fixture.execute(result["command"], repo, captured) == (0 if owner == "source" else 17)
      observation = {"scons": (repo / "scons.argv").read_text() if (repo / "scons.argv").exists() else "", "prebuilt_removed": not (repo / "prebuilt").exists(), "sign_removed": not (repo / ".sconsign.dblite").exists(), "cache_removed": not cache.exists(), "sudo": (repo / "sudo.argv").read_text() if (repo / "sudo.argv").exists() else ""}
      if owner == "source":
        assert observation["scons"] == "-c\n" and observation["prebuilt_removed"] is True
        if action == "git_rebuild":
          assert observation["sign_removed"] is True and observation["cache_removed"] is True
        else:
          assert observation["sudo"] == "reboot\n"
      else:
        assert observation == {"scons": "", "prebuilt_removed": False, "sign_removed": False, "cache_removed": False, "sudo": ""}
        assert (repo / "rust/runtime-manifest.json").read_text() == "old package"
        assert (repo / "package-attempt").read_text().splitlines()[:3] == ["--stage-update", "--root", str(repo)]
      results[owner] = observation
      fixture.emit(captured, "effects.json", observation)
    assert results["source"] != results["native"], "Original rebuild must fail the native staged contract"
    fixture.emit(root, "comparison.json", {"scenario": action, "owned_path_boundary": "BASH_ENV translates only /data/openpilot and /tmp/scons_cache", "external_boundary": "recording scons/sudo executables; shell deletion is real", "source": results["source"], "native": results["native"], "pass": True})
