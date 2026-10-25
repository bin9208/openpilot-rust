# #554 logging private Git untracked cache 후보

## 근거와 현재 상태

2026-10-25. #523 원본 보존 후보의 후속이며 credit0이다. 이번 변경으로 2초 기준 충족이나
root 해결을 주장하지 않는다. 기능·설치·원본 실행은 Actions 전용이며 현재 구현 후보는 UNRUN이다.

6a logging [40151446356/job121043653993](https://github.com/bin9208/openpilot-rust/actions/runs/40151446356/job/121043653993)의
원문 artifact12582772050은 33,846,507B,
SHA `67e6020444172f624ac438fa6a46fbd3bc15c8e9cd313d6fde3540863e4dcb20`다.
부모의 2686 CRC 전수 감사를 재사용하고 기존 로컬 ZIP의 필요한 member를 inert하게 읽었다.
`outcome.json`은 `lifecycle-runtime_directory_int`/exit101/held_final_exit_code0이며
실제 stderr는 `backpressure send and shutdown exceeded bound`다.
직전 first_int/first_term/first_eof와 preload4상태·환경음성1/core21/prepare9/trace20/
summary6/Clippy 성공은 해당6a 기록이고 새 candidate 성공으로 소급하지 않는다.

해당 trace에는 Git22개/status11개가 있다. status 하나의 syscall은 newfstatat22215,
getdents645430/openat3692 등이며, ptrace 아래 untracked directory 탐색 비용이 크다.
부모가 분리한 start2.975207670s/command0.18298s/finish0.107128s/total3.265316554s와
엄격2초 경계는 유지한다. 개별 Git trace의 wall 구간과 이 application 단계 측정은 같은 값이 아니다.
`core.preloadIndex=false`만으로 이6a 경계는 통과하지 못했다.

## 공식 계약

- [git-update-index UNTRACKED CACHE](https://git-scm.com/docs/git-update-index#_untracked_cache):
  directory mtime를 사용하는 cache이며, `--test-untracked-cache`로 현재 filesystem을 먼저 검사한다.
  성공 뒤 `--untracked-cache`로 index extension을 켠다. config를 명시false로 둔 채 강제하지 않는다.
- [git-status 성능](https://git-scm.com/docs/git-status#_untracked_files_and_performance) 및
  [GIT_OPTIONAL_LOCKS](https://git-scm.com/docs/git#Documentation/git.txt-codeGITOPTIONALLOCKScode):
  status가 cache를 index에 저장할 준비 단계와 runtime의 선택적 쓰기 억제를 구분한다.

runtime의 `GIT_OPTIONAL_LOCKS=0` 및 기존 `GIT_CONFIG_COUNT=1`,
`GIT_CONFIG_KEY_0=core.preloadIndex`, `GIT_CONFIG_VALUE_0=false`는 그대로다.
새 runtime 환경키, 사용자/global Git config 변경, assume-unchanged/skip-worktree,
ctime 무시, fsmonitor 또는 fake Git은 추가하지 않는다.

위 optional-locks0은 helper의 바깥 runtime 배열이다. 동결 fixture의
`fixed_git_preload_environment`는 env_clear 뒤 PATH/HOME/TMPDIR/TZ/LANG/LC_ALL과 Git config3키만
전달하므로 fixture의 checker 자식에는 optional-locks0이 전달되지 않는다. 이 기존 동작은 수정하지
않고, source prime 전에 이 정확한 자식 환경에서도 `core.untrackedCache`가 unset인지 별도 조회한다.
fixture 자식의 index 쓰기 억제나 전체 config/provider closure를 수용했다고 주장하지 않는다.

## 구현 계획과 경계

아래 완료 표시는 source 연결 완료이며 실제 실행 성공을 뜻하지 않는다.

- [x] 기존 preload2048파일/동일blob/4상태 및 기존 환경음성의 본문을 그대로 뒀다.
- [x] 별도 fresh0700/16MiB private Git fixture에서 filesystem 검사와 cache enable/prime을 연결했다.
  clean, tracked content, tracked executable mode, untracked add/delete,
  nested directory add/rename/remove의8상태를 직접 작성한 porcelain 기대값 및 cache off/on과 대조한다.
  각 stdout/stderr/exit를 보존한다. fixture prime은 다음 mutation의 cache invalidation을 검사하기 위해
  상태별로 저장하며, 실제 source에서 단 한 번 prime하는 경계와 구분한다.
- [x] 모든 Cargo/Clippy/기존 빠른 제어 뒤, lifecycle trace 시작 전에 source cache 준비를 연결했다.
  source owner marker/devino/canonical path와 실제 `.git`/index 소유·type·device·nlink를 확인한다.
  실제 runtime 환경에서 `core.untrackedCache`가 unset(exit1+빈 channels)인지 확인한다.
  명시false/true 또는 config 오류는 실패하며 config를 고쳐 진행하지 않는다.
- [x] source에서는 filesystem 검사→`update-index --untracked-cache`→단 한 번
  `GIT_OPTIONAL_LOCKS=1 git status --porcelain --untracked-files=normal`만 수행한다.
  앞선 cachefalse status와 prime/이후 readonly status의 stdout/stderr/exit를 정확 비교하고 clean을 요구한다.
  status/dirty 판단은 skip하거나 재해석하지 않는다.
- [x] raw index 전후 원문·SHA·metadata를 별도 proof에 보존하도록 연결했다. index stat/cache 갱신은 mutable metadata이며
  raw index 전체가 불변이라고 주장하지 않는다. tracked file list와 mode/OID/stage 및 flags,
  전체 tracked source tar SHA/HEAD/config/held 파일과 전체 prepared SDK hash/roster는 다시 확인한다.
- [x] 기존 runtime14+retained1 trace, 2초·600초, packet/원문/expected, raw/proof/growth 상한,
  failure source/SDK seal/upload/cleanup을 유지하고 WF의 helper pin과 새 receipt 검사를 연결한다.

source Git cache 준비를 안전하게 검증하지 못하거나 filesystem test/config/status/봉인 하나라도
실패하면 candidate를 실패시킨다. cacheoff 재실행으로 성능 gate를 우회하거나 일반 config를 덮어쓰지 않는다.
실제 cache hit·syscall 감소 및2초 충족은 새 Actions trace로만 판단한다.

## 검증 기록

로컬에서 helper와 Git heredoc3개 Bash 문법, WF YAML 및 run9개 Bash 문법,
scoped `git diff --check`를 확인했다. 새 두 삽입 구간만 제거하면 기존 helper 전체가 HEAD와
byte 동일하며 runtime 배열·기존 preload4상태·lifecycle 이후 전 구간도 그대로다.
WF는 helper pin/새 receipt2개와 이 문서 trigger path를 제외한 전체 parsed model이 HEAD와 같다.
별도 fixture8상태, source prime, 기존 lifecycle/성능/봉인 기능 결과는 모두 Actions 대기다.
기존 Rust fixture/17본문/env3keys와 원본 source/model/기대값은 이번 편집 범위가 아니다.
동결 fixture `tests/native_logging_lifecycle/retained.rs`의 SHA는
`b3f04c6400679d0d47196c5046c37c849fb5d7505642bf80524b6908f224aadb`로 불변이다.

새 fixture는 기존 `run()`/timeout 아래120초, source 준비는240초다. 기존60분 helper/75분 job,
95초 case/600초 shared lifecycle과2초 본문 기준은 그대로다. 새 fixture16MiB는 기존 proof1GiB 안에
포함되며512MiB reserve와 기존 build8GiB cap은 그대로다. source2GiB 확인에서는 별도로 감시하는
정확한 `source/rust/target`만 제외하고 `.git`/index cache는 포함한다.

주요 새 원문은 `logging/git-cache-554/`의8상태 off/on/prime stdout·stderr·exit와
`logging/git-source-cache-554/`의 config 조회·filesystem 검사·cache-off/prime/cache-on channels,
raw index before/primed/after `.bin`·SHA/identity `.tsv`, tracked roster/flags/source SHA/SDK roster다.
source mutation 뒤 실패하면 `index-failed` 관측을 시도하며 원래 실패를 유지한다.
기존 helper EXIT의 held 검사와 WF always source/SDK finalizer·failure upload는 그대로다.
