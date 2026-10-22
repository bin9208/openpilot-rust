#!/usr/bin/env python3
"""Explicit owner-requested delivery of an unchanged, CI-verified source checkout.

This controller is retrieved at its pinned dispatch commit outside the immutable
source checkout. All build/seal/install functions and the native publisher come from that
source; its marker-only automatic entry point remains unchanged.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys


REPOSITORY = "bin9208/openpilot-rust"
GATES = {"fast checks", "check mapped user docs", "integration gate", "rust checks", "rust aarch64 build"}


def validate_request(issue, sha, run_id, ref, actor):
    owner = REPOSITORY.split("/")[0]
    assert re.fullmatch(r"[a-f0-9]{40}", sha), "invalid source SHA"
    assert re.fullmatch(r"[1-9][0-9]*", run_id), "invalid CI run"
    assert ref.startswith("refs/heads/codex/"), "manual controller requires an issue branch"
    assert actor == owner and issue["user"]["login"] == owner, "request requires repository owner"
    assert issue["state"] == "open" and "pull_request" not in issue, "request is not an open issue"
    lines = issue["body"].splitlines()
    for value in (f"source_sha: {sha}", f"ci_run_id: {run_id}",
                  f"controller_ref: {ref}", "release_authorized: true"):
        assert lines.count(value) == 1, f"request does not authorize {value}"


def validate_ci(run, latest, jobs, sha, run_id):
    assert run["id"] == int(run_id) and run["head_sha"] == sha
    assert run["path"] == ".github/workflows/ci.yml" and run["head_branch"] == "dev-md"
    assert run["event"] == "push" and run["repository"]["full_name"] == REPOSITORY
    assert run["status"] == "completed" and run["conclusion"] == "success", "source CI incomplete or failed"
    assert latest["id"] == run["id"] and latest["run_attempt"] == run["run_attempt"], "superseded CI"
    for name in GATES:
        matching = [job for job in jobs if job["name"] == name]
        assert len(matching) == 1, f"missing/duplicate gate {name}"
        job = matching[0]
        assert job["head_sha"] == sha and job["status"] == "completed" and job["conclusion"] == "success", name


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", f"repos/{REPOSITORY}/{path}"], text=True))


def rows(path, key):
    result = []
    for page in range(1, 101):
        batch = api(path + ("&" if "?" in path else "?") + f"per_page=100&page={page}")[key]
        result.extend(batch)
        if len(batch) < 100:
            return result
    raise AssertionError("unexpected GitHub pagination size")


def authorize():
    assert os.environ.get("GITHUB_ACTIONS") == "true"
    assert os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
    assert os.environ["GITHUB_REPOSITORY"] == REPOSITORY
    assert os.environ["GITHUB_EVENT_NAME"] == "workflow_dispatch"
    sha, run_id, request_id = (os.environ[key] for key in
                               ("RUNTIME_SOURCE_SHA", "RUNTIME_CI_RUN_ID", "RUNTIME_MANUAL_REQUEST"))
    assert re.fullmatch(r"[1-9][0-9]*", request_id), "missing manual request"
    root = Path(os.environ["GITHUB_WORKSPACE"])
    assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip() == sha
    issue = api(f"issues/{request_id}")
    validate_request(issue, sha, run_id, os.environ["GITHUB_REF"], os.environ["GITHUB_ACTOR"])
    assert api("branches/dev-md")["commit"]["sha"] == sha, "dev-md source changed"
    prs = api(f"commits/{sha}/pulls?per_page=100")
    assert any(pr["merged_at"] and pr["merge_commit_sha"] == sha and pr["base"]["ref"] == "dev-md" for pr in prs)
    run = api(f"actions/runs/{run_id}")
    runs = rows(f"actions/runs?event=push&branch=dev-md&head_sha={sha}", "workflow_runs")
    latest = max((value for value in runs if value["path"] == ".github/workflows/ci.yml"), key=lambda value: value["id"])
    validate_ci(run, latest, rows(f"actions/runs/{run_id}/jobs?filter=latest", "jobs"), sha, run_id)
    receipt = dict(source_commit=sha, ci_run_id=run_id, request=issue["html_url"],
                   controller_commit=os.environ["GITHUB_SHA"], controller_ref=os.environ["GITHUB_REF"],
                   controller_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    output = Path(os.environ["RUNNER_TEMP"]) / "agnos-runtime"
    output.mkdir(exist_ok=True)
    (output / "manual-request.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return root, sha


def main(command):
    root, sha = authorize()
    delivery = runpy.run_path(str(root / "rust/ci/runtime/package.py"))
    os.environ["RUSTUP_TOOLCHAIN"] = "1.94.0"
    if command == "source-policy":
        delivery["check_source_policy"]()
    elif command == "prepare":
        delivery["stage_baseline"]()
        delivery["sysroot"]()
        delivery["environment"]()
    elif command == "host-tools":
        delivery["host_tools"]()
    elif command == "build":
        delivery["build"]()
    elif command == "publish":
        delivery["sysroot"]()
        work, release = delivery["WORK"], delivery["RELEASE"]
        identity = json.loads((work / "delivery-tools/identity.json").read_text())
        assert identity["source_commit"] == sha
        publisher = work / "delivery-tools/publish-runtime-release"
        publisher.chmod(0o755)
        assert delivery["digest"](publisher) == identity["publisher_sha256"]
        assert delivery["digest"](release / "openpilot-runtime-install-aarch64") == identity["installer_sha256"]
        delivery["restore_release"]()
        # This exact-source native binary independently rechecks latest push CI,
        # all five real gates, sealed assets and immutable publication identity.
        delivery["run"]([publisher, "--root", root, "--assets", release, "--branch", "dev-md"])
        delivery["install"](True)
    else:
        raise AssertionError("unknown manual delivery command")


if __name__ == "__main__":
    main(sys.argv[1])
