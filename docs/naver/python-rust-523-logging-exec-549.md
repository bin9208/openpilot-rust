# #549 current15 exact exec 정책 연결 — source 후보 / GHA UNRUN

2026-10-25. 기준 HEAD `7b277494164da62b8c1971019ad9043de86b864a`다.
원본2/함수·fixture·trace·shared workflow를 실행하지 않고 현재 source와 Git object만 읽었다.
아래 첫 체크포인트 당시에는 이 기록 외 tracked 편집0이었고, 정책 jq/helper 구현 전에
summary 정보 손실을 보고했다. 이후 사용자 승인으로 trace/summary tests까지 소유 범위를 확대해
마지막 절의 제한 구현을 추가했다. 아래 과거 체크포인트를 현재 미구현 상태로 해석하지 않는다.
원본 삭제·caller 전환·old27/R4/퇴역 credit은0이다.

## 첫 체크포인트: 기존 연결을 그대로 구현할 수 없던 이유

`tools/ci/logging_retained_549.sh`의 기존15 prefix는 모두
`check-native-logging inspect-trace-summary`를 호출한다.
`native_logging_check/trace.rs::EventSummary::observe`는 전체 줄을 같은 strict lexer로 읽지만,
정상 syscall은 exec를 제외하고 `event_counts[kind:operation:outcome]` 합계에만 남긴다.
`.events`에 보존하는 조건은 `row.exec.is_some() || row.kind != "syscall"`다.
따라서 clone/clone3/fork/vfork/setsid/setpgid 등의 성공·실패는 opcode 전체 합계만 있고
**정상 호출의 PID·child result·해당 exec 전후 line 연결은 summary에서 없어졌다.**
signal/terminal/aborted 및 exec의 PID와 원문 span은 보존된다.

반면 기존 collision8은 `inspect-trace` schema1의 모든 syscall Event를 사용한다.
`logging_collision8_549.sh`의 exact policy는 각 checker PID의 모든 Event를 골라
fork/vfork/clone/clone3 및 session 작업이 없음을 검증한다.
이 필터를 current15 schema2의 축약 `.events`에 그대로 적용하면
**누락된 spawn을 조회한 빈 집합의 all()이 true가 되는 false acceptance**다.
fixture의 실제 pidfd/reap assertion이 있는 case도 이 정보 손실을 복원하지 않는다.
특히 output5/금지옵션3 logical cases의 checker child0은 fixture barrier 부재만으로 증명할 수 없다.

| 요구 연결 | 현재 summary에서 가능한 것 | 가능한 것처럼 쓰면 안 되는 것 |
| --- | --- | --- |
| 모든 exec attempt의 byte program/argv/env·result/count | `.exec` 전체와 source span이 남아 있음 | program basename allowlist 또는 성공 exec만 선택해 실패 attempt를 제외 |
| root/checker/producer terminal | terminal PID/result가 남음 | terminal만으로 어떤 parent가 만든 child인지 또는 실제 wait/reap 완료 주장 |
| checker child0/session0 | global opcode 합계만 남음 | fixture/Git/ZMQ worker의 생성과 checker 생성의 합계를 분리한 것처럼 해석 |
| 실제 fixture PIDfd/peer FD/escaped reap 연결 | 기존 retained receipt의 actual PID/UID/start/parent/exe·FD 관측과 assertion 존재 | 관측하지 않은 PID/cwd/FD/DSO 전체 closed flags를 true로 변경 |

새 policy가 exec 일부만 검증할 수는 있지만 이를 요청한 terminal/spawn/case 결합 완료 gate로
표기하면 안 된다. 이 차이를 숨긴 jq나 항상 unsupported로 끝나는 실제 gate는 추가하지 않았다.

## 첫 체크포인트에서 제안한 가장 작은 source 보완

범용 PID/cwd/FD/DSO framework나 새 decoder는 필요하지 않다.
기존 strict decoder가 만든 Event 중 **case별 spawn/session 연결에 필요한 고정 구조 syscall만**
summary의 상세 `.events`에 추가 보존하는 제한 delta를 먼저 심사하는 것이 가장 작다.
기존 모든 raw/SHA/줄 검사·opcode 집계·64MiB report/1GiB prefix/512MiB file 상한을 그대로 둔다.
실제 case에서 사용되는 clone/clone3/fork/vfork/setsid/setpgid/prctl/unshare/setns 및
필요한 wait result의 정확 source 범위를 확인해 필요한 것만 고정한다.
fd/cwd/mmap의 범용 상태기계나 metadata 호출 전체를 상세 출력하지 않는다.
summary의 출력 scope 설명과 의미 있는 소형 음성도 함께 필요하며,
기존 full parser/원문/unknown 거부와20+4/summary6의 의미를 완화하지 않는다.

이 보완 파일 `src/native_logging_check/trace.rs`와 summary tests는 현재 worker 편집 범위 밖이다.
현재 허용한 jq/helper/docs 범위를 넘어 임의 편집하지 않았다.
full mode로 current15를 일괄 바꾸는 안도 기존 대형 metadata Event의64MiB 초과 문제 때문에
대안으로 채택하지 않는다. raw grep/정규식으로 새 syscall decoder를 만들거나
프로그램명만 허용하는 방식도 쓰지 않는다.

필요한 구조 Event가 보존되면 새 `logging_retained_exec_policy_549.jq`는
기존 collision의 byte exactexec 함수를 재사용해 source-authored argv/env와
held canonical checker/fixture/probe/Git 경로, case별 producer invocation·PIDfd 관측·terminal을
결합한다. dynamic root/endpoint/FD는 별도 receipt의 현재 생성값과 고정 source 규칙에 bind하고,
raw exec 자체를 expected argv로 복사하지 않는다. 미관측 branch/모순은 unsupported FAIL이며
closed_pid/cwd/fd/dso/full_acceptance/R4/old27/credit은 그대로 false다.
실제 current15와 GHA 양음성 수용 전에는 정책 연결 성공을 주장하지 않는다.

## historical17/pending runtime2의 작은 source bridge

historical SOURCE는 `40eb0429013cd853c1e74dc9530cdb5f484aa32c`다.
이번에 해당 Git object와 현재 source를 직접 byte 비교했다. 이 표는 **source 사실**이며
과거 provider/feature/DSO/실행 context를 current PASS로 승인한 표가 아니다.

| 범위 | 직접 비교 결과와 원래 assertion | 수용 경계 |
| --- | --- | --- |
| `native_logging_check/lifecycle.rs` | 전체 byte 동일, SHA `3886122ed4353d1a83bc49f3d2b8567c2e4cfae1ab744e7b1980ee4271b53622` | 동일 cleanup 소스만 입증. 현재 ELF/SDK/실제 descendant 수용은 별도 |
| `native_logging_check/socket.rs` | 전체 byte 동일, SHA `4ec498aa34bfaf41bd9b5721c5f1afd23bbdd021aba2b69e95421a838c5351ea` | receiver/foreign path 보존 소스 근거. 범용 FD closure 아님 |
| 공통 Peer/packet | `struct Peer {` 직후부터 normalize 설명 comment 전까지10,125B 동일, SHA `71da79763a34ddc4b549dbcf41e77f5feb58438d07a88dd84eda8527f4d83993` | start/실제 command·receive·finish/packet 검증의 직접 carry 근거 |
| legacy run_cli | `fn run_cli(` 직후부터 cfg(test) 전까지3,787B 동일, SHA `6c0d4ecc8cb5598b8870f355f3b5bce6240bb7e1b36976fcb6f6ac8645898d66` | occupied-output가 provider open/build 전 거부되는 분기와 legacy 실제 비교 유지 |
| old EOF3 test | 함수 구간2,709B 동일 | peer KILL/release→checker1, owned/private receiver 제거·foreign 보존·reap/report 없음을 원래대로 유지 |
| old signal14 test | 추가는 RESUME 변수 및 runtime/directory만 남기는 filter. 실제 signal/reap/foreign assertion 본문 유지 | RESUME는 selection이며 reuse proof가 아님. historical17의 current provider binding은 아직 별도 |
| old collision10 test | RESUME filter와 live socket leaf `foreign-listener`→`s` 변경. 나머지 output/external snapshot·exit1·stdout empty/barrier 부재 유지 | historical no-reference file/directory2는 live leaf 변경 미도달. current collision8 성공은 부모 source593 감사의8개에만 귀속 |

historical17은 old C++ first3 + runtime12 + occupied-output2다.
current retained first3은 native-first이므로 old C++ first3으로 이름만 바꿔 대입하지 않는다.
pending runtime2의 기존 selector는 legacy `run_cli`→live C++/native compare→two-role transport를
지난 뒤 runtime native barrier에 도달한다. current retained runtime2는
고정 inert C++ 원문 비교→native-only transport 준비라는 다른 entry다.
같은 post-barrier Peer/cleanup assertion이 있다는 것만으로 preparation/source-provider bridge가
닫히지 않으므로 **pending runtime2는 HOLD**다.
새 원본 C++ 실행·capture·archive ELF 실행·timer 이동 없이 같음을 증명할 작은 continuation은
별도 심사 대상이며, 이번 exact exec 정책 구현에 몰래 추가하지 않는다.

## 보완 전 체크포인트 상태

helper postimage는 기존 동결 `0781d4a13b78bc4125d3903751466a33431176e118f41469596723a8d3ed5109`로
변경하지 않았다. 새 jq 미작성, 원본/caller/WF/Focus/ledger/기존 cache554 문서 수정0이다.
정확 source 비교 metadata는 ignored `logging-exec-549-initial-source-bridge.json`에 있다.
local functional/test/compile/CLI/Actions 실행·stage/commit/push0이다.

## 승인된 제한 구현

변경 파일은 `trace.rs`, `trace_summary_tests.rs`, `logging_retained_549.sh`,
새 `logging_retained_exec_policy_549.jq`, 이 문서5개뿐이다. cache554 문서/fixture 본문/
workflow/Focus/ledger/원본2/caller는 편집하지 않는다.

summary schema2의 `detailed_operations`는 `fork/vfork/clone/clone3/setpgid/setsid/unshare/setns`
8종으로 고정했다. 성공·실패 attempt와 원래 decoder의 PID/result/source span을 보존한다.
기존 exec/signal/terminal/aborted 보존과 전체 strict lexer/집계는 같다. cwd는 이 정책에 필요하지
않아 추가하지 않았다. wait/poll/read/파일/메모리 상세를 보존하는 새 모드는 만들지 않았다.
full schema1/기존 trace24·summary6 본문과 모든 상한은 그대로다.

새 summary test1은 8종 각각 정상+EPERM 실패16개를 포함한20줄에서 상세18개와
전체 집계20개를 대조한다. read/wait4는 집계에만 남는다. source span/result/raw bytes와 false flags도
확인한다. 정확 selector는 `native_logging_check::trace::summary_tests::`이며 이제 **7 PASS 기대/0 ignored**다.
새 이름은 `summary_keeps_successful_and_failed_process_session_details_with_exact_counts`다.
helper의 정렬 exact-name roster도7개로 바꿨다. 기존 core21+import9+trace24+summary7은 총61개다.
이는 기대 선택 수이며 **로컬/새 GHA 기능 합격 수가 아니다.**

## exact15 정책의 입력과 실제 assertion 연결

helper는 모든14 lifecycle body와 retained-once가 끝난 뒤 읽기 전용으로 바인딩을 만든다.
traced14는 lifecycle root16개다. forbidden selector의 실제3개 거부 case가 각각 receipt를 남긴다.
별도 untraced runtime-directory-int의 증거는 `paired-554/untraced/evidence`이므로 이16개에
넣지 않는다. root roster/key의 누락·중복은 FAIL이다. 각 owned0700 canonical root의
작은 single-link JSON과 고정14개 또는1개 invocation을 held FD/hash에 넣는다.
root의 dev/ino/mode/uid/gid도 읽기 전후 같아야 한다. 입력 파일당1MiB 상한이다.
generated binding은 원문 exec에서 argv/env를 복사한 expected가 아니다.

| case | source에서 정한 exact argv/env와 결과 | 실제 receipt/trace 연결 |
| --- | --- | --- |
| first3 | fixture→checker→initial Git2→Peer Git2→FD probe1; EOF만 사후 Git2. checker130/143/1, probeSIGKILL | 실제 checker/peer pidfd·UID/start/parent/PGID/session·exe inode/hash와 invocation PID/endpoint, owned IPC 제거·foreign 보존·실제 peer reap assertion |
| runtime directory2 | 위 준비 +7console/rate/backpressure/threads/console-failure/invalid2/INT/TERM/runtime의 source 순서. Git30/native16, held runtime peerSIGKILL | 14개 고정 invocation, runtime endpoint/private marker, 기존2초 backpressure·packet/response·foreign directory assertion 그대로 |
| escaped3 | first3 +현재 canonical probe의 고정 `--qa-logging-escaped`→`--qa-logging-leaf` | 실제2 pidfd/exe 관측·parent chain·독립 session/PGID·setsid1·각 terminal TERM/KILL와 원래 descendants reaped assertion |
| occupied output5 | fixture→checker1/exit1만. QA opt-in argv/env도 source와 동일 | 원래 before==after/barrier 부재 assertion + checker PID의 **8종 상세 attempt0**; 빈 summary만으로 child0 판정하지 않음 |
| forbidden3(한 selector) | builder/reference/source-hook 순서의 exact checker3/각 exit2, 다른 exec0 | 원래 CLI error/output 부재 assertion + 각 checker의8종 상세 attempt0 |
| retained-once | 직접 checker→Git32/native16, 총49 exec, exit0. 실제 helper의 whitelist env | 기존 fresh250/697·retained249/694·7+7 bridge/report/provider/source before-after 및14 invocation 연결 |

full recipe의 console LOGPRINT7·빈값·한글 ctx·CLEAN=0, runtime prefix, invalid endpoint2,
signal2의 argv/env/terminal도 source에서 각각 만든다. dynamic endpoint/FD는 고정 경로 규칙과
별도 invocation·held-provider receipt로 bind한다. FD pathname을 canonical provider 경로라고
바꿔 쓰지 않는다. escaped self-exec만 actual canonical provider 경로다.
Git은 현재 held `/usr/bin/git`, 고정 argv2종과 case별 실제 환경을 비교한다.
fixture/checker는 현재 Cargo-selected canonical ELF, probe는 fresh private nlink1 copy다.
jq/provider/DSO 및 policy source도 기존 held before/after 목록에 추가한다.

jq는 상세8종의 kind/op/outcome별 count를 summary 전체 `event_counts`의 해당 부분과 정확히
비교한 뒤 zero-child/creation을 검사한다. failed spawn도 count에서 빠지지 않는다.
성공 spawn의 child PID는 해당 raw PID 파일을 가져야 하며 actual checker/peer/escaped 생성
edge와 source 순서·terminal에 연결된다. fixture의 libtest worker를 경유하는 checker 생성만
깊이8 한도의 creation 경로로 연결한다. clone의 thread/FD 공유 flag를 해석해 closed family로
승격하지 않는다. namespace 작업, aborted spawn, 미지 session/exec branch는 unsupported FAIL이다.
setpgid는 알려진 Git/native child의 exec 전 성공0, setsid는 escaped PID의 성공1회만 허용한다.
execveat/실패 exec/추가 exec/다른 program·argv·env는 거부한다.
따라서 Python/C++ 실행0은 **이15개 raw prefix의 모든 exec attempt가 exact recipe에 맞을 때만**
기록한다. build/진단/전체 프로젝트의 Python 부재나 whole provider closure로 확대하지 않는다.

## GHA 음성과 변경하지 않은 경계

실제 first_int positive summary가 먼저 수용된 뒤 작은 변형4개를 GHA에서만 검증한다.
원문 raw/expected/fixture는 수정하지 않는다. 각 jq exit5·빈 stdout·해당 guard의 구체 오류를
함께 요구하므로 단순 실행 오류를 negative PASS로 처리하지 않는다.

| 변형 | 요구 오류 |
| --- | --- |
| structural 상세 제거/집계 보존 | `process/session detail cardinality differs` |
| checker argv1개 추가 | `checker source argv differs` |
| checker env1개 추가 | `ordered exec/argv/env/terminal/creation differs` |
| checker exec 복제/집계 보존 | `unsupported exec attempt` |

원래15 inspect 순서와 raw/metadata/SHA를 보존하고 각 inspector 뒤 별도60초 read-only jq를
호출한다. 음성도 같은 기존 owner/budget wrapper를 쓴다. native14의 공유600초/각95초와
기존 backpressure2초·packet·console/rate·기능 본문/retained-once360초는 변경하지 않는다.
raw1GiB prefix/512MiB file/report64MiB 및 job proof2GiB/build8GiB/reserve512MiB도 같다.
새 metadata/report가 기존 예산을 초과하면 그대로 실패한다. 통과 성능/용량을 예단하지 않는다.

정책 성공 receipt도 `full_event_index`, `closed_pid/cwd/fd/dso`, `all_adopted_reaped`,
`old27_accepted`, `pending_runtime2_accepted`, `R4_acceptance`를 false로 남기고 retirement credit0이다.
raw immutable은 기존 inspector의 held 입력 검증 scope다. 이후 모든 시점의 공격적 same-UID
변경 배제를 새로 주장하지 않는다. actual fixture의 named-peer/escaped reap assertion과
관측한 creation/terminal을 묶었으며 모든 non-exec worker의 wait/FD/DSO semantic closure는 아니다.
위 historical17/pending runtime2 표의 HOLD와 C++ provider provenance 차이도 그대로다.

현재 로컬 검사는 Bash 문법/rustfmt/jq의 실행되지 않는 branch 문법/서식 diff뿐이다.
프로젝트 CLI/fixture/원본/functional/test/build/Clippy/Actions 실행·stage/commit/push0이다.
부모는 새 jq 경로 source admission/pin과 helper literal/source pins 및 summary7 선택 수를
통합해야 한다. **새 source의15 policy/4음성/summary7 기능 결과는 GHA UNRUN**이다.
