# #474 recovery full traced8의 private provider 연결 준비

2026-10-26. 추적: [#474](https://github.com/bin9208/openpilot-rust/issues/474),
[#560](https://github.com/bin9208/openpilot-rust/issues/560), [#523](https://github.com/bin9208/openpilot-rust/issues/523).

**상태: Batch9 source `7a928cefe`의 실제 hosted GHA는 provider 준비/context3/closure18/
protocol3/untraced8을 통과했다. 추가 PTY 통제의 원문 exec 결과 `= ?`를 strict하게 거부해
job은 실패했다. STOP/CONT PTY 통제와 full traced8/direct-root/strict post audit는 UNRUN이다.**
provider/SDK/source 최종 봉인은 통과했다. 아래 Batch9 관측에 exact artifact와 범위 밖 전제를
기록했다. Fast 기본 scope와 unknown 거부 정책은 그대로다.

후속 사용자 허용으로 기존 Rust PTY 테스트에 exec 이후 sleep readiness barrier를 추가했다.
새 source의 기능/Clippy/full traced8 수용은 GHA UNRUN이며, 아래 별도 절에 변경과 인계를 기록했다.

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
**26개**다. 최초 준비 때 모두 GHA UNRUN이었으며, Batch9의 실제 통과/실패/미실행은 아래에
구분했다. 원래 closure18의 개수와 본문은 그대로다.

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

## Batch9 exact-source GHA 관측

source `7a928cefe22e3c81bcb941f6ed20eaa80eeabc59`의
[run40236805934/job121300012622](https://github.com/bin9208/openpilot-rust/actions/runs/40236805934/job/121300012622)을
읽기 전용으로 관측했다. 2026-10-26 06:54 KST 종료 결과는 **failure**다. 상태가 그대로인
조회 뒤에는 간격을 늘렸고, cancel/rerun/dispatch는 하지 않았다.

| 대상 | 실제 결과 |
| --- | --- |
| private provider 준비 + context 음성3 | PASS. 세 거부 receipt와 provider build-complete가 current source에 결합됨 |
| 원래 closure18 + STOP/CONT protocol3 | PASS. 원래18행과 protocol3행 및 native semantic verifier/direct wait 결과 보존 |
| 동일7실행물 build/select | PASS. 관련 native unit32 및 Clippy도 해당 step에서 성공 |
| untraced full8 | PASS. harness exit0, `1 passed; 0 failed`, 256.91s |
| 실제 CWP HUP→KILL PTY 통제 | native test/tracer/direct wait0. 원문 selected exec의 `= ?` 때문에 adapter는 실패 |
| 별도 controlling PTY STOP/CONT 통제 | 앞 통제 실패로 compile/실행 모두 UNRUN |
| full traced8 / strict post / direct-root guard | 앞 PTY 완료 marker가 없어 시작 전 거부. driver1/harness125, `recovery-trace/exec.*`0개 |
| provider after-fast / SDK after / source·index after | PASS. 실패 상태를 성공으로 바꾸지는 않음 |
| artifact upload / cleanup | upload PASS. 두 cleanup은 AND 조건에 따라 skipped |

따라서 전체26개 중 context3 + closure18 + protocol3의 **24개 통과**이며, 추가 PTY 첫 통제는
실패, 두 번째는 미실행이다. full8×2나 strict auditor/direct-root 성공을 주장하지 않는다.

artifact는 `recovery-native-lanes-40236805934-1`, ID `12615328251`이다.
ZIP55,004,275B, unique/safe/CRC 정상6,717 members, expanded194,492,371B를 inert하게 확인했다.
원문 archive는 `.analysis/scratch/2026-10-25-523-completion/ci/12615328251.zip`에 있다.

| 실제 산출물 | SHA256 |
| --- | --- |
| artifact ZIP | `0b55c63a1b00564eba07dfa01c2401726acb28b0f5c5fdd920e009df7bd152ad` |
| `cold-strace-560/bin/strace` | `e4cc9502be3826da01635e6f7b2ce9cf59c49e7d95b30b58f0628d8bf29973d1` |
| `proof/recovery-provider-474/pty/pty.exec` | `55c153b631a3eaeb90b1bf1b283e4eb12f5eb46e63ee98a439ba32d90104c053` |
| `proof/recovery-provider-474/controls/inheritance/controls.tsv` | `fcd0a76afcd4a81bac7c4e7163a5d36efc216064fe42b338d9af503c5cbbd14a` |
| `proof/recovery-provider-474/controls/protocol-controls.tsv` | `12f483f6f046a525df229b1e3400c0b046d694154c307a102147fde9315042b2` |
| `proof/recovery-provider-474/prepare/context-controls.tsv` | `dfcdfd8ebb7b0aed2d51c40607c7d2ac3dedf1aeb35c3b2076ecb10e7b375f47` |

### 첫 실제 실패와 소유 범위 밖 전제

job log의 `2026-10-25T21:54:11.3147009Z`에서
`recovery tracer 474: unclassified/unavailable PTY control trace`로 실패했다.
원문 `proof/recovery-provider-474/pty/pty.exec`는24행/7,441B다.

- 3행: PID36634의 `/bin/sh -c`가 HUP-resistant background child를 시작하고 PID를 출력한 뒤 exit23.
- 6–8행: PID36638의 descendant `/bin/sh` exec가 시작·성공.
- 9–19행: PID36638이 실제 hosted PATH의 `sleep` 후보를 탐색하며 ENOENT를 기록.
- **20행: PID36638 `execve("\\x2f\\x75\\x73\\x72\\x2f\\x62\\x69\\x6e\\x2f\\x73\\x6c\\x65\\x65\\x70", ... ) = ?`**.
- 21행: 같은 PID36638 `+++ killed by SIGKILL +++`; 22행은 harness PID36630의 CLD_KILLED receipt.

이는 처음 조사한 dd3의 `???( <unfinished ...>`와 다른 **selected exec 결과 미확정**이다.
해당 trace의 다른 exec entry/resume 쌍은 남아 있지만 이 결과를0/ENOENT/exec 부재로 추정할
근거는 없다. 실제 ptrace errno나 마지막 kernel stop은 이 artifact에 없으며, 특정 errno 또는
provider 내부 분기의 결함으로 단정하지 않는다. unknown/result 거부를 유지한다.

소스에서 좁힌 통제의 문제는
`rust/crates/carrot-recovery/src/terminal/pty_tests.rs:41,61–78`의 준비 완료 조건 부재다.
`OWNED_CHILD`는 background PID만 전달한다. 테스트는 descendant의 exec 이후 준비 완료를
기다리지 않고 `drop(generation)`을 호출한다. 실제 production
`rust/crates/carrot-recovery/src/terminal/pty.rs:129–140`은 HUP 직후 KILL을 보내므로,
이번처럼 descendant의 exec 관측 완료와 종료가 겹칠 수 있다. native ownership/cleanup
assertion 자체는 통과했지만 lossless exec 관측 통제로는 그 준비 조건이 부족하다.

bounded 후속은 **이 native 통제에서 descendant의 exec 이후 준비 완료를 실제로 확인한 뒤**
동일 `Generation::drop`과 leader reservation/descendant cleanup/unrelated child assertion을
실행하는 것이다. 단순 PID 출력, exec 직전 printf 또는 고정 sleep 지연은 완료 증거가 아니다.
예를 들어 고정 sleep ELF/동일 PID identity와 실제 sleep syscall 도달을 함께 확인하는
조건을 native test에 둘 수 있다. production의 HUP/KILL 순서·timers나 audit 정책을 바꾸는
제안이 아니다. Batch9 관측 당시 이 테스트 파일은 recovery4 소유 범위 밖이므로 수정하지 않았다.
후속 사용자 허용으로 이 파일의 최소 barrier만 구현한 내용은 아래에 따로 기록했다.

owned adapter/preparer/WF에서 이 trace를 성공으로 만들려면 결과 거부 완화, trace 제외 또는
실제 종료 순서 변경이 필요하므로 그런 patch는 만들지 않았다. 첫 원인과 범위 충돌을 부모에게
먼저 알렸다. 부모가 별도로 통보한 recovery WF 두 `if`의 outer guard 수정과 Fast literal/
`Vec<OsString>` 수정도 부모 소유이며 이 관측 patch에서 건드리지 않는다.

### 실패 뒤에도 확인한 봉인과 자원 관측

`after-build`, `after-cold`, `after-fast` 각각의 SDK 전체 및 provider inputs/source/build/bin
roster/SHA와 tracked source/index/stage가 before와 byte 동일했다. 준비 tool/ELF/DSO120행,
compiler input tree6개와 각 outer admission receipt도 원래 verify에서 성공했다.
별도 SDK files/programs before/after, workflow index/source before/after도 byte 동일했다.
tracked source tar digest는 `c88e97d5e9590bb627009091436544c85aa02716c1647917f9a304747c86780d`다.

각 phase의 held FD/path/stat/SHA receipt는 순서와 반복 검증을 고려한 고유 행 집합이 전후
동일했다: prepare6, controls78, PTY17, trace7, after7. controls after는 각 행을 두 번 기록한
156행이며 다른 값은 없다. PTY와 trace의 실패 status는 그대로1이며 owned group nonlive는
true다. PTY의 provider inheritance receipt는 실제 root36630/root_seccomp1/fresh4/inherited4/
additional_ptrace0/parent_link_observed0이고 observer io_error0/overflow0다. 이 정상 receipt가
selected exec 결과 미확정 실패를 덮지 않는다.

prepare step의 실제 구간은21:43:58.505–21:45:32.298 UTC로 약93.8초다. admission에 기록된
최대 provider/proof allocated223,854,592B, apparent179,959,301B는512MiB 이하이며,
after-fast의 available86,723,317,760B였다. 이는 이 hosted 실행의 sampled 관측이고 Fast
7분/6GiB 또는 다른 runner의 peak/fit 보장이 아니다. recovery의 scoped10GiB와 outer17GiB,
provider120초·512MiB 상한은 완화하지 않았다.

Batch9 관측 변경은 이 문서뿐이었다. 로컬은 archive CRC/SHA/텍스트·source/diff 검사만 수행했다.
로컬 기능·Cargo·compile·strace·project CLI 실행과 GHA dispatch/cancel/rerun, stage/commit/push는0이다.
이 결과로 원래 PTRACE_LISTEN 결함 전체의 해결 또는 이슈 종료·퇴역 credit을 주장하지 않는다.

## 후속 native PTY 통제의 exec 완료 barrier

2026-10-26 후속 사용자 허용으로
`rust/crates/carrot-recovery/src/terminal/pty_tests.rs`와 이 문서만 수정했다.
기존 `process_state`와 production/session helper를 먼저 읽었다. 기존 executable identity
helper는 다른 crate의 private API이고 sleep 진입을 증명하지 않으므로 새 의존성/공유 API 없이
이 test module에 `wait_for_sleeping_descendant` 함수 하나를 추가했다.

barrier는 기존 `OWNED_CHILD`의 실제 PID를 받아 다음 조건을 모두 요구한다.

- `/proc/<pid>/stat`의 PGID가 아직 reap하지 않은 leader의 예약 PGID와 일치.
- 수집 전후 stat의 startticks와 PGID가 같은 실제 process identity를 유지.
- `/proc/<pid>/exe` 링크가 canonical `/usr/bin/sleep`이며 실제 dev/inode도 그 파일과 일치.
- `/proc/<pid>/syscall`을 읽기 전후의 state가 모두 `S`이고, 실제 syscall이 `nanosleep` 또는
  `clock_nanosleep`. x86_64의35/230, aarch64의101/115는 해당 Linux syscall ABI이며
  알 수 없는 architecture에는 성공 fallback을 두지 않았다.
- 준비 완료를 받아들이는 시점도 기존2초 deadline 안에 있어야 함.

exe 링크만 먼저 바뀐 ptrace exec-stop은 state/syscall 조건을 만족하지 않는다. 실제 sleep
syscall까지 진행해야 하므로 exec 이후 userspace 실행을 요구한다. `/proc/wchan`이나 임의
시간 지연을 사용하지 않는다. process가 아직 준비되지 않았을 때의 `yield_now` 대기만 있으며,
trace/parser의 unknown을 재시도하거나 건너뛰는 로직은 없다. proc 읽기/identity 실패도 그대로
실패한다. GHA stdout에 실제 PID/PGID/startticks/S/syscall/dev/inode를
`PTY474_SLEEP_READY` 한 행으로 남긴다.

실행 전제는 Linux procfs에서 해당 PID의 `stat`/`exe`/`syscall`을 실제로 읽을 수 있는 것이다.
기존 `set_child_subreaper(Some(Pid::INIT))`에 의해 orphan descendant는 테스트의 직접 소유
자식으로 입양되므로 same-uid attach-read 접근이 가능할 근거는 있다. 그러나 이는 권한,
ptrace 보안 정책과 실제 tracer가 연결된 상태에서의 가시성을 입증한 결과가 아니다.
**untraced와 private-provider traced GHA 각각에서 `/proc/<pid>/syscall` 읽기 및 readiness
receipt를 확인해야 한다.** 가시성 실패에는 fallback이 없고 그대로 실패한다. 현재 수치
분기는 Linux x86_64/aarch64 ABI에 한정하며 다른 architecture나 kernel/security 설정에
이식 가능하다고 주장하지 않는다. aarch64 기능 검증도 UNRUN이며 `/proc/wchan`은 사용하지 않는다.

기존 post-drop cleanup의2초 deadline 생성 위치를 barrier 앞에 옮겨 readiness와 cleanup이
같은 한도를 공유한다. 새 대기 한도/timeout 연장은 없다. 준비가 실패하면 unrelated child를
생성하기 전이며, 소유 `Generation`의 원래 Drop 회수는 그대로다. 이후 unrelated `/bin/sleep`,
`Generation::drop`의 HUP→KILL, leader exit23/Z/reserved PGID/reap, descendant 제거와
unrelated child 생존/최종 kill·wait assertion은 모두 원문 그대로다.

selector는 기존
`terminal::pty::tests::exited_leader_remains_reserved_until_owned_descendant_cleanup`,
이 파일의 `#[test]` 수는1→1이다. closure18/protocol3/context3/PTY2의 전체26개 구성과
full8×2, source-authored program roster/strict root auditor, tracer raw 출력은 바꾸지 않았다.
WF/adapter/preparer/provider patch/fixture/pin/Cargo/production 변경은0이다.

### source-only 확인과 부모 GHA 인계

새 helper와 barrier/deadline 이동을 제거·역투영한 파일은 패치 작성 기준 HEAD `51fd430f76dd6bd28776ac1fbd04c16955bd977e`의
기존3,525B와 byte 동일하다. 그 원문 SHA256은
`b54b1533523b58052352f4bdb92990929bc0f6ba81fe22c9ae2c1d1f273b0942`다.
새 파일6,212B의 SHA256은
`1c37bf09742aa37ccbe671a781f686dec5acefddeccf7b06c79f948435387223`이다.
Rust2021 rustfmt로 전체 source의 구문을 확인하고 새 함수만 서식을 대조했다. 기존 부분에는
전체 rustfmt를 적용하지 않았다. source/diff 확인 외 로컬 Cargo/compile/test/helper 실행은0이다.

현재 발행 중인 `51fd` recovery run에는 구 PTY 코드가 들어 있다. 동일 readiness 부재에 따른
경합 실패를 예상하며 새 barrier의 검증 run으로 세지 않는다. 실제 결과를 확인하기 전에 실패로
확정하지 않으며, 이 후속에서 해당 run을 cancel/rerun/dispatch하지 않았다.

부모는 다음 source batch의 기존 recovery build/select 이후 동일 helper 호출로 실제
private-provider PTY2와 전체 lane을 검증한다. 새 registration이나 승인 program 추가는 필요 없다.
다음 명령은 모두 **GHA 전용**이며 이 작업에서 실행하지 않았다.

```bash
cargo test --manifest-path rust/Cargo.toml -p openpilot-carrot-recovery --lib --locked terminal::pty::tests::exited_leader_remains_reserved_until_owned_descendant_cleanup -- --exact --test-threads=1 --nocapture
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-carrot-recovery --lib --tests --locked -- -D warnings
# 기존 cohort build/select 완료 뒤, 실제 patched provider/STOP 승인 아래 같은 selector를 검증:
/usr/bin/bash "$GITHUB_WORKSPACE/tools/ci/recovery_tracer_474.sh" pty
```

actual readiness receipt/CWP cleanup/PTY STOP·CONT와 full traced8/strict post/direct-root 및 최종
봉인이 모두 필요한 수용 증거다. 다른 종료 경로에서 미확정 exec가 나오면 원래 strict 거부를
유지한다. 이 통제의 동기화 수정으로 원래 PTRACE_LISTEN이나 모든 종료 경합이 해결됐다고
주장하지 않는다. 새 source의 실제 기능/Clippy/자원 fit는 **GHA UNRUN**이다.

부모 검토 후 이 PTY source와 문서를 batch12 인계 상태로 동결했다. 부모가 SSH595의 별도
한 경로 수정과 함께 통합한다. 추가 testcase/count/selector 변경과 stage/commit/push는0이다.
새 genuine compiler WF는 다음 batch의 부모 작업이며 이 후속에서는 읽거나 수정·stage하지 않았다.

## source3f532 실제 성공과 원본 artifact 감사

2026-10-26 후속 읽기 전용 감사다. 앞 절의 실패/UNRUN은 각 source 당시 기록이며,
이번 결과는 source `3f5329dcf76f461bde804d09dfef0c4e5980893b`의
[run40240111863/job121309898098](https://github.com/bin9208/openpilot-rust/actions/runs/40240111863/job/121309898098)에
한정한다. API의 job/모든 step 결과는 success다. 사용자 최신 지시에 따라 `51fd` 성공 run의
추가 artifact 감사는 생략했다. 부모의 Focus/Fast8 감사와도 범위를 분리했다.

원본 artifact는 `recovery-native-lanes-40240111863-1`, ID `12616648753`이다.
ZIP90,226,398B, unique/safe/CRC 정상14,107 members, expanded315,212,974B를 확인했다.
raw 파일을 추출·정규화하거나 실행하지 않고 ZIP member bytes를 직접 읽었다.
보관 경로는 `.analysis/scratch/2026-10-25-523-completion/ci/12616648753.zip`이다.

| 원본 산출물 | SHA256 |
| --- | --- |
| artifact ZIP | `3c2d6dd70c34118248cff2fb53e1a4513a3a513620c69aaa4ddbbc51f3383f57` |
| 실제 private strace ELF | `5c48a2f14563b3b4a7d7a611ea338f2a92a20be06aca34e062933b99cfb4b012` |
| 실제 approve-launch ELF | `74ed503c1527fa822e9d3e63d9ff78a52352413c7dab219122d6d386267ea71b` |
| `recovery-trace/exec-preflight.json` | `acd9823d1541389c0acd73b7c815b83aa025fb9a1b00aa1c593fb98e6b987bdc` |
| `recovery-trace/exec-audit.json` | `60967023925f129b692ec3d653f9236ee469e9073810dbcd6500f01f4684bd51` |
| `recovery-trace/direct-root-audit.json` | `202328de94123edd60af062b03ffe93665ca53654117299eaf2d827fb7c9cd7c` |
| 실제 CWP `pty/pty.exec` | `e6f4a0f457f5b6ba13338dd4a6a4e18df0bd169a2496abef1ff4863efa1172b0` |
| 별도 `pty/pty-stop-cont.exec` | `06fd1de34a0aa8ad2f918d3d685552d5a77296403b2f9e12852e23ac71cac788` |

### 예정26개와 실제 실행 구분

| 대상 | 원본 evidence에서 확인한 실제 결과 |
| --- | --- |
| context 음성3 | 정확한3행 PASS와 command/direct wait 각각1/1. provider 준비는0/0 |
| 원래 closure18 | 7양성/4binding 음성/7context 음성의18행 모두 PASS. 추출 recipe11,145B SHA는 기존 `6cc97850…`와 동일 |
| 원래 STOP/CONT protocol3 | childless0/0, original exit7/direct wait7, genuine live member 거부1/회수 wait137의3행 PASS |
| 실제 CWP HUP→KILL PTY1 | 고정 selector의1 test PASS, tracer/direct wait0/0. readiness와 원래 cleanup assertion 모두 실제 출력됨 |
| 실제 controlling PTY STOP/CONT1 | C probe compile0/0, 실행0/0. raw SIGSTOP/SIGCONT와 child 직접 reap0 PASS |

따라서 **예정26개가 실제26개 모두 통과**했다. marker10/cold 본체 통제는 이 recovery job의
대상이 아니며26개에 끼워 세지 않았다. 준비 시점의 `provider.txt`에 남은
`actual_cold_run=false`/`controls18_run=false`도 바꾸지 않았다. 후속 통과는 별도의 실제
통제 receipt와 완료 marker로 확인했다.

CWP stdout의 실제 준비 완료 행은 다음과 같다.

```text
PTY474_SLEEP_READY pid=36145 pgid=36142 start=64801 state=S syscall=230 dev=66305 ino=1951
```

동일 출력의 leader36142는 Z 상태/exit23로 예약돼 있었고, cleanup 뒤 leader_reaped=true,
descendant_after=null이다. 별도 probe는 child36394의 실제 SIGSTOP 뒤 parent36391의
SIGCONT와 `RECOVERY474_PTY_STOP_CONT PASS ... reaped=0`를 남겼다. 세 launch의 binding은
모두 실제 source3f532/common library `1d5d0ab9…`/위 provider ELF SHA와 exact timeout
parent/argv 검증에 결합돼 있다. 추가 승인 program을 full trace에 넣지 않았다.

### untraced8, traced8 및 원본 reference 범위

untraced harness는 exit0/181.40s, traced harness는 exit0/182.70s이며 driver도0이다.
각 native admission은 `pass=true`, failures는 빈 배열이다. 각 lane의 checkout/private
두 root에서 http/terminal/lifecycle/support/shutdown/actions/upload/cwp8개 family의
result/admission이 모두 `pass=true`다. 동일 worker ELF6개의 digest가 양 lane에서 같고,
harness를 포함한 선정7개의 전후 stat/fixed-ctime 대조도 통과했다.

현재 WF의 `RECOVERY_FAMILY_HISTORICAL_PRESENT=0`에 따라 checkout/private는 모두
역사적 helper13개가 없는 경계다. selector 이름에 present/absent가 남아 있다는 이유로
이번 run에서 helper13개가 존재하는 추가 조합까지 실행했다고 주장하지 않는다.

원본 기대값은 `recovery-original/observations.json`의 data-only frozen fixture다.
SHA256 `db82220fc911ad6ee0d0b960a7c4f1bf7665c6e6d3404fc3c9135d80eb1495e9`,
source `73cb4466e9ee9a426911d4ba0c0040e68d988c5f`, 원본 artifact12367729913/
ZIP SHA `73dbb9fac31c64ad61e3e08c19ac965571834ed036c8222c1e2c936573c38eba`에 결합된다.
fixture213개 decoded bytes의 길이/SHA를 inert하게 대조했고 source input15/missing0도
확인했다. 213개는 source-observation207, controller-execution-receipt1,
native-only-controller-contract5다. 이를 모두 Python 원본 관측으로 묶어 주장하지 않는다.
이번 GHA가 기존 Python 원본을 다시 실행한 것은 아니다.

### strict audit와 직접 root, 전후 봉인

native strict audit는 passed=true이며 attempts5,033개 중 Success2,393/명시 Errno2,640이다.
raw trace2,510개/2,910,370B/7,146행의 파일 집합·길이·SHA가 native receipt와 하나도 빠짐없이
일치했다. raw의 `???`/`= ?` 표시는0개다. preflight 원문 SHA도 보관한 receipt와 일치하고,
audit의 admission SHA는 실제 native admission bytes와 일치한다.

full traced8의 실제 root37109는 첫 exec(line1)에서 원래 harness/고정5개 argv/Success다.
direct-root passed=true, trace_modified=false, extra_program_rule=false다. approval fixture와
PTY probe executable은 이 full trace의 attempt에 없다. inheritance receipt는
root_seccomp1/fresh2509/inherited2509/additional_ptrace0/parent_link_observed0이며
observer도 seccomp5033/exit5033/io_error0/overflow0이다. 이번 감사는 native parser를
로컬에서 다시 실행한 것이 아니며, 기존 native receipt와 raw bytes를 대조한 것이다.
7a928→3f532의 strict parser/policy/common launcher/fixture/provider patch source diff도0이다.

provider의 after-build/after-cold/after-fast에서 SDK 전체, provider inputs/source/build/bin,
tracked source/index/stage, compiler input tree6개, tool/ELF/DSO120행, ld cache와 outer
admission receipt의 before/after813쌍을 byte 대조했다. workflow SDK165 files/programs/전체
roster, index와 source seal도 전후 동일하다. source tar digest는
`813c30613d51794fc0603ee4b52c4d4032a0d086953a725a8480304e0af0fc53`다.
native program seal25쌍도 전후 동일하다.

held FD/path/stat/SHA의 고유 행 집합은 prepare6/controls78/PTY32/trace16/after7 각각
전후 동일했다. controls after156행은 각 행의 두 차례 검증이며 다른 값은 없다.
모든 phase status는 original_exit0/finalizer_exit0/owned_groups_nonlive=true다.
artifact upload ID/digest가 이번 ZIP과 일치한 뒤 source cleanup과 common job cleanup
모두 실제 step success다. archive는 cleanup 전 증거이며, cleanup 성공은 API/job log와
소유성 검증을 포함한 원래 run body에 근거한다.

실제 provider runner는 GitHub Actions1000029676/github-hosted/Linux/X64,
kernel6.17.0-1022-azure다. sampled provider/proof 최대 allocated224,124,928B,
apparent180,193,703B는512MiB 이하이고 after-fast available86,618,689,536B였다.
이 한 recovery 실행의 관측이며 Fast7분/6GiB 또는 다른 runner/장치의 peak 보장이 아니다.

### 남은 증거와 이번 기록의 한계

readiness marker는 **private-provider traced CWP PTY**에서 확인했다. untraced8의 CWP
family는 별도 `cwp::tests::child_driver`를 사용한다. 양 lane native evidence에 standalone
`PTY474_SLEEP_READY`는 없으므로, untraced8 PASS를 readiness barrier 자체의 untraced
`/proc/<pid>/syscall` 가시성 증거로 확대하지 않는다. 해당 exact unit의 별도 untraced GHA
증거와 aarch64/다른 보안 설정의 이식성은 여전히 미확인이다.

이 recovery job의 Clippy는 dev-tools lib/audit CLI/recovery_family_frozen 대상이다.
그 성공을 변경한 carrot-recovery PTY test module 자체의 strict Clippy 성공으로 바꾸어
주장하지 않는다. 부모의 별도 Fast 검증 결과와도 합쳐 추정하지 않았다.

이번 감사에서 새 실행 source 결함은 발견하지 못했다. 기존 기록 끝에 이 절만 추가했고
코드/WF/preparer/pin/Focus/원장 수정은0이다. 로컬은 source/원본 ZIP의 CRC/SHA/JSON·텍스트
및 diff 검사만 수행했다. 프로젝트 CLI/테스트/Cargo/strace 실행과 GHA dispatch/cancel/rerun,
stage/commit/push는0이다. 한 실행의 성공으로 원래 PTRACE_LISTEN 문제의 전역 해결 또는
#474/#560 종료·퇴역 credit을 주장하지 않는다.
