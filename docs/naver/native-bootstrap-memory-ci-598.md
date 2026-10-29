# #598 Genuine bootstrap의 CI 메모리 상한과 읽기 진단

## 실제 실패와 해석 범위

기준 source는 `c7bd7b6a514e122a6cd6ea8bba957f3ae8709297`이다.
이전 source `9b8c6337b56b80b79788ebc389b6622b484fe0c7`의
[Genuine 40904508393/job123556239177](https://github.com/bin9208/openpilot-rust/actions/runs/40904508393/job/123556239177)는
2026-10-29 18:45:53 UTC에 실패했다. 부모가 digest 검증한 artifact
`12924545511`의 `resources.jsonl` 2,103행과 stderr를 읽었다.
worker는 약 2,281초에 private cgroup의 정확한 3GiB 한도에 도달했다.
마지막 peak는 `3221225472`, current는 `3009732608` bytes이며
`private native build memory limit reached; normal runtime retained`를 기록했다.
`limit_events=5`는 `max/oom/oom_kill` 합계라 OOM kill 5회를 의미하지 않는다.
`build.log`는 0 bytes이고 `worker-result.json`이 없어 정확한 준비·컴파일 단계와
anon/file 비율, OOM kill 발생 여부는 확인하지 못했다. 압축 해제·복제·page cache·
컴파일 중 하나를 원인으로 단정하지 않는다.

원격 실패 기록의 system MemAvailable은 시작 약 8.64GiB, 누적 최저 약 5.17GiB다.
이와 별도로 감사 도구를 실행한 로컬 데스크톱의 순간 MemAvailable 약 5.78GiB를
읽었으나, 이는 bin2/bin3의 실시간 관측이 아니다. 원격 현재 여유로 사용하지 않는다.

## 변경

`tools/ci/verify_native_bootstrap.py`가 만드는 CI fixture의
`maximum_memory_bytes`만 3GiB에서 6GiB로 조정한다. 실측된 3GiB 상한 도달을
근거로 한 보수적인 다음 검증 상한이며, 검증된 필요량·성공 보장·물리 ARM 또는
실제 기기 예산을 뜻하지 않는다. 128MiB minimum margin과 기존 진입·반복 guard를
유지하므로 시작에는 host/ancestor 양쪽에서 6.125GiB headroom이 필요하다.
진입 검사는 메모리를 예약하지 않으며 다른 runner의 동시 사용으로 중단될 수 있다.

동일 CI helper의 실제 `--bootstrap-tools` 네 호출(initial/reused/docs/small) 동안만
1초 간격의 daemon observer를 둔다. 일반 Git 명령과 `--help`에는 실행하지 않는다.

- 기존 job owner와 root setup의 lease(schema/uid/gid/source/inode)를 대조한다.
  job/work/attempt/cgroup은 canonical·현재 uid 소유의 실제 디렉터리만 허용한다.
- 호출 전 존재한 attempt를 제외하고, 새 attempt 첫 `resources.jsonl` 행만 읽어
  config pin과 parent를 대조한다. 검증한 parent 바로 아래의 `build-*`만 읽고
  inode를 계속 확인한다. journal 전체 반복 읽기와 cgroup 전체 탐색은 하지 않는다.
- `memory.events` 원문, `memory.stat`의 anon/file/shmem/kernel,
  current/peak/max, host MemAvailable을 기록한다. shmem은 file에 포함될 수 있으므로
  이 필드들을 더해 총량을 만들지 않는다.
- 직접 `cgroup.procs`에서 최대 64개 PID만 조사하고 `/proc/<pid>/cgroup` 소속 확인 후
  `comm`만 남긴다. 전체 cmdline·환경 변수·관련 없는 프로세스는 수집하지 않는다.
- Rust receipt와 Cargo set tree receipt, cargo-build 디렉터리와 worker-result의
  존재 여부만 관측한다. receipt 내용·트리·해시·파일 본문을 검증하거나 복사하지 않는다.
  prefix의 바로 아래 항목을 최대 8개만 확인하며 완료 단계의 증명으로 해석하지 않는다.
- 입력 파일당 최대 16KiB, attempt 바로 아래 최대 64개 항목만 읽는다.
  syscall은 read-only/nonblocking이며 `O_NOFOLLOW`를 사용한다.
- subprocess 종료/예외 모두 `finally`에서 stop을 요청하고 최대 3초 join한다.
  daemon이므로 별도 자식 프로세스가 남지 않으며 PID loop도 stop에 반응한다.
  진단 오류·일시적인 journal/cgroup 생성·삭제 경합은 명시적으로 JSONL 또는 stderr에
  기록하되 기존 subprocess 반환 코드와 guard 판정은 그대로 사용한다.

파일은 `bootstrap-current-evidence/<phase>.json.memory.jsonl`에 기록하여 기존
`always()` artifact의 `bootstrap-current-evidence/**`에 포함된다.
1초보다 짧은 재사용 호출이나 마지막 순간의 spike를 놓칠 수 있으며, 카운터별 읽기가
원자적인 동시 snapshot도 아니다. 누락을 성공으로 해석하지 않는다.

## 보존과 검증

production `resources.rs`/`memory_cgroup.rs`, SDK/source/Git/ABI, owner,
13개 도구 inventory, initial/reused/docs/small의 기존 단언과 normal active/log 보존
단언, 원래 bootstrap argv와 artifact workflow는 변경하지 않는다.
로컬에서는 Python AST와 기준 source 대비 diff/단언 보존만 확인한다.
기능·build·test·실제 cgroup/AGNOS 실행은 하지 않는다. 새 정확 source의 메모리 필요량,
진단 수집 성공과 전체 bootstrap 성공은 부모의 Actions 검증에서 확인해야 한다.
기존 c7 Genuine 실행은 취소하거나 재시작하지 않는다. push·CI 조작·원격 글은 수행하지 않는다.
