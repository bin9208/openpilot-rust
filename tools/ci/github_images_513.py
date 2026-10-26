#!/usr/bin/env python3
"""#513 전용 GHA 원문 준비/fake HTTP 관측. 새 임시 실행 경계의 등록은 부모 소유다."""
import ast
import base64
import contextlib
import copy
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import sys
import traceback
import types
import urllib.error
import urllib.parse
import urllib.request

REPO = "bin9208/openpilot-rust"
API = "https://api.github.com/repos/" + REPO
ORIGINAL_PATH = "openpilot/tools/lib/github_utils.py"
ORIGINAL_COMMIT = "a364b0b037b3a90ba4c66936013014aa88835ab5"
ORIGINAL_BLOB = "6a443b4155bc7d864bd4749935e9bd842367d5ed"
ORIGINAL_SHA = "05d53b2bfa9cf73b9165b534fc1c81fa88171f9f25f1a15fc9d9009d3ca51cbc"
CURRENT_SHA = "f642a6652ab7238fc2b613ca2f7722b2a4756ef0363169210564d42290aeb063"
NATIVE_SOURCE = "9e7a3b39ebaa2d215e2cadbae0618cc8720f3649"
NATIVE_RUN, NATIVE_JOB, NATIVE_ARTIFACT = 40080413343, 120832600964, 12554902305
NATIVE_ARTIFACT_SHA = "5e79482c0f26815da14a9d649fe1b467fd245143edefc2843ffde2aae013ab9a"
NATIVE_FILES = (
    ("rust/crates/dev-tools/src/github_utils.rs", "d3e4294c2a6cff019aced6c882afed2d2120a163",
     "6010fb32078700cdb054303b8b5bdcbbc9c70eaf74487c6325940ec8bd6db37c"),
    ("rust/crates/dev-tools/tests/github_utils.rs", "663da5f1fcfe2bf6e1c06a225e63083bda72db44",
     "80e0c07885d517cccca0a1c103bad4deded532c921df972be535da22c1a9e492"),
)
IMAGE_TESTS = (
    "images_513_upload_prefix_requested_branch_odd_rows_and_default_post",
    "images_513_missing_private_file_stops_before_pr_lookup",
    "images_513_pr_lookup_failures_retain_complete_upload_prefix",
    "images_513_put_and_comment_failures_keep_existing_effect_order",
    "images_513_empty_commenter_or_branch_keep_false_overwrite_default",
)
PNG = base64.b64decode(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="
)


def require(value, message):
    if not value:
        raise RuntimeError(message)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def identity(s):
    return [s.st_dev, s.st_ino, s.st_mode, s.st_uid, s.st_gid, s.st_nlink,
            s.st_size, s.st_mtime_ns, s.st_ctime_ns]


def read(path, limit):
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_size <= limit, "regular bounded input required")
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        require(identity(os.fstat(fd)) == identity(before), "input open identity changed")
        with os.fdopen(fd, "rb", closefd=False) as stream:
            raw = stream.read(limit + 1)
        require(len(raw) <= limit and identity(os.fstat(fd)) == identity(before)
                == identity(path.lstat()), "input changed while reading")
    finally:
        os.close(fd)
    return raw, {"path": str(path), "identity": identity(before), "sha256": sha(raw)}


def write(path, raw):
    require(len(raw) <= 4 * 1024 * 1024, "proof file exceeds bound")
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())


def record(path, value):
    write(path, (json.dumps(value, indent=2, sort_keys=True) + "\n").encode())


def owned(path):
    s = path.lstat()
    require(stat.S_ISDIR(s.st_mode) and s.st_uid == os.geteuid()
            and stat.S_IMODE(s.st_mode) == 0o700 and path.resolve() == path,
            "canonical owned private directory required")


def context():
    require(os.environ.get("GITHUB_ACTIONS") == "true"
            and os.environ.get("GITHUB_REPOSITORY") == REPO
            and os.environ.get("GITHUB_JOB") == "github-images-513"
            and os.environ.get("GITHUB_EVENT_NAME") in ("push", "workflow_dispatch")
            and os.environ.get("GITHUB_REF") == "refs/heads/codex/feat-523-python-rust"
            and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
            and os.environ.get("RUNNER_OS") == "Linux"
            and os.environ.get("RUNNER_ARCH") == "X64", "fixed hosted Actions context required")
    source = Path(os.environ["DIAG_SOURCE"])
    require(source.is_absolute() and source.resolve() == source and Path.cwd() == source,
            "private source cwd required")
    job = Path(os.environ["CI_JOB_TEMP"])
    require(re.fullmatch(r"/tmp/openpilot-ci\.[A-Za-z0-9]{6}", str(job)), "private job path differs")
    owned(job)
    owned(job / "proof")
    owner = ":".join(os.environ[k] for k in ("GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT", "GITHUB_JOB"))
    require(read(job / "owner", 512)[0].decode().rstrip("\n") == owner, "job owner differs")
    require(re.fullmatch(r"[0-9a-f]{40}", os.environ["GITHUB_SHA"]), "source SHA required")
    return source, job / "proof" / "github-images-513", owner


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def fetch(path, limit, token, logs=False):
    # 인증은 고정 repository의 read-only GET에만 붙인다. proxy/netrc는 사용하지 않는다.
    require(path.startswith("/") and ".." not in path, "fixed API path required")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    url = API + path
    request = urllib.request.Request(url, headers={"Authorization": "Bearer " + token,
        "Accept": "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28"})
    try:
        response = opener.open(request, timeout=30)
    except urllib.error.HTTPError as error:
        if not logs or error.code != 302:
            raise RuntimeError(f"fixed API GET failed with status {error.code}") from None
        target = urllib.parse.urlsplit(error.headers["Location"])
        require(target.scheme == "https" and target.port in (None, 443)
                and not target.username and not target.password
                and target.hostname is not None
                and (target.hostname.endswith(".blob.core.windows.net")
                     or target.hostname.endswith(".actions.githubusercontent.com")),
                "unapproved authenticated job-log redirect")
        # GitHub가 반환한 HTTPS log URL에 Authorization을 전달하지 않는다.
        response = opener.open(urllib.request.Request(target.geturl()), timeout=30)
    with response:
        require(response.status == 200, "fixed GET did not return 200")
        length = response.headers.get("Content-Length")
        require(length is None or int(length) <= limit, "remote body exceeds bound")
        raw = response.read(limit + 1)
    require(len(raw) <= limit, "remote stream exceeds bound")
    return raw


def original_file(root, token, commit, path, blob, digest, name):
    raw = fetch(f"/contents/{path}?ref={commit}", 256 * 1024, token)
    record_path = root / (name + ".api.json")
    write(record_path, raw)
    item = json.loads(raw)
    require(item["type"] == "file" and item["path"] == path and item["sha"] == blob
            and item["encoding"] == "base64" and 0 < item["size"] <= 65536,
            "immutable repository/path/blob metadata differs")
    content = base64.b64decode(item["content"], validate=False)
    require(len(content) == item["size"] and sha(content) == digest
            and hashlib.sha1(b"blob " + str(len(content)).encode() + b"\0" + content).hexdigest() == blob,
            "immutable source bytes differ")
    write(root / name, content)
    return content


def ast_delta(before, current):
    old = ast.parse(before, filename="pinned-original.py")
    new = ast.parse(current, filename=ORIGINAL_PATH)
    changed = copy.deepcopy(old)
    methods = [n for n in ast.walk(changed) if isinstance(n, ast.FunctionDef)
               and n.name == "comment_images_on_pr"]
    require(len(methods) == 1, "unique image method required")
    calls = [n for n in ast.walk(methods[0]) if isinstance(n, ast.Call)
             and isinstance(n.func, ast.Attribute) and n.func.attr == "comment_on_pr"]
    require(len(calls) == 1 and not calls[0].keywords
            and all(isinstance(a, ast.Name) for a in calls[0].args)
            and [a.id for a in calls[0].args] == ["table", "commenter", "pr_branch"],
            "pinned original call signature differs")
    calls[0].args[1], calls[0].args[2] = calls[0].args[2], calls[0].args[1]
    require(ast.dump(changed, include_attributes=False) == ast.dump(new, include_attributes=False),
            "current AST differs beyond the single argument swap")
    return {"single_swap": True, "original_sha256": sha(before), "current_sha256": sha(current),
            "original_args": ["table", "commenter", "pr_branch"],
            "current_args": ["table", "pr_branch", "commenter"], "overwrite_default": False}


def prepare(source, root):
    token = os.environ["GITHUB_TOKEN"]
    require(token and "\n" not in token, "read-only preparation token required")
    before = original_file(root, token, ORIGINAL_COMMIT, ORIGINAL_PATH,
                           ORIGINAL_BLOB, ORIGINAL_SHA, "pinned-original.py")
    current, current_seal = read(source / ORIGINAL_PATH, 65536)
    require(sha(current) == CURRENT_SHA, "current corrected source pin differs")
    seals = [current_seal]
    for index, (path, blob, digest) in enumerate(NATIVE_FILES):
        previous = original_file(root, token, NATIVE_SOURCE, path, blob, digest, f"native-{index}.rs")
        now, seal = read(source / path, 65536)
        require(now == previous, "accepted native implementation/tests changed; no rerun implied")
        seals.append(seal)
    record(root / "ast-delta.json", ast_delta(before, current))
    write(root / "corrected.py", current)
    proofs = {}
    for label, path in (("run", f"/actions/runs/{NATIVE_RUN}"),
                        ("job", f"/actions/jobs/{NATIVE_JOB}"),
                        ("artifact", f"/actions/artifacts/{NATIVE_ARTIFACT}")):
        raw = fetch(path, 1024 * 1024, token)
        write(root / ("native-" + label + ".api.json"), raw)
        proofs[label] = json.loads(raw)
    run, job, artifact = (proofs[k] for k in ("run", "job", "artifact"))
    require(run["id"] == NATIVE_RUN and run["head_sha"] == NATIVE_SOURCE and run["run_attempt"] == 1
            and run["status"] == "completed" and run["conclusion"] == "failure"
            and run["repository"]["full_name"] == REPO, "accepted failed enclosing run differs")
    require(job["id"] == NATIVE_JOB and job["run_id"] == NATIVE_RUN
            and job["head_sha"] == NATIVE_SOURCE and job["run_attempt"] == 1
            and job["status"] == "completed" and job["conclusion"] == "failure", "accepted job differs")
    require(artifact["id"] == NATIVE_ARTIFACT and artifact["size_in_bytes"] == 688030
            and artifact["digest"] == "sha256:" + NATIVE_ARTIFACT_SHA
            and artifact["workflow_run"]["id"] == NATIVE_RUN
            and artifact["workflow_run"]["head_sha"] == NATIVE_SOURCE, "accepted artifact metadata differs")
    logs = fetch(f"/actions/jobs/{NATIVE_JOB}/logs", 4 * 1024 * 1024, token, logs=True)
    write(root / "native-job.raw.log", logs)
    text = re.sub(r"\x1b\[[0-9;]*m", "", logs.decode("utf-8"))
    lines = [re.sub(r"^\d{4}-\d\d-\d\dT\S+Z ", "", line) for line in text.splitlines()]
    starts = [i for i, line in enumerate(lines) if re.search(r"Running tests/github_utils\.rs \(", line)]
    require(len(starts) == 1, "unique actual GithubUtils test invocation required")
    block = lines[starts[0] + 1:]
    ends = [i for i, line in enumerate(block) if re.match(r"test result:", line)]
    require(ends and re.fullmatch(r"test result: ok\. 26 passed; 0 failed; 0 ignored;.*", block[ends[0]]),
            "accepted native26 result absent")
    block = block[:ends[0] + 1]
    for name in IMAGE_TESTS:
        require(block.count(f"test {name} ... ok") == 1, "accepted affected image group absent")
    write(root / "native-accepted-prefix.txt", ("\n".join(block) + "\n").encode())
    record(root / "native-reuse.json", {"source_commit": NATIVE_SOURCE, "run": NATIVE_RUN,
        "job": NATIVE_JOB, "enclosing_job_conclusion": "failure", "accepted_native_tests": 26,
        "affected_image_groups": list(IMAGE_TESTS), "source_seals": seals,
        "job_log_sha256": sha(logs), "cargo_executed": False, "artifact_zip_downloaded": False})
    record(root / "prepared.json", {"source_commit": os.environ["GITHUB_SHA"], "seals": seals})


def compare(source, root):
    require(not any("TOKEN" in key or "AUTH" in key or "PROXY" in key for key in os.environ),
            "comparison must have no token/auth/proxy environment")
    prepared = json.loads(read(root / "prepared.json", 65536)[0])
    require(prepared["source_commit"] == os.environ["GITHUB_SHA"], "preparation source differs")
    require([row["path"] for row in prepared["seals"]]
            == [str(source / ORIGINAL_PATH)] + [str(source / path) for path, _, _ in NATIVE_FILES],
            "complete original/native/test source roster required")
    for row in prepared["seals"]:
        require(read(Path(row["path"]), 65536)[1] == row, "prepared current source changed")
    before, old_seal = read(root / "pinned-original.py", 65536)
    current, new_seal = read(root / "corrected.py", 65536)
    require(sha(before) == ORIGINAL_SHA and sha(current) == CURRENT_SHA, "prepared Python bytes differ")
    delta = ast_delta(before, current)
    image = root / "private-image.png"
    write(image, PNG)
    image_bytes, image_seal = read(image, 4096)
    denied = []

    def deny(event, args):
        if event.startswith("socket.") or event in ("urllib.Request", "http.client.connect",
                "subprocess.Popen", "os.system", "os.exec", "os.posix_spawn", "os.fork"):
            denied.append(event)
            raise PermissionError("#513 comparison forbids real network/process execution")

    sys.addaudithook(deny)
    all_calls = []
    for label, raw, branch, failing in (("pre-fix", before, "commenter-513", True),
                                       ("corrected", current, "requested-branch-513", False)):
        case = root / label
        case.mkdir(mode=0o700)
        calls = []
        data_route = "https://api.github.com/repos/commaai/ci-artifacts/"
        api_route = "https://api.github.com/repos/commaai/openpilot/"
        expected = [("GET", data_route + "git/ref/heads/bucket-513", 200, {"object": {"sha": "exists"}}),
                    ("GET", data_route + "contents/image.png?ref=bucket-513", 404, {}),
                    ("PUT", data_route + "contents/image.png", 201, {}),
                    ("GET", api_route + f"commits/{branch}/pulls", 404 if failing else 200, [{"number": 513}])]
        if not failing:
            expected.append(("POST", api_route + "issues/513/comments", 201, {}))

        def request(method, url, *, headers, data):
            method = str(method)
            row = {"method": method, "url": url, "headers": headers,
                   "data_utf8": data, "data_hex": data.encode().hex(), "transport": "fake"}
            calls.append(row)
            require(len(calls) <= len(expected), "unexpected extra fake request")
            wanted_method, wanted_url, status_code, body = expected[len(calls) - 1]
            require((method, url) == (wanted_method, wanted_url) and headers == {}, "fake request differs")
            row["response_status"] = status_code
            return types.SimpleNamespace(ok=status_code < 400, status_code=status_code, json=lambda: body)

        fake = types.ModuleType("requests")
        fake.request = request
        sys.modules["requests"] = fake
        namespace = {"__name__": "genuine_github_images_513", "__file__": str(root / ("pinned-original.py" if failing else "corrected.py"))}
        stdout, stderr = io.StringIO(), io.StringIO()
        error = None
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            try:
                exec(compile(raw, namespace["__file__"], "exec"), namespace)
                client = namespace["GithubUtils"]("", "")
                returned = client.comment_images_on_pr("github-images-513", "commenter-513",
                    "requested-branch-513", "bucket-513", [("image.png", str(image))])
                require(returned is None, "genuine return value changed")
            except Exception as caught:
                error = {"type": type(caught).__name__, "message": str(caught)}
                traceback.print_exc()
        write(case / "stdout.txt", stdout.getvalue().encode())
        write(case / "stderr.txt", stderr.getvalue().encode())
        write(case / "requests.raw.jsonl", b"".join((json.dumps(row, sort_keys=True) + "\n").encode() for row in calls))
        record(case / "result.json", {"exception": error, "requests": len(calls), "genuine_calls": 1})
        require(len(calls) == len(expected), "genuine request prefix incomplete")
        wanted_error = {"type": "Exception", "message": f"Call to {api_route}commits/commenter-513/pulls failed with 404"}
        require(error == wanted_error if failing else error is None, "genuine result differs")
        upload = json.loads(calls[2]["data_utf8"])
        require(upload == {"message": "uploading image.png", "branch": "bucket-513",
            "committer": {"name": "Vehicle Researcher", "email": "user@comma.ai"},
            "content": base64.b64encode(image_bytes).decode()}, "actual private image upload bytes differ")
        all_calls.append(calls)
    require(all_calls[0][:3] == all_calls[1][:3], "raw upload request prefix changed")
    expected_body = ('{"body": "<details><summary>github-images-513</summary><table><tr><td>'
        '<img src=\\"https://raw.githubusercontent.com/commaai/ci-artifacts/bucket-513/image.png\\">'
        '</td></table></details>"}')
    require(all_calls[1][-1]["data_utf8"] == expected_body, "raw POST/odd-row HTML changed")
    require(not denied, "real network/process action was attempted")
    for path, seal, limit in ((root / "pinned-original.py", old_seal, 65536),
                             (root / "corrected.py", new_seal, 65536), (image, image_seal, 4096)):
        require(read(path, limit)[1] == seal, "original/current/image changed during comparison")
    for row in prepared["seals"]:
        require(read(Path(row["path"]), 65536)[1] == row, "current source changed after genuine calls")
    record(root / "comparison.json", {"passed": True, "source_commit": os.environ["GITHUB_SHA"],
        "ast_delta": delta, "genuine_calls": {"pre_fix": 1, "corrected": 1},
        "raw_upload_prefix_equal": True, "overwrite_default": False, "network_attempts": denied,
        "image_seal": image_seal, "python_seals": [old_seal, new_seal],
        "current_source_seals": prepared["seals"], "native_executed": False,
        "source_retirement_credit": 0})


def main():
    require(len(sys.argv) == 2 and sys.argv[1] in ("prepare", "compare"), "expected prepare or compare")
    phase = sys.argv[1]
    source, root, owner = context()
    if phase == "prepare":
        root.mkdir(mode=0o700)
        write(root / "owner", owner.encode())
    owned(root)
    require(read(root / "owner", 512)[0].decode() == owner, "proof owner differs")
    status = 1
    try:
        if phase == "prepare":
            prepare(source, root)
        else:
            compare(source, root)
        status = 0
    finally:
        record(root / (phase + ".status.json"), {"exit_code": status, "source_commit": os.environ["GITHUB_SHA"]})
    return status


if __name__ == "__main__":
    sys.exit(main())
