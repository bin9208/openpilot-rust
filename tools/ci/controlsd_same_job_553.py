#!/usr/bin/env python3
"""#553 같은 VM fullnormal40 뒤 한정 삼각 관측. 기능은 Actions 전용, 로컬 UNRUN."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import stat
import sys
import time
import traceback

COMMON = "tools/ci/controlsd_normal_523.py"
COMMON_SHA = "8a5da6caaf97dc00a0672e271130e0ce0ead8c055bee9883f7e503b9904b85a7"
DIAGNOSTIC = "tools/ci/controlsd_diagnostic_553.py"
DIAGNOSTIC_SHA = "b050f59a60a0afa1450ec935f4d07ceb5d57b6ae7943d7eadeede14f2968dfa4"
WORKFLOW = ".github/workflows/python-rust-523-controls-same-job.yml"
JOB = "controls-same-job-553"
JOB_NAME = "Same VM normal40 and frozen original native triage"
MIB = 1024**2


def need(value, message):
    if not value:
        raise RuntimeError(message)


def module(root, name, pin):
    path = root / name
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        before = os.fstat(fd)
        need(stat.S_ISREG(before.st_mode) and before.st_size <= 128*1024, "bounded regular source module")
        raw = os.pread(fd, 128*1024 + 1, 0)
        need(before == os.fstat(fd) and before == path.stat(follow_symlinks=False)
             and len(raw) == before.st_size and hashlib.sha256(raw).hexdigest() == pin,
             "exact source module identity/SHA: " + name)
    finally:
        os.close(fd)
    spec = importlib.util.spec_from_file_location("_same_job_" + path.stem, path)
    loaded = importlib.util.module_from_spec(spec)
    exec(compile(raw, str(path), "exec"), loaded.__dict__)
    return loaded


def failure_execs(c, normal, artifacts, raw):
    """기존 exec 문법을 재사용하되 실패한 CLI의 SIGCHLD status1만 명시 수용한다."""
    calls, pending = [], {}
    for line in raw.splitlines():
        row = re.fullmatch(r"([1-9][0-9]*) +(.*)", line)
        need(row, "unknown failure trace prefix")
        pid, body = int(row[1]), row[2]
        if body.endswith("<unfinished ...>"):
            need(pid not in pending and body.startswith("execve("), "failure trace pending")
            pending[pid] = body.removesuffix("<unfinished ...>")
            continue
        if body.startswith("<... "):
            resumed = re.fullmatch(r"<\.\.\. execve resumed>(.*)", body)
            need(resumed and pid in pending, "failure trace orphan resume")
            body = pending.pop(pid) + resumed[1]
        if body.startswith("execve"):
            call = re.fullmatch(r'execve\(("(?:[^"\\]|\\.)*"), (\[.*\]), .*\) = 0', body)
            need(call, "failed/opaque failure-trace exec")
            calls.append({"pid":pid,"program":c.unique_json(call[1]),"argv":c.unique_json(call[2])})
        else:
            metadata = (r"\+\+\+ exited with (?:0|1|101) \+\+\+",
                r"--- SIGCHLD \{si_signo=SIGCHLD, si_code=CLD_EXITED, si_pid=[1-9][0-9]*, si_uid=[0-9]+, si_status=(?:0|1), si_utime=[0-9]+(?: /\* [0-9]+\.[0-9]{2} s \*/)?, si_stime=[0-9]+(?: /\* [0-9]+\.[0-9]{2} s \*/)?\} ---",
                r"--- SIGUSR1 \{si_signo=SIGUSR1, si_code=SI_TKILL, si_pid=[1-9][0-9]*, si_uid=[0-9]+\} ---")
            need(any(re.fullmatch(pattern,body) for pattern in metadata), "unknown failure-trace metadata")
    need(not pending and len(calls)==3, "exact auditor/CLI/native exec attempts")
    auditor, checker, trace = (artifacts[k]["executable"] for k in ("auditor","checker","trace"))
    expected = [[auditor,"--ignored","--exact",c.ACTUAL,"--nocapture","--test-threads=1","--color=never"],
        [checker,"--root",str(normal.root),"--trace",trace,"--numerics",str(normal.numerics),
         "--evidence",str(normal.out/"native"),"--source-commit",normal.source,
         "--frozen-reference",c.REFERENCE_ID,"--reference-dir",str(normal.reference)]]
    need(all(calls[i]["program"]==argv[0] and calls[i]["argv"]==argv for i,argv in enumerate(expected))
         and calls[2]["program"]==trace and len(calls[2]["argv"])==2 and calls[2]["argv"][0]==trace
         and Path(calls[2]["argv"][1]).is_relative_to(normal.out/"native"), "actual failure ELF/CLI argv differs")
    return calls


def main():
    need(len(sys.argv) == 1 and sys.flags.isolated and sys.flags.no_site and sys.dont_write_bytecode,
         "zero-arg stdlib-only Actions controller")
    root = Path(os.environ["DIAG_SOURCE"])
    need(os.environ["GITHUB_JOB"] == JOB and os.environ["GITHUB_EVENT_NAME"] == "push"
         and os.environ["GITHUB_REF"] == "refs/heads/codex/feat-523-python-rust"
         and os.environ["GITHUB_WORKFLOW_REF"] == "bin9208/openpilot-rust/" + WORKFLOW + "@" + os.environ["GITHUB_REF"]
         and os.environ["GITHUB_WORKFLOW_SHA"] == os.environ["GITHUB_SHA"], "actual workflow/job/ref/source binding")
    c = module(root, COMMON, COMMON_SHA)
    d = module(root, DIAGNOSTIC, DIAGNOSTIC_SHA)
    d.hosted()
    evidence = Path(os.environ["CI_JOB_TEMP"]) / "proof/controls-same-job-553"
    evidence.mkdir(mode=0o700)
    state = {"schema": 1, "source": os.environ["GITHUB_SHA"], "normal_status": None, "normal_attempted": False,
             "normal_pass": False, "triage_observed": False, "normal_status_promoted": False,
             "normal_workflow_replaced": False, **d.FLAGS}

    class Normal(c.Probe):
        def command(self, argv, label, seconds=60, env=None, limit=32*MIB, allowed=None, trace=None):
            if label == "normal40":
                # 실행에 전달하는 값을 그대로 저장한다. 인증 download 환경은 저장하지 않는다.
                c.save(evidence / "normal-execution.json", {"argv": list(map(str, argv)),
                    "environment": env or self.env, "cwd": str(self.root), "seconds": seconds,
                    "native_timeout_seconds": 600, "auditor_timeout_seconds": 700,
                    "scope": "exact super.command inputs; native overlay is source-bound CommandSpec, not proc-environ observation"})
            return super().command(argv, label, seconds, env, limit, allowed, trace)

    normal = None
    normal_finished = False
    triage = None
    error = None
    passed = False
    signal.signal(signal.SIGTERM, lambda n, f: (_ for _ in ()).throw(InterruptedError(f"same-job signal {n}")))
    signal.signal(signal.SIGINT, lambda n, f: (_ for _ in ()).throw(InterruptedError(f"same-job signal {n}")))
    try:
        normal = Normal()
        normal.holds.add(root / COMMON, COMMON_SHA)
        normal.holds.add(root / DIAGNOSTIC, DIAGNOSTIC_SHA)
        normal.holds.add(Path(__file__).resolve())
        normal.holds.add(root / WORKFLOW)
        normal.holds.add(Path("/usr/bin/gh"), elf=True)
        auth = {**normal.env, "GH_TOKEN": os.environ["GH_TOKEN"], "GH_TELEMETRY": "false"}
        run = c.unique_json(normal.command(["/usr/bin/gh", "api", f"repos/{c.REPO}/actions/runs/{os.environ['GITHUB_RUN_ID']}"],
                                           "same-job-run-api", env=auth, limit=MIB))
        jobs = c.unique_json(normal.command(["/usr/bin/gh", "api", f"repos/{c.REPO}/actions/runs/{os.environ['GITHUB_RUN_ID']}/attempts/{os.environ['GITHUB_RUN_ATTEMPT']}/jobs?per_page=100"],
                                            "same-job-jobs-api", env=auth, limit=MIB))
        actual_jobs = [j for j in jobs["jobs"] if j["name"] == JOB_NAME]
        need(run["head_sha"] == normal.source and run["event"] == "push"
             and run["path"] == WORKFLOW and run["run_attempt"] == int(os.environ["GITHUB_RUN_ATTEMPT"])
             and jobs["total_count"] == 1 and len(actual_jobs) == 1
             and actual_jobs[0]["run_id"] == run["id"]
             and actual_jobs[0]["labels"] == ["ubuntu-24.04"], "actual API workflow/source/job identity")
        c.save(evidence / "actual-identity.json", {"run_id": run["id"], "job_id": actual_jobs[0]["id"],
            "source": normal.source, "job_key": JOB, "workflow": WORKFLOW, "ref": os.environ["GITHUB_REF"],
            "runner": actual_jobs[0]["runner_name"], "same_vm": True, "uname": list(os.uname())})
        normal.holds.add(Path("/usr/bin/lscpu"), elf=True)
        normal.command(["/usr/bin/lscpu", "--json"], "same-job-cpu", seconds=30)
        with open("/proc/sys/kernel/random/boot_id", "r", encoding="ascii") as stream:
            boot_id = stream.read(128).strip()
        need(re.fullmatch(r"[0-9a-f-]{36}",boot_id), "bounded actual boot identity")
        state["boot_id"] = boot_id
        state["normal_attempted"] = True
        try:
            normal.run()
        except BaseException:
            error = traceback.format_exc()
        # 기존 normal finish도 그대로 호출: 비교 실패/미준비 baseline을 성공으로 덮지 않는다.
        normal_pass = normal.finish(error)
        normal_finished = True
        state.update(normal_pass=normal_pass, normal_status=0 if normal_pass else 1,
                     normal_error=error, normal_phase=normal.phase)
        c.save(evidence / "normal-verdict.json", state)
        print(json.dumps({"normal_pass":normal_pass,"normal_phase":normal.phase,"normal_error":error}),flush=True)
        if normal_pass:
            state["disposition"] = "normal-passed-no-target-replay"
            passed = True
        else:
            stderr = c.read(normal.out / "normal40.stderr", 32*MIB).decode() if (normal.out / "normal40.stderr").is_file() else ""
            mismatch = re.findall(r"^controlsd native: controls\[3\]\.rows\[0\]\.state\.lat_pid\[0\]: (-?[0-9.eE+]+) != (-?[0-9.eE+]+)$", stderr, re.M)
            marker = re.findall(r'CONTROLSD_FROZEN_FAILURE_FINAL_SEALS (\{[^\n]+\})', stderr)
            command = [r for r in normal.commands if r["label"] == "normal40"]
            if not (normal.phase == "actual-normal40" and len(mismatch) == len(marker) == len(command) == 1
                    and command[0]["returncode"] == 101 and c.unique_json(marker[0]) == {"errors": [], "pass": True}):
                state["disposition"] = "other-normal-failure-no-target-replay"
            else:
                seal = c.unique_json(c.read(normal.out / "final-seals.json"))
                need(seal["failures"] == ["native: native: baseline unavailable"]
                     and seal["all_owned_children_reaped"] and normal.holds.records()
                     and not normal.children.scan(), "normal failure has additional seal/reap errors")
                normal.holds.verify()
                artifacts = c.unique_json(c.read(normal.out / "current-artifacts.json"))
                raw_trace = c.read(normal.out / "normal40.exec",16*MIB).decode("ascii")
                calls = failure_execs(c,normal,artifacts,raw_trace)
                # 실제 성공 파싱 뒤 같은 raw의 누락/추가/경로변조/unknown을 GHA에서만 거부한다.
                for bad in ("\n".join(raw_trace.splitlines()[1:])+"\n",raw_trace+raw_trace,
                            raw_trace.replace(artifacts["trace"]["executable"],"/unowned/controlsd_trace"),
                            raw_trace+"999999 unknown_operation() = 0\n"):
                    try:
                        failure_execs(c,normal,artifacts,bad)
                    except RuntimeError:
                        pass
                    else:
                        raise RuntimeError("failure-trace mutation was accepted")
                maps = normal.runtime_pid_maps.get(calls[2]["pid"],{}).get("files",set())
                need(artifacts["trace"]["executable"] in maps and str(normal.numerical_library) in maps,
                     "actual native ELF and numerical library mappings were not sampled")
                c.save(evidence / "actual-failure-executions.json", {"executions":calls,
                    "sampled_native_pid_maps":sorted(maps),"negative_data_controls":4,
                    "mapping_scope":"normal 20ms loop plus hashing overhead; not complete loader/kernel observation"})
                context = {"identity": c.unique_json(c.read(evidence / "actual-identity.json")),
                    "normal_verdict": state.copy(), "first_failure": mismatch[0], "normal_proof": str(normal.out),
                    "normal_proof_after_failure": c.snapshot(normal.out, c.walk(normal.out)),
                    "normal_holds": normal.holds.records(), "normal_command": command[0],
                    "failure_executions": calls,
                    "normal_execution": c.unique_json(c.read(evidence / "normal-execution.json")),
                    "core": c.CORE, "common_sha256": COMMON_SHA,
                    "source_spec": {p: c.regular(root / p) for p in c.CORE}}
                c.save(evidence / "normal-admission.json", context)
                triage = replay_probe(c, d, normal, context, evidence)
                try:
                    triage.initialize(argparse.Namespace(root=str(root), python=os.environ["ORIGINAL_PYTHON"],
                        source_commit=normal.source, evidence=str(normal.job / "proof/controls-diagnostic-553")))
                    triage.run()
                except BaseException:
                    error = traceback.format_exc()
                else:
                    error = None
                observed = triage.finish(error)
                state.update(disposition="target-normal-failure-observed", triage_observed=observed,
                             triage_error=error)
    except BaseException:
        error = traceback.format_exc()
        state.update(controller_error=error)
        print(error, file=sys.stderr)
    finally:
        failures = []
        if normal is not None:
            if not normal_finished:
                try:
                    normal.finish(error or "controller interrupted before normal finish")
                except BaseException:
                    failures.append(traceback.format_exc())
            operations = [normal.children.stop, normal.holds.verify]
            if triage is not None and triage.children is not None:
                operations.insert(0, triage.children.stop)
            for operation in operations:
                try:
                    operation()
                except BaseException:
                    failures.append(traceback.format_exc())
            if triage is not None and triage.baseline is not None:
                try:
                    need(c.snapshot(normal.out, c.walk(normal.out)) == triage.baseline,
                         "normal failed proof changed after triage/finalizers")
                    c.save(evidence / "normal-proof-after.json", triage.baseline)
                except BaseException:
                    failures.append(traceback.format_exc())
            if triage is not None and triage.holds is not None:
                triage.holds.close()
                if triage.children is not None:
                    os.close(triage.children.journal)
            normal.holds.close()
            os.close(normal.children.journal)
        if "boot_id" in state:
            try:
                with open("/proc/sys/kernel/random/boot_id", "r", encoding="ascii") as stream:
                    need(stream.read(128).strip() == state["boot_id"], "same boot identity after triage")
            except BaseException:
                failures.append(traceback.format_exc())
        state["normal_preservation_final_errors"] = failures
        state["pass"] = passed and not failures and error is None
        c.save(evidence / "outcome.json", state)
    return 0 if state["pass"] else 1


def replay_probe(c, d, normal, context, evidence):
    class Replay(d.Diagnostic):
        same_job_normal_mode = True
        baseline = None

        def run(self):
            self.deadline = time.monotonic() + 32*60
            self.phase = "same-job-normal-admission"
            self.budget(force=True)
            need(context["identity"]["source"] == self.source and context["identity"]["job_key"] == JOB
                 and context["normal_proof"] == str(self.job / "proof/controls-normal40")
                 and context["common_sha256"] == COMMON_SHA and context["core"] == c.CORE,
                 "immutable same-job context")
            self.baseline = context["normal_proof_after_failure"]
            self.holds.add(evidence / "normal-admission.json")
            need(self.snapshot(normal.out) == self.baseline, "failed normal proof before reuse")
            for name, row in self.baseline.items():
                need("symlink" not in row, "normal proof aliases forbidden")
                if "sha256" in row:
                    path = normal.out / name
                    need(c.regular(path) == row, "failed normal proof FD baseline")
                    self.holds.add(path,row["sha256"])
            for path, row in context["normal_holds"].items():
                need(c.regular(Path(path)) == {k: row[k] for k in ("identity", "sha256")}, "normal held input before reuse")
                self.holds.add(Path(path), row["sha256"])
            for path, pin in d.SOURCE_PINS.items():
                self.holds.add(self.root / path, pin)
            self.baselines["source"] = self.sources("before")
            need(self.baselines["source"]["files"] == normal.source_before["files"]
                 and self.baselines["source"]["index_sha256"] == normal.source_before["index_sha256"],
                 "same whole normal source/index before replay")
            self.baselines["sdk"] = c.snapshot(self.sdk, c.walk(self.sdk), sdk=True)
            need(self.baselines["sdk"] == normal.sdk_before, "same whole normal SDK before replay")
            self.save("sdk-before.json", self.baselines["sdk"])
            artifacts = d.loads(c.read(normal.out / "current-artifacts.json"))
            trace, trace_row = normal.select(c.read(normal.out / "current-trace-build.stdout"), "controlsd_trace", "example",
                "rust/crates/controlsd/examples/controlsd_trace.rs", ["native"], "controlsd")
            need(trace_row == artifacts["trace"] and str(trace) in context["normal_holds"], "same actual normal native ELF")
            execution = context["normal_execution"]
            need(execution["seconds"] == 1200 and execution["native_timeout_seconds"] == 600
                 and execution["auditor_timeout_seconds"] == 700, "unchanged normal command bounds")
            # 원래 Rust CommandSpec은 상속 환경에 정확한 3값만 겹친다.
            native_env = {**execution["environment"], "PATH": "/nonexistent",
                "CONTROLS_NUMERICS": str(normal.numerics),
                "CONTROLS_ASSETS": str(self.root / "opendbc_repo/opendbc/car/torque_data")}
            # 큰 full40 입력/출력 네 개를 동시에 유지하지 않는다. 선택 후 전체 객체는 해제한다.
            full = d.loads(c.read(normal.out / "native/input.json",128*MIB))["cases"]
            need(len(full)==40,"actual native input40")
            inputs = full[:4]
            del full
            full = d.loads(c.read(normal.out / "reference/loop/source.json",128*MIB))
            need(len(full)==40,"frozen output40")
            old_state, old_name = full[3]["rows"][0]["state"], full[3]["name"]
            del full
            full = d.loads(c.read(normal.out / "native/native-raw.json",32*MIB))
            need(len(full)==40,"actual native output40")
            actual_state, actual_name = full[3]["rows"][0]["state"], full[3]["name"]
            del full
            full = d.loads(c.read(normal.out / "reference/loop/input.json",128*MIB))["cases"]
            need(len(full)==40 and inputs[3]==full[3]
                 and inputs[3]["name"]=="HYUNDAI_PALISADE-torque" and len(inputs[3]["frames"])==200,
                 "exact current/frozen case3/frame0 inputs")
            del full
            need([float(v) for v in context["first_failure"]] == [old_state["lat_pid"][0], actual_state["lat_pid"][0]],
                 "first stderr mismatch and actual outputs agree")
            self.request = self.out / "case3-frame0-input.json"
            c.write_new(self.request, (json.dumps({"cases": [{**inputs[3], "frames": inputs[3]["frames"][:1]}]}, allow_nan=False)+"\n").encode())
            prefix = [*inputs[:3], {**inputs[3], "frames": inputs[3]["frames"][:1]}]
            need([len(x["frames"]) for x in prefix] == [200,200,200,1], "exact original native prefix600/target1")
            prefix_path = self.out / "native-prefix600-target1-input.json"
            c.write_new(prefix_path, (json.dumps({"cases": prefix}, allow_nan=False)+"\n").encode())
            for path in (self.request, prefix_path):
                self.holds.add(path)
            replay = {}
            for label, request in (("same-native-oneframe",self.request),("same-native-prefix",prefix_path)):
                self.phase = label
                output = self.out / (label+".json")
                with request.open("rb") as stream:
                    need(c.ident(os.fstat(stream.fileno())) == self.holds.records()[str(request)]["identity"], "held replay input FD")
                    self.command([trace,output],label,seconds=600,env=native_env,stdin=stream)
                self.holds.add(output)
                replay[label] = d.loads(c.read(output,16*MIB))
            need(len(replay["same-native-oneframe"])==1
                 and len(replay["same-native-oneframe"][0]["rows"])==1
                 and [len(x["rows"]) for x in replay["same-native-prefix"]]==[200,200,200,1], "replay exact case/frame roster")
            for rows, cases in ((replay["same-native-oneframe"],[inputs[3]]),
                                (replay["same-native-prefix"],prefix)):
                need(all(isinstance(row,dict) and set(row)=={"name","printed","rows"} for row in rows)
                     and [row["name"] for row in rows]==[case["name"] for case in cases],
                     "original/current replay case names and output schema")
            need(old_name==actual_name==inputs[3]["name"], "frozen/current case3 identity")
            states = {"frozen":old_state,"actual_full_native":actual_state,
                "same_native_oneframe":replay["same-native-oneframe"][0]["rows"][0]["state"],
                "same_native_prefix":replay["same-native-prefix"][3]["rows"][0]["state"]}
            reproduced = d.target_reproduced(actual_state, states["same_native_oneframe"], states["same_native_prefix"])
            self.result = {"schema":1,"source":self.source,"same_vm":True,"actual_identity":context["identity"],
                "same_native_elf":{k:context["normal_holds"][str(trace)][k] for k in ("identity","sha256")},
                "native_path":str(trace),"native_environment":native_env,"native_argv_change":"output path only; bounded input subset/prefix explicitly recorded",
                "normal_failure_preserved":True,"normal_pass":False,"state_bits":{k:d.bits(v) for k,v in states.items()},
                "typed_equalities":{a+"__"+b:d.bits(states[a])==d.bits(states[b]) for a in states for b in states if a<b},
                "reproduction_required_for_diagnostic_completion":False,
                "actual_failure_reproduced":reproduced,"current_original_observed":False,
                "scalar_layer_observed":False,"inference_stopped":not reproduced,**d.FLAGS}
            self.save("same-job-triangulation.json",self.result)
            if not reproduced:
                # 원본은 실행하지 않았다. 비어 있는 owned 준비영역/기존 Python root도 finalizer에 봉인한다.
                for label, root in (("originaldeps",self.deps),("packages",self.packages),("numerics",self.numerics),("python-provider",self.python_prefix)):
                    self.baselines[label] = self.snapshot(root)
                    self.save(label+"-before.json",self.baselines[label])
                self.result["stop_reason"] = "same-held plain one-frame/prefix did not reproduce actual fullnormal state"
                self.result["unused_original_provider_baseline_scope"] = "after plain replay, before finalizers; original not executed"
                self.save("same-job-triangulation.json",self.result)
                self.phase = "same-job-observed"
                return
            # 실제 실패의 상태가 달라진 replay로 layer 원인을 설명하는 우회를 거부한다.
            foreign = dict(states["same_native_oneframe"], _foreign=True)
            need(not d.target_reproduced(actual_state, foreign, states["same_native_prefix"])
                 and not d.target_reproduced(actual_state, states["same_native_oneframe"], foreign),
                 "target reproduction mutation controls")
            self.result["reproduction_negative_controls"] = 2
            self.save("same-job-triangulation.json",self.result)
            self.holds.verify()
            c.copy_new(normal.out / "numerics-wheel.stdout", self.packages / d.NUMPY_WHEEL, 32*MIB)
            self.prepare_python()
            for label, root in (("originaldeps",self.deps),("packages",self.packages),("numerics",self.numerics),("python-provider",self.python_prefix)):
                self.baselines[label] = self.snapshot(root)
                self.save(label+"-before.json",self.baselines[label])
            config = self.out / "worker-config.json"
            c.save(config,{"root":str(self.root),"source":self.source,"site":str(self.site),"input":str(self.request),"evidence":str(self.out)})
            self.holds.add(config)
            self.phase = "same-job-current-original"
            self.command([self.python,"-I","-B","-S",self.root / DIAGNOSTIC,"--worker","original-plain","--config",config],
                "original-plain",seconds=700,env={**self.env,"PATH":"/nonexistent","LD_LIBRARY_PATH":str(self.python_lib)})
            for suffix in (".json","-seals.json","-providers-before.json","-providers-after.json","-numpy-dispatch.json"):
                self.holds.add(self.out / ("original-plain"+suffix))
            worker = d.loads(c.read(self.out / "original-plain-seals.json"))
            need(worker["pass"] and worker["errors"] == [] and worker["worker_error"] is None, "current original worker seals")
            original = d.loads(c.read(self.out / "original-plain.json",16*MIB))
            need(len(original)==1 and set(original[0])=={"name","printed","rows"}
                 and original[0]["name"]==inputs[3]["name"] and len(original[0]["rows"])==1,
                 "target original exact one-frame output")
            states["current_original"] = original[0]["rows"][0]["state"]
            self.result.update(current_original_observed=True,
                state_bits={k:d.bits(v) for k,v in states.items()},
                typed_equalities={a+"__"+b:d.bits(states[a])==d.bits(states[b]) for a in states for b in states if a<b})
            self.save("same-job-triangulation.json",self.result)
            observation = self.observe_target_failure(replay["same-native-oneframe"], original, native_env)
            self.result.update(scalar_layer_observed=True, target_observation=observation,
                               reproduction_negative_controls=2)
            self.save("same-job-triangulation.json",self.result)
            self.phase = "same-job-observed"

        def finish(self,error):
            if self.baseline is not None:
                try:
                    need(self.snapshot(normal.out)==self.baseline,"normal proof changed during replay")
                    normal.holds.verify()
                    self.save("normal-proof-after.json",self.baseline)
                except BaseException:
                    error = (error or "") + traceback.format_exc()
            return super().finish(error)
    return Replay()


if __name__ == "__main__":
    raise SystemExit(main())
