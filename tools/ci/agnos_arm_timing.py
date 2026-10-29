#!/usr/bin/env python3
"""Record timing for the complete selected AGNOS workflow on a hosted ARM VM."""
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import shutil
import sys
import time


def main():
    assert os.environ['GITHUB_ACTIONS'] == 'true'
    assert os.environ['RUNNER_ENVIRONMENT'] == 'github-hosted'
    assert os.environ['RUNNER_ARCH'] == 'ARM64' and platform.machine() == 'aarch64'
    root = Path(os.environ['CI_JOB_TEMP'])
    path = root / 'agnos-arm-timing.json'
    if sys.argv[1] == 'start':
        cpus = sorted(os.sched_getaffinity(0))
        assert len(cpus) > 1, 'User prohibits one-job execution'
        report = dict(source_commit=os.environ['GITHUB_SHA'],
                      selected_branch='codex/feat-599-complete-runtime-inputs',
                      selected_branch_base='2375949c19c63667ad66de0f9ee7e2c90ad8ba70',
                      baseline_run=40961936699,
                      runner=os.environ['RUNNER_NAME'], allowed_cpus=cpus,
                      started_utc=datetime.now(timezone.utc).isoformat(),
                      monotonic_start=time.monotonic(), initial_free_bytes=shutil.disk_usage(root).free,
                      execution_mode='native ARM / PRoot; no QEMU')
    else:
        assert sys.argv[1] == 'finish'
        report = json.loads(path.read_text())
        report['elapsed_seconds'] = time.monotonic() - report['monotonic_start']
        report['finished_utc'] = datetime.now(timezone.utc).isoformat()
        report['final_free_bytes'] = shutil.disk_usage(root).free
        evidence_path = root / 'native-agnos-environment-599.json'
        if evidence_path.exists():
            evidence = json.loads(evidence_path.read_text())
            report['agnos_execution_seconds'] = evidence['elapsed_seconds']
            report['original_workflow_success'] = evidence['success']
            report['failure'] = evidence.get('failure')
            for key in ('proot_source_commit', 'proot_patch_sha256', 'proot_sha256'):
                if key in evidence:
                    report[key] = evidence[key]
            report['completed_native_phases'] = sorted(k for k in evidence if k.startswith('native_'))
            report['commands'] = [dict(elapsed_seconds=c['elapsed_seconds'], exit_code=c['exit_code'],
                                       argv=c['argv']) for c in evidence['commands']]
        else:
            report['original_workflow_success'] = False
            report['failure'] = 'Preparation did not reach the AGNOS execution report'
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as stream:
            stream.write('## 선택한 작업 브랜치의 전체 AGNOS workflow 측정\n\n')
            stream.write(f"- 소스: `{report['source_commit']}`\n")
            stream.write(f"- CPU: `{report['allowed_cpus']}`\n")
            stream.write(f"- 준비부터 원본 증거 업로드까지: {report['elapsed_seconds']:.3f}초\n")
            stream.write(f"- 원본 AGNOS 실행 성공: `{report['original_workflow_success']}`\n")
            if 'agnos_execution_seconds' in report:
                stream.write(f"- AGNOS 내부 전체 명령 구간: {report['agnos_execution_seconds']:.3f}초\n")
            if report['failure']:
                stream.write(f"- 실패: `{report['failure']}`\n")
    path.write_text(json.dumps(report, indent=2) + '\n')
    print('AGNOS_ARM_TIMING ' + json.dumps(report), flush=True)


if __name__ == '__main__':
    main()
