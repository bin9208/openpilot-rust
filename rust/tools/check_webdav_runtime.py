from __future__ import annotations

from contextlib import closing
import hashlib
import http.client
import json
import os
from pathlib import Path
import select
import socket
import subprocess
import sys
from typing import TypeAlias

Json: TypeAlias = None | bool | int | float | str | list["Json"] | dict[str, "Json"]


def save(path: Path, value: Json) -> None:
    path.write_text(json.dumps(value, indent=2) + "\n")


def receipt(path: Path) -> Json:
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    return {"path": str(path), "sha256": digest}


def main() -> None:
    if sys.argv[1:] == ["--help"]:
        print("Usage: check_webdav_runtime.py HEALTH_EXAMPLE WORKER_BINARY OUTPUT_DIR")
        return
    health, worker, output = (Path(value).resolve() for value in sys.argv[1:])
    output.mkdir(parents=True, exist_ok=False)
    state = output / "fixture"
    (state / "state").mkdir(parents=True)
    private = state / "state/private_upload.json"
    private.write_text(json.dumps({"url": "http://invalid.example.test/openpilot/", "username": "fixture-user", "password": "fixture-secret"}))
    private.chmod(0o600)
    with closing(socket.socket()) as trap:
        trap.bind(("127.0.0.1", 0))
        trap.listen()
        trap.setblocking(False)
        api_url = f"http://127.0.0.1:{trap.getsockname()[1]}"
        save(state / "state/web_settings.json", {"web_upload_url": api_url})
        save(output / "invocation.json", {"argv": sys.argv, "health": receipt(health), "worker": receipt(worker)})
        worker_request = {
            "parent_pid": os.getpid(), "root": str(state), "id": "fixture-job", "segments": ["2026-10-13--12-00-00--0"],
            "settings": {
                "root": str(state), "base_url": api_url, "token": "",
                "metadata": {"carName": "Test Car", "dongleId": "fixture-device"},
                "webhook": "", "concurrency": 1, "private_upload": str(private),
            },
        }
        process = subprocess.run([str(worker), "--worker"], input=json.dumps(worker_request) + "\n", text=True, capture_output=True, timeout=15, check=False)
        (output / "worker.stdout").write_text(process.stdout)
        (output / "worker.stderr").write_text(process.stderr)
        assert process.returncode == 0
        packets = [json.loads(line) for line in process.stdout.splitlines()]
        assert packets[-1]["event"] == "finish" and packets[-1]["patch"]["ok"] is False
        assert "private upload configuration" in packets[-1]["patch"]["error"]
        assert "fixture-secret" not in process.stdout + process.stderr
        assert "fixture-user" not in process.stdout + process.stderr
        with closing(socket.socket()) as closed_destination:
            closed_destination.bind(("127.0.0.1", 0))
            unavailable_url = f"https://127.0.0.1:{closed_destination.getsockname()[1]}/openpilot/"
        segment = state / "2026-10-13--12-00-00--0"
        segment.mkdir()
        (segment / "rlog.zst").write_bytes(b"synthetic route log")
        (segment / "qcamera.ts").write_bytes(b"synthetic video")
        with (output / "health.stderr").open("w") as errors:
            with subprocess.Popen([str(health)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, text=True) as server:
                assert server.stdin is not None and server.stdout is not None
                server.stdin.write(json.dumps({"state": str(state), "repository": str(state), "composed": False}) + "\n")
                server.stdin.flush()
                assert select.select([server.stdout], [], [], 5)[0]
                ready = json.loads(server.stdout.readline())
                try:
                    with closing(http.client.HTTPConnection("127.0.0.1", ready["port"], timeout=5)) as connection:
                        connection.request("POST", "/api/dashcam/upload/test", body=b"")
                        response = connection.getresponse()
                        raw = response.read().decode()
                        payload = json.loads(raw)
                        save(output / "health.json", {"status": response.status, "payload": payload})
                        assert response.status == 500 and payload["ok"] is False
                        assert "private upload configuration" in payload["error"]
                        assert "fixture-secret" not in raw and "fixture-user" not in raw
                    private.write_text(json.dumps({"url": unavailable_url, "username": "fixture-user", "password": "fixture-secret"}))
                    unavailable = subprocess.run(
                        [str(worker), "--worker"], input=json.dumps(worker_request) + "\n",
                        text=True, capture_output=True, timeout=15, check=False,
                    )
                    (output / "worker-webdav.stdout").write_text(unavailable.stdout)
                    (output / "worker-webdav.stderr").write_text(unavailable.stderr)
                    assert unavailable.returncode == 0
                    failed_result = json.loads(unavailable.stdout.splitlines()[-1])["patch"]["result"]
                    assert failed_result["ok"] is False and failed_result["target"] == "webdav"
                    assert failed_result["storageOnly"] is True and "webComplete" not in failed_result
                    assert failed_result["remoteBasePath"] == "routes/Test Car fixture-device/"
                    assert "Open & Analyze" not in failed_result["shareText"] and "· Open" not in failed_result["shareText"]
                    assert "fixture-secret" not in unavailable.stdout + unavailable.stderr
                    assert unavailable_url not in unavailable.stdout + unavailable.stderr
                    with closing(http.client.HTTPConnection("127.0.0.1", ready["port"], timeout=5)) as connection:
                        connection.request("POST", "/api/dashcam/upload/test", body=b"")
                        response = connection.getresponse()
                        raw = response.read().decode()
                        payload = json.loads(raw)
                        save(output / "health-webdav.json", {"status": response.status, "payload": payload})
                        assert response.status == 502 and payload["ok"] is False
                        assert payload["target"] == "webdav" and payload["storageOnly"] is True
                        assert "fixture-secret" not in raw and unavailable_url not in raw
                finally:
                    server.stdin.write("stop\n")
                    server.stdin.flush()
                    server.wait(timeout=5)
                assert server.returncode == 0
        assert "fixture-secret" not in (output / "health.stderr").read_text()
        try:
            unexpected, _ = trap.accept()
        except BlockingIOError:
            contacted_api = False
        else:
            with closing(unexpected):
                contacted_api = True
        assert not contacted_api
        save(output / "result.json", {
            "ok": True, "worker_finish_failed": True, "worker_webdav_failure_is_truthful": True,
            "health_http_status": 500, "health_webdav_status": 502, "api_connections": 0, "credential_leaks": 0,
        })
        print(json.dumps({"ok": True, "output": str(output)}))


if __name__ == "__main__":
    main()
