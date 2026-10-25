# #474 recovery full traced8의 private provider 연결 준비

2026-10-26. 추적: [#474](https://github.com/bin9208/openpilot-rust/issues/474),
[#560](https://github.com/bin9208/openpilot-rust/issues/560), [#523](https://github.com/bin9208/openpilot-rust/issues/523).

**상태: 후속 소유 확대에 따라 `recovery474` 명시 context를 구현하여9.5GiB 대5.5GiB의
source 계약 충돌을 해소했다. hosted 실행 준비 source는 연결했으며 실제 provider build/
통제26개/full traced8/strict audit·자원 fit는 GHA UNRUN이다.** Fast 기본 scope는 그대로다.

## 근거와 수정 범위

source `dd3a41f44fadda755c90224785a84554fa62318d`의
[run40227564162/job121272080733](https://github.com/bin9208/openpilot-rust/actions/runs/40227564162/job/121272080733)은
untraced8과 traced8 harness가 각각254.90s/318.68s에 PASS한 뒤 strict exec audit에서 실패했다.
artifact12612666505의 SHA256은
`caed9ba3737ea19a60dea46e4f0265880b77e5b062dcd64e0ab9d6b31ef1d696`이며
12,211 unique/safe members, expanded219,753,266B다. exec trace2,508개 중 유일한 unknown은
`recovery-trace/exec.16876:5`의 `???( <unfinished ...>`다. 같은 파일의1–3행은 세 exec0,
4행은 서버16855의 SIGHUP,6행은 SIGKILL이다. 실제 ptrace errno는 이 산출물에 없다.
종료 관측을 근거로 unknown을 정상 또는 exec 부재로 해석하지 않는다.

이번 소유 파일은 다음뿐이다.

- 새 `tools/ci/recovery_tracer_474.sh`: hosted 준비/통제/PTY/실제 trace/최종 봉인의 adapter.
- `tools/ci/prepare-cold-strace-560`: 후속 허용된 명시 recovery context/outer admission 분기만 추가.
- `.github/workflows/python-rust-523-recovery-lanes.yml`: 실제 단계와 추가 예상 성장량 연결.
- 이 문서.

기존 provider C source/patch/common launcher/fixture와 recovery production,
strict audit/parser는 수정하지 않았다. 새 Python helper, 승인 protocol, trace 정규화,
unknown skip/allow, fallback system strace를 추가하지 않았다. stage/commit/push/Actions 조작은0이다.

## 이전 범위 충돌과 명시 recovery context

기존 Fast default는 provider512MiB + non-provider최대5.5GiB = envelope6GiB다.
이전 recovery9.5GiB 예상은 이 조건에서 차단되었다. 후속 사용자 허용으로
`COLD560_PREPARE_CONTEXT=recovery474`만 별도 범위로 추가했다.

명시 context는 실제 `github-hosted` + `GITHUB_JOB=recovery`, 허용된 repository/event/Linux/X64,
기존 private source/job owner/SDK root/manifest/SHA 검증을 모두 요구한다. 남은 forecast도
정확히 `10200547328`B(9.5GiB)여야 한다. 새 envelope는 literal10GiB이며 임의 숫자로
global cap을 바꾸는 환경변수는 없다. context 미설정 또는 `fast`는 원래6GiB다.

현재 workflow의 `recovery-growth-plan.tsv` 세 행을 실제 workspace/job/APT device/path와
source-authored 성장량에 대조하고, `recovery-growth-admitted.tsv`가 filesystem별 합산 필요량
이상의 실제 available을 기록했는지 검증한다. 누락/중복/잘못된 행·숫자·device·성장량은 거부한다.
두 receipt도 provider 입력에 복사하고 원래 before/after metadata+SHA 봉인에 포함한다.

두 범위는 단계별 잔여량이 다르다.

| 입장 시점 | 포함하는 예상 | 합계 |
| --- | --- | ---: |
| checkout 전 outer | source2 + target6 + registry2 + SDK3 + proof1 + APT2 + reserve0.5 + provider0.5 | 17GiB |
| checkout/SDK/APT 완료 뒤 provider | target6 + registry2 + proof1 + reserve0.5 + provider0.5 | 10GiB |

이미 앞 단계에 속한 source2/SDK3/APT2의7GiB를 다시 future growth로 세지 않는 차이다.
provider의 실제 allocated/apparent512MiB 상한, 준비120초·make-j1, 최초 trust420초와
첫 disk-guard 전 호출, source/SDK/컴파일 입력/ELF/DSO의 전체 봉인은 그대로다.
verify에서도 같은9.5GiB forecast를 보수적으로 유지하며0으로 바꾸지 않는다. 두 숫자는
고정 free-space floor나 실측 peak 보장이 아니다.

Fast default 역투영은 추가한 `BEGIN/END recovery474` 다섯 블록만 제거하면 원래20,232B와
byte 동일하다. 역투영 SHA256은
`6c7d99e6f5c1ee7fc4929c105a8bbb80334a47ec75e7e990ca5ea47c3d81bd58`다.
원래 recipe/기능18통제/library/observer·상속 patch/512MiB/6GiB/기한 body는 바꾸지 않았다.
현재 Fast ee74의 Prepare/Seal 실패 원인은 부모가 조사 중이며 이 변경으로 해결했다고
주장하지 않는다. Fast caller의 새 preparer SHA pin 반영도 부모가 담당한다.

adapter의 `prepare`에는 source-authored context 음성 fixture3개를 추가했다.
`unknown-context`, `wrong-job`, `selfhosted-context`가 각각 정확한 거부 문구·exit1/direct wait1·
빈 stdout과 provider/proof/cache 미생성을 요구한다. 잘못된 job/runner 값은 이 음성 통제의
명시 fault input이며 실제 runner의 식별값으로 주장하지 않는다. 이 통제도 GHA에서만 실행한다.
증거는 `proof/recovery-provider-474/prepare/context-controls{,.expected}.tsv`에 남긴다.

## 현재 구현한 호출과 보존한 실행 경계

workflow는 실제 GitHub-hosted Linux/X64/정식 private SDK를 그대로 사용한다. 첫 trust step의
실제 clock을 보존하고 helper에 넘긴다. SDK 설치/공유 `/usr/bin` 교체나 가짜 runner/SDK 경로는 없다.
source filesystem과 job/APT filesystem이 같으면 성장량을 합산한다. 기존16.5GiB 예상에
provider0.5GiB를 명시적으로 더한 **17GiB 예상**이며 고정 여유 floor나 실측 peak 보장은 아니다.
추가 통제의 로그/소규모 native fixture는 기존 proof1GiB 예상 안에 두고 adapter proof/work에는
별도256MiB 상한을 둔다. 실제 model/성능 검사는 없고 기존 Cargo jobs2/incremental0/debug0다.

APT에는 기존 recovery 도구 외 `build-essential`, `dpkg-dev`, `perl`, `patch`, `xz-utils`,
`curl`, `jq`를 명시했다. 기존 `/usr/bin/strace`는 unchanged preparation의 package/ELF 비교
입력이며 실제 traced8을 실행할 provider 대체물이 아니다.

단계와 명령은 다음과 같다. 모두 **Actions 전용**이다.

```bash
/usr/bin/bash "$GITHUB_WORKSPACE/tools/ci/recovery_tracer_474.sh" prepare
/usr/bin/bash "$GITHUB_WORKSPACE/tools/ci/recovery_tracer_474.sh" controls
# 원래 동일7실행물의 build/select와 untraced8은 독립적으로 유지한다.
/usr/bin/bash "$GITHUB_WORKSPACE/tools/ci/recovery_tracer_474.sh" pty
/usr/bin/bash "$GITHUB_WORKSPACE/tools/ci/recovery_tracer_474.sh" trace
/usr/bin/bash "$GITHUB_WORKSPACE/tools/ci/recovery_tracer_474.sh" after
```

`prepare` 실패 후에도 원래 build/select와 untraced8은 `always()` 및 SDK/APT 성공 조건으로
진행한다. 반대로 trace는 provider/통제/PTY 완료 marker가 없으면 실패한다. 실패를 성공으로
바꾸거나 stock tracer로 재실행하지 않는다. 같은 server/actions/upload/process-child/CWP/tmux/
harness7개만 build/select하고 기존 전후 SHA/stat와 authorized hardlink 예외를 그대로 보존한다.

full traced8 target은 원래 `eight_native_recovery_controllers_same_elf_present_and_absent`
selector/argv이며 원래 native evidence root를 사용한다. provider는 실제
`$CI_JOB_TEMP/cold-strace-560/bin/strace`와 준비 receipt의 full SHA로 결합한다.
기존 trace 인자는 `-ff -qq -s 32768 -xx -e trace=execve,execveat`이며 여기에 실제
`--seccomp-bpf`와 기존6개 controller 환경 제거 옵션만 추가한다.
기존 strict `audit-recovery-exec pre/post`의 argv·policy·수용 기준은 그대로다.

`inherited_command`의 기존 API로 fresh `setsid Bash → timeout → env -i → private strace`를
만들고, 기존 native approver가 실제 STOP/PID/startticks/PPID/PGID/session/ELF/SHA/정확 argv와
single-use O_EXCL 승인 namespace를 확인한 뒤 CONT한다. `COLD560_FILTER_INHERIT=1`만
설정해서 통과시키지 않는다. 실제 hosted/SDK/source 값과 preflight가 사용한 원래 PATH를
전달한다. shared implementation binding은 실제 common library SHA이며 diagnostic SHA를 사칭하지 않는다.

full trace timeout은900초 상한, 기존 TERM/KILL-after15초와 drain 상수, 외부 step16분,
전체 job45분을 유지한다. 별도 controls step8분·PTY step2분은45분 안에서만 실행한다.
provider 준비의120초/512MiB/첫 trust420초 제한은 완화하지 않았다. 실제 fit는 미검증이다.

## 기존18통제와 추가 통제

common launcher는 held FD에서 고정 SHA를 검사한 뒤 definitions만 source한다.
18통제와 protocol3은 고정 SHA의 원래 `cold_tracer_diagnostic_560.sh`에서 다음 네 함수
정의만 원문 그대로 추출해 사용한다. diagnostic의 main/preparation/cold body는 source하지 않는다.
추출 결과도 고정 combined SHA를 검사한 held FD에서 source하므로 임의 line range 실행이 아니다.

| 함수 | bytes | SHA256 |
| --- | ---: | --- |
| `bind_elf` | 1726 | `5156ba6edabb3562dde8565e2a4bb358cf9b1f27a3c4f332e88b7fe1d3dc0095` |
| `inherited_controls` | 4860 | `c1176ce15eaed88dad3439ee2fb1cde0191c2613261ae09bd1fdd2b77b458a5f` |
| `run` | 325 | `08c418702fee49e29e9f4a60c21bdcd5df0dfa3ed427e75e8eb771c89f09bead` |
| `protocol_controls` | 4234 | `0167bd31ee6e5826577f6f322d781363525a93cdf2422fd1a8051fa43f8244de` |

연결11,145B SHA256은 `6cc97850d050f6a18a24afd949eae3e9c7a8e189245f43ca384cb358f2335bf6`다.
원래18개의7양성/4binding음성/7context음성, 각 내부 순서·timeout·native semantic verifier·
실제 VFORK event·정확 stdout·원래 unknown/잘린 EOF 거부는 그대로다. controls는 첫
disk-guard 전에 실행해야 하는 원래 계약을 유지한다. protocol3의 실제 STOP/CONT,
원래 exit7 보존, live-member 거부와 소유 KILL/direct wait도 같은 원문이다.

추가 PTY 통제는 두 가지다.

1. 동일 CWP test ELF의 실제
   `terminal::pty::tests::exited_leader_remains_reserved_until_owned_descendant_cleanup`를
   private provider로 실행한다. 기존 production `Generation::drop`의 HUP→KILL과
   HUP-resistant descendant 제거, leader reservation/reap, unrelated child 보존 assertion을
   그대로 사용한다. 통과 marker와 원문 trace를 보존한다.
2. 새 helper 안에 source로 봉인한 작은 C probe를 GHA에서만 compile한다. 실제 `openpty`/
   controlling terminal과 child exec 뒤 SIGSTOP을 발생시킨다. 부모가 WUNTRACED로 실제
   SIGSTOP을 확인하고 SIGCONT한 뒤 직접 wait로 exit0를 확인한다. PTY의 전후 출력 및
   raw trace의 실제 SIGSTOP/SIGCONT를 함께 요구한다. child에는 parent-death KILL과
   실패 시 소유 child 회수를 둔다. 이는 새로운 tracer 승인 protocol이 아니다.

두 통제의 unknown/unavailable trace는 실패하며, 이러한 kernel 동작 통제가 full traced8의
strict exec audit를 대신하지 않는다. 기존 marker10/cold auditor의 실제 실행을 했다고도
주장하지 않는다. 전체 요구는 새 context음성3 + closure18 + 기존 protocol3 + PTY2 =
**26개**이며 모두 **GHA UNRUN**이다. 원래 closure18의 개수와 본문은 그대로다.

## 실패, 전후 봉인과 인계

helper는 실제 현재 owned root에서만 proof/work를 만들고 불명 소유 root를 정리하지 않는다.
완료 label 재사용/기존 artifact 덮어쓰기는 거부한다. 실패한 owned command는 원래 shared
회수/직접 wait/nonlive 검사를 거치며 원래 실패와 finalizer 실패를 별도로 기록한다.
native harness status와 adapter status를 따로 보존해서 harness0이 승인/봉인 실패를 덮지 않는다.

원래 prepare helper의 source/SDK/index/inputs/source/build/bin/compiler/DSO 봉인은 재사용한다.
controls 뒤 기존 `after-cold`, 전체 trace 결과 뒤 `after-fast` 검증을 호출하고 workflow의
SDK/source/index always-final도 유지한다. 예산 단계에서 provider가 시작하지 않았으면
after가 성공 marker를 만들지 않는다. artifact는 초기 scope failure, raw/status/observer/
binding/argv/whole-source seals, provider 입력/license/patch와 실제 private ELF도 보존한다.

cleanup은 원래 upload id/digest·source/SDK final·두 lane 성공에 더해 prepare/18통제/
PTY2/provider final을 모두 요구한다. partial/unknown/timeout/소유권 또는 봉인 실패는 회수
승인이 아니며 실패 원문을 지우지 않는다.

로컬은 Bash/YAML syntax, embedded workflow Bash syntax, source-only 함수 추출 byte/SHA와
diff만 확인한다. helper source/실행·원본·모델·Cargo·C/Rust compile·test·strace·GHA 실행은0이다.
부모가 Fast caller pin/Focus/원장과 별도 commit/push를 담당한다. 새 context/음성3과
provider build·기존18/PTY2/full traced8/strict audit를 exact-source GHA에서 확인해야 한다.
실제 성공 전에는 #474/#560 해결·퇴역 credit을 주지 않는다.

검토한 source pin은 다음과 같다. 실제 provider ELF SHA는 GHA의 current recipe build가
완료된 뒤 생성하는 값이며, 과거 ELF SHA를 새 실행 값으로 사용하지 않는다.

| 파일 | SHA256 |
| --- | --- |
| `tools/ci/recovery_tracer_474.sh` | `5704296015b6ac6bbf404a5b5ec94ad9421284524b0b2b7464ac7f486b63aed7` |
| `tools/ci/prepare-cold-strace-560` | `051332a53de80a8debc38c7ef1fcaf0fa43a113d03300b33ea7e6660687f0c5c` |
| `tools/ci/cold_tracer_launch_560.sh` | `1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a` |
| `tools/ci/cold_tracer_diagnostic_560.sh` | `8a7383587589d01404f7e6573db8fd1ad458c22fbc6ff2755298b5a4a35ddc4b` |
| `tools/ci/fixtures/cold560_exec_inheritance.rs` | `e012d523f41a1a51cb49fd05cd334e04085ab562873f17303fffff6b10ad15fd` |

## source1000464의 direct harness와 strict 승인 roster 재검토

통합 기준 `1000464f57239a1d15bfba3bdb6c01550f12d8a0`에서 실제 caller와 원래 strict
policy/parser를 다시 읽었다. full traced8은 fixture나 root-wrapper ELF를 tracee로 넣지 않는다.
원래 harness를 provider의 target argv로 직접 전달하므로 **소스 계약상 기존 승인 roster로
연결 가능**하다. 이것은 현재 GHA 통과 또는 앞서 기록한 preparation 예산 충돌의 해결이 아니다.

```text
adapter Bash
  ├─ setsid Bash → timeout → env -i → private strace (내부 c560_launch STOP)
  │                                   └─ 원래 RECOVERY_HARNESS (기록된 root PID)
  │                                        └─ 원래 recovery controller/자손 전체
  └─ env -i → timeout → 기존 fixture ready/approve (부모 쪽 승인 명령)
```

근거는 다음과 같다.

- `recovery_tracer_474.sh`의 `inherited_command recovery ...`는 `-o .../recovery-trace/exec`
  다음에 원래 `RECOVERY_HARNESS`와 동일 selector/argv만 전달한다. `fixture scenario`와
  새 PTY probe는 별도 통제 호출이며 full traced8 target argv에는 없다.
- common `start_owned`는 tracer의 **부모** timeout/env/Bash를 만든다. `approve_launch`의
  `fixture ready/approve`는 adapter Bash에서 호출한다. tracer가 이 부모나 sibling 명령을
  attach하는 경로는 없다. 기존 provider context는 attach/mixed-attach를 거부한다.
- pinned provider patch의 `c560_launch`가 tracer 자신을 STOP하고 승인을 읽는다. 그 함수에
  fixture exec를 넣지 않았으며, 원래 target을 생성하는 경로와 `strace_child` root PID를
  그대로 사용한다. root-first 검증을 위해 target 앞에 새 ELF를 끼우지 않는다.

승인 fixture는 provider 제어 쪽의 held ELF/SHA·PID/startticks/argv/소유성 증거로 검증한다.
이를 recovery program의 새로운 승인 rule 또는 전체 CI exec 감사 성공으로 주장하지 않는다.
특히 **실제 `recovery-trace/exec.*`에 들어온 행이나 파일은 하나도 제외하지 않는다.**
fixture/wrapper가 실제 그 corpus에 나타나면 원래 strict policy가 그대로 적용된다.
승인되지 않은 ELF에는 Generic allow나 unknown skip을 추가하지 않았다.

workflow에 추가한 guard는 원래 strict `post`가 성공해 쓴 `exec-audit.json`을 사용한다.
provider의 고정 inheritance receipt에서 root PID를 받고, 원래 preflight의 sealed
`selected.harness`를 독립 기대값으로 삼아 해당 PID의 **첫 native-parsed exec attempt**가
그 harness/원래 argv의 Success인지 확인한다. fixture를 거친 뒤 harness를 exec하는 구성은
이 조건을 만족하지 않는다. 현재 고정 Actions 경로/argv의 ASCII byte 조건도 명시한다.
판정과 첫 root attempt는 `recovery-trace/direct-root-audit.json`에 남긴다.

이 guard는 native parser를 대체하지 않는다. 기존 `pre`/`post` 호출과 승인 roster는
바꾸지 않았으며, post에서 unknown이 나오면 guard에 도달하기 전에 그대로 실패한다.
원본 artifact의 `exec.16876:5` 문자열을 수정·삭제·정규화하지 않았다.

따라서 roster 때문에 observer-only 경로로 바꿔야 한다는 충돌은 현재 소스에서 발견하지
못했다. 이후 사용자 소유 확대로 위 명시 recovery context를 구현해 preparation 계약 충돌은
source에서 해소했다. 실제 current provider/full traced8/direct-root 결합의 GHA 수용은
미검증이며, 그 전의 퇴역 credit은0이다.
