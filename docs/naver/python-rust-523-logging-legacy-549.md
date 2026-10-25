# #549 shared logging legacy runtime2 집중 후보

2026-10-25 KST. 이 기록은 `tools/ci/logging_retained_549.sh legacy-runtime2`의
source 구현과 부모 WF 연결 계약을 설명한다. **기능은 GHA UNRUN**이다.
원본2 삭제·old27/R4 승인·retirement credit은0이며 기존 exec 문서는 수정하지 않았다.
로컬 원본/CLI/테스트/빌드/Clippy/컴파일은 실행하지 않았다.

## 명시 mode와 기존 retained 범위

- 무인자는 기존 retained recipe다. 원래 core21/import9/trace24/summary7의 기대61,
  lifecycle14와 무계측1의 기존600초, backpressure2초, retained-once/inspect15와
  exact policy의 기능 명령·본문·선택·상한을 유지한다.
- 성공 끝에 cross-call용 `proof/logging/legacy-reuse.json`,
  `legacy-reuse.roles.jsonl`, `legacy-reuse.held.tsv`를 추가한다.
  실행 성공 판정은 기존 EXIT 후검사를 지난 `outcome.json`과 함께 읽어야 한다.
  receipt 하나만 있거나 원래 실행의 phase/exit/held/paired/untraced 중 하나라도 실패하면 재사용을 거부한다.
- 인자1개 `legacy-runtime2`만 새 mode다. 다른 인자·추가 인자는 exit64로 거부한다.
  출력은 새 `proof/logging-legacy-runtime2`에만 쓰며 기존 retained proof를 덮어쓰지 않는다.
  같은 mode의 재호출도 디렉터리/새 builder target이 이미 있으면 실패한다.
- mode는 기존61/retained14/once/old17/collision8을 반복하지 않는다.
  제안했던 command-literal 동등 정책·14 mutation test는 적용하지 않았다.
  shared main/WF/Focus/ledger/Cargo도 이 worker가 편집하지 않았다.

## checker/probe/fixture의 정당한 같은-job 재사용

같은 target에서 다시 Cargo build/test를 호출하면 `fresh:true`가 될 수 있다.
이를 새 compile이나 fresh provider qualification으로 쓰지 않는다.

무인자 실행이 실제 선택했던 checker/probe/fixture JSONL의 exact target/kind/src_path/profile,
원래 executable 경로 및 JSONL SHA를 receipt에 묶는다. provider는 해당 실행이 hold했던
실제 dev/ino/owner/mode/nlink/size/mtime/ctime/SHA와 연결한다.
probe는 Cargo original과 private executable copy의 경로/identity를 따로 기록하고 hash equality를 유지한다.
source SHA/root identity, run/attempt/job owner 및 prior proof root identity도 고정한다.

`legacy-reuse.held.tsv`는 전회 provider/source/DSO의 held-before 행을 복사한 bounded snapshot이며
그 SHA를 receipt가 고정한다. snapshot·receipt 자신의 행은 그 뒤 original held 목록에 추가된다.
다음 mode는 pin된 snapshot의 **모든 행**을 원래 identity/hash와 대조해 새 FD로 hold한다.
최초 FD가 두 Bash 프로세스 사이에도 열려 있었다고 주장하지 않는다.
`prior_fd_continuity_claimed:false`, `historical_reuse:false`를 명시한다.

새 mode가 원래 JSONL의 고유 Cargo target을 다시 읽어 receipt의 path/hash/identity와 맞추지만
checker/probe/fixture에 Cargo를 다시 호출하지 않는다. 이전 Cargo의 `fresh`는 boolean인지 확인할 뿐
그 값을 `false`로 고쳐 쓰거나 현재 compile으로 바꾸지 않는다.
성공 끝까지 도달하지 못한788의 exec-policy failure 자료에는 이 재사용 권한이 없다.

## fresh builder/reference와 기존 자원 상한

새 builder는 `CARGO_TARGET_DIR/legacy-runtime2-builder`라는 **비어 있던 private target**에서
`native-logging-check-native` feature와 원래 lock으로 한 번만 빌드한다.
exact `build-native-logging-reference` artifact1과 성공 Cargo EOF를 고르고 `fresh:false`를 요구한다.
SDK/registry는 현재 job의 기존 경로를 사용하며 rustup/install/chmod/fallback을 추가하지 않는다.

새 target을 기존 target 아래 두었으므로 기존 `admit`의 단일 `du -sx`가 두 build 영역을 포함한다.
기존 target8GiB/proof2GiB/command output512MiB와 실제 free-space admission을 그대로 쓴다.
두 target의 할당량을 따로 더해 hardlink를 중복 집계하거나 새 target을 집계에서 빼지 않는다.
실제 성장량·75분 job 내 적합성은 아직 측정되지 않았다. 초과 시 strict failure이며 상한 확대·자동 B 전환은 없다.

현재 source의 `native_logging_build.rs`, `json11_stage.rs`, `uv.lock`,
swaglog.cc/h·timing.h·native_logging_reference.cc를 build 전부터 hold한다.
reference는 원래 builder CLI로 한 번 fresh build하며 `--zmq-include /usr/include --architecture x86_64`를 명시한다.
고정 json11 wheel42604B/SHA
`a309d401dd22b758455dafd3c28b72c4bd88c7482ddd70fe8653e53efc745a83`,
정확 source4 map·compiler command·version/architecture를 기존 collision8 recipe와 같은 방식으로 확인한다.
새 C++ ELF와 loader/DSO, 실제 compiler/cc1plus/collect2/as/ld/curl/pkg-config 및 link data를 hold한다.
compiler alias identity/target, `/usr/include`와 실제 gcc provider tree도 전후 대조한다.
과거 d540 reference나 R11 ELF를 실행하지 않으며 현재 reference SHA를 historical pin으로 바꾸지 않는다.

## legacy runtime-directory INT/TERM2의 원래 실행

mode 내부에서만 secret-free `env -i` whitelist와 다음 env를 명시한다.

```text
NATIVE_LOGGING_TEST_BINARY=전회 실제 선택·hold한 native probe
NATIVE_LOGGING_TEST_BUILDER=이번 current fresh builder
NATIVE_LOGGING_TEST_REFERENCE=이번 current fresh C++ reference
NATIVE_LOGGING_LIFECYCLE_EVIDENCE=proof/logging-legacy-runtime2/lifecycle
NATIVE_LOGGING_LIFECYCLE_RESUME=39833849895
```

QA_ESCAPED/retained env·token·임의 Git config를 넘기지 않는다.
현재 qualification의 fixed-true preload profile을 명시하고, 과거 false receipt를 수정하지 않는다.
fixture는 기존 Cargo ELF를 아래 exact selector로 한 번만 실행한다.

```text
actual_cli_int_term_reap_held_producer_and_preserve_runtime_foreign_replacement
```

`--ignored --exact ... --test-threads=1 --color=never`를 사용한다.
RESUME에 따라 **legacy runtime/directory INT·TERM2만** 남는다.
1 passed/0 failed/0 ignored 요약과 exact test 이름을 요구하고,
evidence immediate roster가 정확2개의 private case root인지 확인한다.
각 원문 `case.json`의 runtime/directory, returncode130·143 각1개, runtime endpoint,
실제 peer reap/owned private directory 제거를 검사한다.

원래 test body의 live C++/Rust two-role 준비, actual checker/peer PIDfd,
STOP→foreign directory replacement→checker signal 순서,
checker130/143·finish20초·foreign dev/ino 보존을 수정하지 않았다.
원래60초 runtime barrier/15초 hold 및 transport2초도 그대로다.
추가로 record 측정치에 새20초 기준을 걸지 않고 원래 `Checker::finish`의 bound를 유지한다.

runtime2 selector 한 번에는 **별도600초 aggregate**와 monotonic start/end를 둔다.
reference build는 그 selector 전의 별도 준비이며 기존 retained14의600초 구간을 밀거나 늘리지 않는다.
원래 full syscall classes·`-ff -ttt -x -v`·raw read/pread64·EXITKILL/BPF requested를 유지한다.
reference-build와 runtime2 raw prefix는 같은 strict inspector로 검증한다.
unknown line은 거부하며 activated BPF/closed PID/cwd/FD/DSO/full semantic acceptance를 주장하지 않는다.
현재 mode는 legacy runtime의 **기능 case2와 lexical/immutable 증거** 후보이고,
retained 전용 exact15 policy를 legacy runtime에 잘못 적용하거나 collision의 zero-child 정책을 대입하지 않는다.

## 실패 후검사와 부모 WF 연결

mode의 EXIT에서 실패 때도 source/index·SDK whole file manifest/roster·compiler/header tree·held files·
provider alias 전후 검사를 시도한다. 원래 command failure는 유지하고 후검사 실패도 최종 실패다.
`complete.json`은 최종 EXIT보다 먼저 생길 수 있으므로 반드시 다음을 함께 요구해야 한다.

- `proof/logging-legacy-runtime2/outcome.json`: mode가 legacy-runtime2,
  phase가 legacy-runtime2-observed, `exit_code`, `source_sdk_final_exit_code`,
  `source_final_exit_code`, `sdk_final_exit_code`, `held_final_exit_code`,
  `provider_alias_final_exit_code`, `resource_final_exit_code`가 모두0.
- `complete.json`: current source SHA·exact selector·logical_cases2/libtest_passed1,
  fresh current reference path/SHA, old17/collision8/retained14 재실행false,
  old27/R4 false·retirement0.
- 기존 retained `outcome.json`의 성공 및 재사용 receipt/provider pin.

부모 WF에는 기존 `qualify` 성공 뒤 **별도 step**으로 아래 mode 호출을 연결한다.
trusted-source 조건을 유지하며 같은 private source/SDK/Cargo 경로에서 실행한다.

```bash
/usr/bin/env -u LD_PRELOAD -u LD_LIBRARY_PATH /usr/bin/bash tools/ci/logging_retained_549.sh legacy-runtime2
```

부모는 이 step에 기존 job75분 안의 유한 outer timeout/회수 정책을 연결해야 한다.
source_final/SDK final/upload는 실패에도 유지하고 cleanup은 **기존 조건 AND 새 mode 성공**이어야 한다.
새 proof namespace의 outcome·reuse checks·Cargo JSONL·provider/reference/source/index/SDK seals,
원문 case2·trace2prefix/summary를 upload/필수파일/실제 raw SHA 검증에 포함해야 한다.
기존 cleanup의 observed PID 확인에도 두 새 prefix의 raw PID roster를 추가하고,
old retained15 전용 policy 파일을 새 legacy prefix에 요구하지 않아야 한다.
추가 target은 기존 owned source target 안에 있어 기존 source/job root cleanup 범위를 벗어나지 않는다.

원본2 삭제는 새 maintained caller 실제 성공과 이 runtime2 별도 판정 뒤다.
old17의 current carry와 historical CPP identity bridge는 별도 미완료이며,
case2 실행/strict lexical 성공을 old27/R4/전체 adopted/DSO closure로 승격하지 않는다.

## 검증 상태

로컬에서는 Bash syntax·jq filter compile-only·diff 및 기존 retained 본문의 byte 보존만 확인한다.
새 mode의 C++/Rust compile·기능 case2·실제 provider reuse·상한 적합성·trace 형식·upload/cleanup은
GHA에서 실행해야 하며 이 문서의 수치는 기대 선택/상한이지 새 성공 결과가 아니다.

source 구현 시 확인한 정적 결과는 다음과 같다.

- 추가 parser/proof 선택/function/분기/끝의 publication 호출만 제거하면 기존 helper
  **66901B/SHA256 `53f8108dfc715ca59fc0ab1dcd2b16ba20994ec3025f119a35f79e0813d78a3b`**와 byte 동일하다.
  원래 run/admit/hold/elf/artifact 함수 및 retained recipe의 본문도 유지했다.
- `bash -n`과 `git diff --check`를 확인했다. 새 jq 필터16개는 `if false then (...) else empty end`로
  실제 입력·분기를 평가하지 않는 compile-only 검사만 했다.
- 새 libtest/생산 seam/feature/bin/원문 expected 추가0이다.
  기존87 정책·2048MiB main guard·functional tests는 부모의 기존 gate에서 그대로 검증한다.


## maintained caller의 함께 적용한 source 후보

같은 source에서 main `logger-runtime`의 원래2048MiB guard와 builder2개 빌드를 그대로 둔 뒤,
기존 feature `native-logging-check-native`의 `check-native-logging` build1개를 더한다.
바로 다음 단계의 Python 명령1곳을 기존 Rust legacy CLI로 바꾸며 같은 current probe/builder/output과
명시 source root를 넘긴다. fresh C++와 Rust의 console/rate/transport 전체 비교를 유지하는 경로다.
retained의 CPP0 scope로 바꾸거나 원본 비교를 줄이지 않는다. 다른 logger/encoder/uploader 단계,
44개 job·fast42/native8·source/SDK/upload/정리 조건은 이 caller 변경에서 그대로다.

전체 main을 다시 직렬화하지 않고 해당 두 run span만 바꿨으며 parsed whole-model이 정확한
before/after 두 step fragment와 같다. main source는489,835B에서490,063B다. 구현 문자열을
중복하는 새 policy test는 추가하지 않고 기존87개 정책·원래 functional 검사와 새 legacy2 actual을
검증으로 사용한다. 독립 main/helper/WF source 검토 P1/P2 없음과 구문/format/diff만 확인했다.
현재 source의 실제 caller·2case·75분 내 자원 적합성·최종 cleanup은 모두 GHA UNRUN이다.
