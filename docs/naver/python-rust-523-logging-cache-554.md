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

## fd 실제 결과와 비계측 단일 관측 A

`fd3f3d02293ff4ce71b40733fa7c46c9ae9e7d1c`의
[run40153172264](https://github.com/bin9208/openpilot-rust/actions/runs/40153172264),
artifact12583985147(27218198B/SHA
`c7e21a6789e9ead6b627717d0a9be68c50c640e6ef89cffd2deafce8c625b3db`)을 부모가
2841CRC/expanded338715688B로 감사했다. source prime1/FS/8상태/status channels/entries flags/
config/SDK와 first lifecycle3은 실제 성공했다. 그러나 runtime_directory_int는 다시2초 경계에
도달하지 못했다. 원문 start_peer2.117255848/command0.181615731/finish0.106551884/
total2.405423463초이며, outcome은 해당 phase/exit101/held_final_exit_code0/credit0이다.
prime 성공을 성능 해결이나 전체 lifecycle 수용으로 확대하지 않는다.

반복 Git status는 runtime-version 또는 logger가 아니라 checker의 Peer::start에 있다.
원래 보존 Python checker도 매 peer에서 HEAD/status를 계산하며 기존2초 timer에 포함한다.
이번 구현은 관측을 생략하거나 timer 밖으로 옮기지 않는다. preload값과 config schema도 불변이다.

승인된 A는 **기존 shared600초 시작 직후** 같은 fresh held fixture의
`--ignored --exact retained::runtime_directory_int --test-threads=1 --color=never`를
비계측으로 정확1회 추가한다. 새 mode/옵션/WF는 없다. bound는 관측 직전 남은 shared 시간과
95초 중 작은 값이며, 이후 원래 traced14-selector loop 본문/순서/명령을 그대로 실행한다.
기존17case 정의와2초 assertion 및 그 뒤 retained-once/trace 검사도 바꾸지 않는다.

비계측 env는 기존 runtime 전체와 lifecycle env6개를 복사하고 evidence root 한 값만
`logging/paired-554/untraced/evidence`로 바꾼다. 기존 traced evidence는 `logging/lifecycle`이다.
fixture/checker/probe/context는 새로 빌드한 같은 파일과 기존 FD hold를 쓴다. 원문 ELF는 실행하지
않으며 source/config 변경·Git status 추가 실험·preload true 비교를 넣지 않는다.

`logging/paired-554/{untraced,traced}`에는 실제 command argv 및 명시 env 배열을 NUL 구분으로
기록한다. arbitrary 환경 dump나 인증값을 수집하지 않는다. 각 side 직전/직후에는 다음을 보존한다.

- initial source HEAD와 기존 tracked tar SHA, files/mode·OID·stage/flags roster를 기존 baseline과 대조
- 새 config FD hold 및 config identity/hash, 동일 fixture/checker/probe/context identity/hash 확인
- raw index를 각 시점의 열린 FD에서 안정적으로 복사한 `.bin`/identity/SHA 기록
- 명시 env 원문 before/after와 command exit/bound, 기존 stdout/stderr 및 실제 fixture evidence

raw index의 before/after bytes를 같다고 강제하지 않고 immutable이라고 표시하지 않는다.
logical entries/flags/config/source·held/provider guard는 그대로 필요하다. 부모의 기존 최종
whole-source/index/SDK gate를 대체하거나 그 baseline 불일치를 허용하는 관측이 아니다.

비계측 실행은 `run()`를 if 조건으로 호출하지 않는다. parent의 errexit를 잠시 끈 뒤
자식 subshell에서는 errexit와 기존 file512MiB 제한을 켜고 timeout 한 명령을 실행한다.
실제 status를 즉시 저장하고 parent errexit를 복원한 **후** metadata/resource 검사를 수행한다.
따라서 마지막 admit 성공이 command 실패를 지울 수 없다. 일반 nonzero는 보존한 채 traced loop에
진입하지만 최종 실패로 남는다. timeout124/launcher125–127/128이상 signal은 즉시 전파하고,
held/source/config/entries/flags/resource 실패도 그대로 실패한다. exit0은 정확1pass summary가 필요하다.
traced 검사 자체의 기존 첫 실패 중단 동작은 바꾸지 않는다.

traced runtime_directory_int에도 같은 전후 관측과 실제 command status 보존만 연결했다.
어느 side에서 command/guard가 실패해도 EXIT가 독립 failed-after snapshot을 시도하며 원래 실패를
성공으로 바꾸지 않는다. `outcome.json`에 `untraced_exit_code`와 `paired_final_exit_code`를 추가하고,
나머지 모든 단계가 성공해도 비계측 실패가 있으면 최종 phase는 `lifecycle-untraced-failed`다.
retirement_credit0/closed_trace_admittedfalse/old27_admittedfalse 및 R4 미수용은 유지한다.

추가 기록도 기존 source2GiB/build8GiB/proof1GiB/file512MiB와 실제 filesystem admission 안에 둔다.
전후 snapshot 시간도 shared600초에 포함되며 남은 시간이 없으면 실패한다. 부모의 source/SDK always
finalizer/upload/소유 cleanup, helper60분/job75분과 기존 caps는 불변이다.

이번 A 연결은 로컬 Bash/YAML 문법과 source/diff 비교만 확인한다. 상태 분기·회수·실제2초와
600초 실행 결과는 **Actions UNRUN**이다. 비계측 PASS만으로 traced gate/old27/R4/퇴역을
수용하지 않는다. source timing/metadata 관측 자체가 cache/order/load에 영향을 줄 수 있다는 한계도
보존하며 성공할 때까지 반복하지 않는다.
