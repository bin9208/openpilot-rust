# #560 closed-launch 공통 함수 추출

초기 추출 이후의 실제 Fast caller/native admission은
[Fast 연결 기록](python-rust-523-fast-native-cold-560.md)에 있다. 아래의 추출 원문·pin은 유지된다.

작성: 2026-10-26. 추적: [#560](https://github.com/bin9208/openpilot-rust/issues/560).
읽기 HEAD: `dd6157c3984a73a59fb808b551cdab73f04dba35`.
상태: **launch 추출 구현 완료, GHA18/actual cold/Fast fit UNRUN**.
provider preparation 공유는 미완료다. 기존 preparation과 원래 cap을 바꾸지 않았다.

## 실제 변경과 source-only 불변 증명

`tools/ci/cold_tracer_launch_560.sh`에 다음 **12개 함수의 이름·본문을 byte 동일하게**
옮겼다. 승인기에 필요한 held FD와 소유 PID/PGID·stopped leader 함수까지 포함하여
Fast caller가 동일한 승인 경로를 사용할 수 있다.

```text
stamp hold verify_held
pid_identity kill_owned group_failure group_quiet await_stopped
approve_launch inherited_command start_owned complete_owned
```

공통 파일의 top level에는 주석과 위 함수 정의만 있다. source할 때 설치·명령 실행·
프로세스 생성·파일 읽기/쓰기·trap/set/umask/환경/배열 초기화를 하지 않는다.
실행 시 사용하는 상태와 예산은 caller가 명시적으로 준비한다. 원래 body 안의
command와 음성 통제 처리는 호출할 때만 실행된다.

diagnostic은 원래 첫 함수 정의 위치에서 새 library를 다음 절차로 읽는다.

1. canonical regular nonsymlink 경로의 metadata를 저장하고 FD를 연다.
2. path와 열린 FD identity가 같은지 확인하고 **그 FD bytes**에 literal SHA256을 검사한다.
3. 같은 FD를 source하고 path/FD metadata를 다시 대조한다.
4. 원래 `hold`로 library의 별도 FD와 full SHA/metadata를 보유하고, 그 보유값도
   원래 identity·검토한 SHA와 같은지 확인한 뒤 loader FD를 닫는다.
   이 held entry도 기존 finalizer의 전후 봉인에 참여한다.

이를 제외한 caller source는 바꾸지 않았다. 원본 Git blob
`a8c80cad02466ecaf9110d4e3e4dfaec9c136df6`에서 네 연속 함수 구간을 추출했다.
현재 diagnostic에 네 구간을 다시 넣고 15행 loader만 제거하는 **역투영 결과가
원본 전체 bytes와 동일**함을 source-only로 확인했다.

| 원본 연속 구간 시작 | bytes | SHA256 |
| --- | ---: | --- |
| `stamp` | 1798 | `eee805c18bdf0bf133943de16703b427c0321f9cff8e0d70413a421da1dd1e4b` |
| `pid_identity` | 3294 | `867da407038cec203bbec60a70169b58e404ffdcb3cf5c93645ceaa23c067f77` |
| `approve_launch` | 5911 | `0072e1617fa59e0bafd24e85db5ea4257425a601073b0f16b821940f02135147` |
| `start_owned` | 3883 | `7b07b5fbadce35ae29ed880e90b0aa849cf9017d820a097d8e8a10adad4d0dab` |

원본/역투영 전체 SHA256은 모두
`b3e7266fdc4d7275c7e9ef58eeb4fda66070bd09ab8eb6e5985974ad3a38f4d5`다.
공통 파일의 주석 다음 bytes는 이 네 구간의 순서 있는 연결과 구간 사이 빈 줄뿐이다.
즉 top-level 새 실행 코드가 없고 원래 함수 구현을 새로 쓴 것도 아니다.

별도로 원본의 `inherited_controls`, marker10, protocol3, 공식 download~configure/make
preparation의 전체 구간도 현재 caller에 byte 동일하게 남았음을 확인했다.
따라서 7양성+4binding음성+7context음성의 **18개/순서/body**, marker10/body,
protocol3, observer/strict auditor body, 원래1200초 shared deadline·각 timeout·
caps·실패 sticky·owned KILL/direct wait는 변경하지 않았다. library FD 하나의 추가
봉인과 함수의 파일 위치·source commit/실제 helper SHA는 새 실행에서 달라진다.
행 번호나 실제 runtime trace bytes가 이전 실행과 동일하다고 주장하지 않는다.

현재 pin은 다음과 같다. workflow/Focus를 직접 수정하지 않았으므로 부모 갱신이 필요하다.

```text
cold_tracer_launch_560.sh SHA256
1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a

cold_tracer_diagnostic_560.sh SHA256 (새 COLD560_HELPER_SHA256)
8a7383587589d01404f7e6573db8fd1ad458c22fbc6ff2755298b5a4a35ddc4b
```

## 호출 API와 필수 상태

외부 진입점은 다음 positional API다. production Fast는 `pass none`만 쓴다.
나머지 expectation/fault는 원래18개 음성 통제를 위해 그대로 둔다.

```text
inherited_command LABEL SECONDS pass none ABS_PRIVATE_STRACE [STRACE_ARGS...] TARGET [ARGS...]
approve_launch                         # start_owned의 기존 polling에서 내부 호출
verify_held                            # caller의 독립 최종 봉인
kill_owned; wait -f "$active"          # 검증한 현재 owned leader가 남았을 때만 finalizer 회수
```

`SECONDS` 인자는 명령별 요청 timeout이다. **global `deadline`은 Bash 내장 `SECONDS`와
같은 시간축의 절대 종료값**이다. shared 함수는 `deadline-SECONDS`로 요청을 줄이며
상한을 늘리지 않는다. 기존 body의 `1200s diagnostic deadline` 오류 문자열은 그대로지만
Fast가1200초를 받아야 한다는 뜻이 아니다. Fast는 첫 trusted step에서 고정한
원래420초 종료값의 **현재 잔여 시간만** 이 시간축으로 변환한다.

source할 파일은 위 literal SHA를 FD에 검사해야 한다. diagnostic에 있는 15행 loader를
그대로 사용하면 된다. 일반 `source PATH` 후에 사후 hash만 비교하는 형태로 바꾸지 않는다.
library loader 자체를 source한다고 설치나 검증을 자동 실행하는 API는 없다.

caller 필수 globals/functions는 다음과 같다.

| 이름 | 계약 |
| --- | --- |
| `fail MESSAGE` | 반드시 실패 종료. 성공 반환 stub 금지 |
| `admit` | 실제 남은 시간/메모리/작업별 disk 성장 및 기존 cap 검사. `true` stub 금지 |
| `mode` | `run-inherited`. 이 값은 승인 경로 선택이며 runner 환경 이름이 아니다 |
| `source`, `job`, `sdk`, `out`, `build` | 실제 checked source, owned temp, 실제 prepared SDK, fresh proof/work root |
| `deadline` | caller가 원래 종료값으로 고정한 `SECONDS` 기준 deadline |
| `tracer` | canonical absolute **실제 private ELF** 경로, `/usr/bin/strace` alias 금지 |
| `fixture` | unchanged `cold560_exec_inheritance.rs`를 실제 SDK rustc로 만든 current private approver ELF |
| `COLD560_HELPER_SHA256` | 실제 호출 코드의 검토된 SHA. diagnostic은 새 자기 SHA, Fast의 공통 구현 binding은 아래 설명 참고 |
| `GITHUB_SHA` | 실제 exact source SHA, 재사용 과거 SHA 금지 |
| `base_env` | target에 전달할 실제 source/SDK/runner 환경의 array. Fast는 jobs1·실제 self-hosted 정보를 사용 |
| `extra_env` | 추가 환경 array. cold에서는 원래 observer log와 prepared SDK binding을 포함 |
| `launch_strip` | 원래6개 `-E` 항목 array. target에서 controller용 `COLD560_*`를 제거 |
| `held_fd`, `held_id`, `held_sha`, `aliases`, `alias_id` | 빈 associative arrays로 시작, 같은 caller shell에서 생존 |
| `active`, `identity`, `owned_code`, `owned_waited` | 빈 초기 상태. 동시 command를 겹치지 않음 |
| `phase`, `groups_ok` | 초기 phase와 `true`. proc/회수 실패를 성공으로 되돌리지 않음 |
| `launch_label/dir/fault/tracer/parent`, `launch_seen` | 빈 초기값, `launch_seen=false`. 함수가 command별 갱신 |

caller는 `set -euo pipefail`, `set +m`, `umask 077`, 기존 안전 PATH/locale 및 trap을
설정한다. library는 이 상태를 자동으로 바꾸지 않는다. **동일 top-level Bash process**에서
FD/state를 유지하며 호출하고, command substitution·pipeline·subshell 안에 launcher를
넣지 않는다. `$$` 기반 /proc FD·PPID 검사가 다른 shell에서는 성립하지 않는다.

`out`, `out/logs`, `out/inheritance` 및 `build/tmp`는 caller가 새 소유 root에서 준비한다.
label은 한 번만 사용한다. library는 `mkdir "$out/inheritance/$label"`, `ready`,
`approve`의 create-new 검사로 이전 launch namespace 재사용을 거절한다.
기존 프로토콜에 별도 nonce field를 추가하지 않았다. **fresh private job root와
single-use launch directory가 nonce namespace**이며 실제 PID/startticks·source/helper/
provider SHA 및 O_EXCL 승인에 묶인다. stale directory를 지우고 같은 label로 재시도하지 않는다.

## Fast가 연결할 정확 경로

prepare helper의 `provider.env`를 검토된 성공 단계 뒤 읽고, 실제 ELF를 그대로 사용한다.
기존 prepare helper는 수정하지 않았다. 이후 Fast caller가 다음 상태를 만든다.

```sh
# source/SDK/runner/owned roots 검증, 위 SHA를 사용한 FD loader 완료 뒤 실행한다.
mode=run-inherited
tracer=$(readlink -e -- "$COLD560_STRACE")
test "$tracer" = "$COLD560_STRACE"
printf '%s  %s\n' "$COLD560_PROVIDER_SHA256" "$tracer" | sha256sum --check --strict --quiet -
hold "$launch_shared"             # 실제 common library
hold "$tracer"                    # 실제 selected ELF
hold /usr/bin/timeout
hold "$fixture"                   # 실제 compiled approver
hold "$source/.github/workflows/fast-checks.yml"
# Fast에서는 실제 공유 구현 SHA를 사용한다. 과거 diagnostic SHA를 사칭하지 않는다.
COLD560_HELPER_SHA256=${held_sha[$launch_shared]}
test "$COLD560_HELPER_SHA256" = 1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a
launch_strip=(-E COLD560_LOG -E COLD560_FILTER_INHERIT -E COLD560_LAUNCH_DIR \
  -E COLD560_SOURCE_SHA -E COLD560_HELPER_SHA -E COLD560_PROVIDER_SHA)
extra_env=(COLD560_LOG="$out/observer.log" \
  OPENPILOT_CI_PREPARED_RUST_ROOT="$sdk" \
  OPENPILOT_CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST")
inherited_command cold "$cold_seconds" pass none "$tracer" \
  --debug --seccomp-bpf -f -qq -s 65535 -e trace=execve,execveat \
  "${launch_strip[@]}" -o "$job/proof/prepared-cold-bootstrap.exec" \
  "$source/rust/tools/check-disk-space" --path "$source" --growth-mib 2048
```

`cold_seconds`는 원래 caller deadline 안에서만 정한다. `base_env`는 diagnostic의 실제
배열 구조를 따르되 self-hosted에서 `RUNNER_ENVIRONMENT=github-hosted`나 private hosted
SDK 경로를 쓰면 안 된다. `CI_JOB_TEMP`, 실제 `GITHUB_*`, `RUNNER_*`, 실제 absolute
`RUSTC/RUSTDOC`, prepared SDK manifest, private `CARGO_HOME/RUSTUP_HOME`, `TMPDIR`,
`RUNNER_TEMP`, jobs1/incremental0/dev-test debug0, GIT 안전 설정을 보존한다.
array는 순서 있는 NUL argv proof에 들어가므로 문자열 eval/재구성으로 넘기지 않는다.
fixture source SHA는 기존
`e012d523f41a1a51cb49fd05cd334e04085ab562873f17303fffff6b10ad15fd`를 확인하고,
Cargo/cold guard를 미리 실행하지 않는다.

함수가 원래 `setsid Bash → timeout --foreground → env -i → 실제 private strace`를
생성한다. target startup 전 provider가 SIGSTOP하고, 기존 approver가 실제 chain의
ELF/device/inode·PID/startticks/PPID/PGID/session·정확 tracer/timeout argv·SHA를 검증한다.
성공한 O_EXCL 승인 뒤 같은 tracer를 CONT한다. 따라서 단순
`COLD560_FILTER_INHERIT=1 strace ...`나 수동 approve 파일 작성은 대체 API가 아니다.

성공/실패와 무관하게 caller가 기존 finalizer에서 현재 소유성을 확인한 회수,
`verify_held`, prepared SDK/source/index/provider 전후, observer health, 실제 cold raw와
strict auditor, artifact 및 조건부 owned cleanup을 유지해야 한다. library는 marker10이나
실제 cold 감사의 결과를 대신 발행하지 않는다. Fast/native Rust 감사 입구는 부모 담당이다.

## preparation 공유 미완료와 검증 인계

provider preparation은 의도적으로 원래 diagnostic body 그대로 남겼다.
diagnostic의45분 job, shared1200초, configure180초·make300초·`-j2`와 기존 watcher/
hold/resource cap을 새 Fast helper의120초·512 MiB·`-j1`로 바꾸지 않았다.
두 경로를 공통 preparation으로 만들려면 caller별 deadline/caps/parallelism과
기존 phase별 owned run/watch hook을 분리하는 별도 변경이 필요하다. 그 공유는 **미완료**다.
현재 `prepare-cold-strace-560` SHA
`4c45123c13ef4e455d1b31f99fcd503fbd4f03ee827c9c116a51a46a5353fb51`는 그대로다.

로컬 검증은 두 Bash 파일의 `bash -n`, source-only 역투영/byte 비교·diff뿐이다.
공통 파일을 실제 source하거나 프로세스/fixture/provider/Cargo를 실행하지 않았다.
새추출의 GHA18/marker10/protocol3/actual cold·strict audit와 Fast7분/6 GiB/jobs1 fit는
**UNRUN**이다. 기존 성공은 새 extraction의 runtime 수용이 아니다.

부모는 diagnostic helper 새 SHA와 common path/pin을 workflow/Focus에 반영하고 기존
18통제·원래 순서·marker10/protocol3·actual cold를 exact-SHA GHA에서 확인해야 한다.
Fast 연결은 별도로 실제 private provider/공통 구현/fixture proof를 사용한다.
실행 중 CI 취소·재실행, commit/push, Fast/감사Rust/WF/Focus/원장 수정은 이 작업에서0회다.

## 후속 실제 Fast caller — 2026-10-26

후속 사용자 승인으로 `fast_native_cold_560.sh`와 실제 native prepared auditor 입구를
구현했다. 공통 파일과 diagnostic은 이번 연결에서 추가 변경0이다. 원본 전체 역투영
SHA `b3e7266f...`와 기존18/marker10/protocol3/타이머 body 불변을 다시 source-only 확인했다.
새 Fast는 actual self-hosted/global SDK/private homes/jobs1 환경을 사용한다. prepare는
std-only approver compile과 after-cold 검증 phase를 추가하여 현재 pin이 `6c7d99e6...`다.
이전 Fast API 예제의 `provider.env` source 대신, 실제 build-complete와 canonical ELF의
held SHA를 새 caller가 직접 사용한다. 따라서 이 공통 함수는 여전히 source-only library이며
provider 준비/실제 cold/strict audit 성공을 스스로 발행하지 않는다.
부모의 WF/Focus/bootstrap worker1 등록과 exact-SHA GHA18/Fast fit는 미완료다.
