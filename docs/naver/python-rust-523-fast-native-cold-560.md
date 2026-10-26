# #560 Fast private provider와 STOP 승인기 연결

작성: 2026-10-26. 추적: [#560](https://github.com/bin9208/openpilot-rust/issues/560).
시작 시 읽은 HEAD: `561f20d9f2a0d2986b7bf271fabab237b7b3d0e6`.
공유 worktree의 다른 작업은 보존했다. **helper/native admission 구현 완료,
WF 연결·bootstrap jobs1 등록·GHA 수용은 부모 대기, 7분/6GiB fit는 UNRUN**이다.

## 73e17 실제 수용과 7분 분리 수정 — 2026-10-26

[Fast40220990623/job121252313245](https://github.com/bin9208/openpilot-rust/actions/runs/40220990623/job/121252313245)의
`Prepare native CI validation`은 성공했다. 같은 private provider와 STOP 승인, 원래 cold
`check-disk-space`를 strict native auditor가 검증하고 실제 receipt를 발행했다.
artifact12609439585는10,848,136B/SHA256
`bfff5ac697d67cb70d2fae4afbf4fe8214f4415f1084fe073905694c3c6decd1`,
1131개 safe unique/non-symlink member와 CRC 정상이다. 부모가 실제 receipt의 source73e17,
absolute cargo1/rustc37, exec181/실패29, Python/Rustup 실행0, SDK regular294를 확인했다.
source/index before/after도 동일했다. cold의 original/final exit0과 owned groups nonlive=true다.
provider admission의 최대 관측 allocated203,632,640B는512MiB 안이며 전체 peak 측정과 다르다.

그러나 dev-tools 감사 compile163초와 actual audit48.14초 뒤 routing 빌드를 시작할 때
원래7분 job이 취소됐다. after는 원래 deadline 경과를 거부했고 upload는 성공했으나 cleanup은
실행하지 않았다. 따라서 **전체 Fast·7분 fit·정상 회수는 미수용**이며 #530/#560은 열려 있다.
감사 PASS가 timeout/after 실패를 덮지 않는다. 나머지 기존6개 worker는 성공했다.

현재 수정은 동일 감사 파일을 `#[path]`로 사용하는 `openpilot-prepared-cold-audit` 경량 crate다.
감사 구현·11개 일반/1개 ignored·FQName와 strict 기준을 그대로 사용한다. cold job은
self-hosted/jobs1/7분/source2+job4GiB 및 provider120초/512MiB 그대로다. actual 감사 이후의
원래 routing 네 Cargo 호출과 unchanged changed-files preparation은 새 필수
`native-routing` hosted ubuntu-24.04/jobs2/7분 worker로 옮긴다. 기존6개 worker 객체는 동일하고
aggregate가 cold/routing을 포함한8개 결과 전부를 요구한다. routing은 source2+target6,
registry2+SDK3+proof0.5+venv0.5+reserve0.5GiB를 실제 filesystem별로 합산하여 입장하며,
기존6144MiB 추가 성장 검사를 유지한다. 이 예상량은 peak 보장이 아니다.

경량 crate·분리 routing의 실제 compile/기능/시간은 다음 exact-source Actions 전까지
UNRUN이다. 아래의 과거 부모 반영 절차에서 dev-tools 직접 compile와 동일 job routing을
설명한 부분은 이 수정으로 대체되며 strict parser, cap, deadline은 바꾸지 않는다.

## 실제 구현과 호출 경계

- `tools/ci/fast_native_cold_560.sh cold`: 실제 `bin-actions/self-hosted/Linux/X64`,
  `GITHUB_JOB=native`, `SOURCE_ALLOWED=true`, owned private source/job/SDK와 compile
  profile을 검사한다. 현재 target/cache가 비어 있는 상태에서 provider를 준비하고,
  공유 `inherited_command cold 420 pass none`으로 기존 `check-disk-space --path
  "$DIAG_SOURCE" --growth-mib 2048`를 관측한다. Cargo warmup은 없다.
- `tools/ci/prepare-cold-strace-560`: 기존 pinned Ubuntu6.8+observer+inherited patch
  provider preparation을 재사용한다. 변경 없는 `cold560_exec_inheritance.rs`를 실제
  공식 SDK의 absolute rustc로 컴파일하여 `$CI_JOB_TEMP/cold-strace-560/bin/approve-launch`에
  둔다. 이 std-only approver 컴파일과 ELF/DSO 봉인도 **같은120초/512MiB**에 포함한다.
  추가 설치나 apt, 공유 `/usr/bin`/SDK 변경은 없다. runtime의 jq/setsid/sleep 및 rustc도
  기존 preparation 도구/DSO 봉인에 추가했다.
- `tools/ci/fast_native_cold_560.sh after`: maintained routing 검사 뒤 항상 실행한다.
  cold 때 보유한 canonical path/full metadata/SHA를 다시 확인하고 provider 전체,
  source/index, 원래 SDK의 full manifest/roster를 검증한다. `cold` 자체의 EXIT에서도
  동일 범위를 `after-cold`로 검증한다. prepare의 기존 `verify` 기본값 `after-fast`와
  `verify-complete`는 유지하며, 새 `COLD560_VERIFY_PHASE=after-cold`에 별도 1회 marker를 둔다.
  after의 새 held FD 기록은 `final-held/`에 분리하여 native auditor가 소비한 cold의
  `held-before.tsv`/`held-after.tsv`/`cold-held.tsv`를 덮거나 append하지 않는다.
- 실제 감사 파일은 `src/prepared_audit.rs`가 아니라 기존 등록된
  `rust/crates/dev-tools/tests/ci_disk_bootstrap/prepared_audit.rs`다. 새 mod/Cargo 등록은 없다.
  `self-hosted/native` actual receipt에는 private provider admission이 **필수**다.
  환경 flag 생략으로 stock strace 경로로 되돌아갈 수 없다. 기존 hosted diagnostic의
  실제 receipt 경로와 marker schema `openpilot.prepared-cold-bootstrap.native.v1`은 유지한다.

실제 selected path는 `$CI_JOB_TEMP/cold-strace-560/bin/strace`다. 공통 library를 검토된
SHA로 확인한 **동일 열린 FD**에서 source한다. source할 때는 함수 정의 외 부작용이 없다.
unchanged approver가 STOP 중인 tracer와 timeout의 실제 ELF/device/inode,
PID/startticks/PPID/PGID/session, 정확한 두 argv, source/common/provider SHA를 검증하고,
fresh private directory의 O_EXCL 승인 뒤 같은 tracer를 CONT한다.
`COLD560_FILTER_INHERIT=1` 단독 실행이나 승인 파일 수동 작성은 사용하지 않는다.

native admission은 실제 private ELF 전체 hash와 metadata를 직접 읽는다. preparation의
bin roster에 기록된 inode/mode/UID/GID/nlink/size/nanosecond mtime/ctime도 대조한다.
보유 전후 roster, source/index/SDK 전후, provider의 inputs/source/build/bin 전체 봉인,
ready/approve byte 일치, binding/승인 출력, 정상 EOF인 inherited5 records, 두 NUL argv와
**50개 실제 local root environment**를 검사한다. 입력과 프로그램은 receipt 원자 발행
직전에 다시 검사한다. 기존 trace parser에서 unknown/미완성/잘못된 exec를 거부하는 정책은
바꾸지 않았다. provider admission이 실제 raw trace의 strict 감사를 대체하지 않는다.

## 예산과 실패 처리

첫 trusted step에서 고정한 `COLD560_JOB_DEADLINE_EPOCH`가 원래420초 종료값이다.
helper 호출 시 새420초를 부여하면 안 된다. shared launcher의 기존 timeout/15초 KILL/
20초 tail body는 바꾸지 않고, 잔여 시간에서 그20초를 내부 예약한다. provider의 바깥
owned command cap120초와 preparation 자체의120초 종료값을 함께 적용한다.
diagnostic의45분 job/shared1200초/configure180초/make300초/`-j2`는 그대로다.

디스크는 다음 **합계6GiB**를 operation-specific envelope로 admission한다.
같은 filesystem이면 잔여 예상 증가량을 합산하고, 다르면 filesystem별로 검사한다.
allocated/apparent 사용량을 모두 제한하며 고정25/35GiB floor는 없다.

| 범위 | cap/포함 관계 |
| --- | --- |
| source owner 전체 | 2GiB. `.git`, checkout, `$DIAG_SOURCE/rust/target` 포함 |
| owned job 전체 | 4GiB. 기존 venv/cache, registry, cold guard, proof, provider, private work 포함 |
| provider root + provider preparation proof | 위4GiB 안의512MiB. prepare/fixture/봉인 모두 포함 |
| provider 이외 최대 forecast | 5632MiB. 별도6GiB를 더 할당하지 않음 |

운영 전 `/proc/meminfo`의 실제 MemAvailable와 읽을 수 있는 현재 cgroup v2 및 그 조상들의
`memory.max-memory.current` 중 최소값을 확인한다. 시작 compile forecast는2GiB이며,
진행 중에는 실제 잔여64MiB 미만을 거부한다. cgroup을 읽어야 하는데 읽지 못하면 거부한다.
worker1/incremental0/dev-test debug0이며 release 설정을 사용하지 않는다. 현재 공유
`rust/Cargo.toml`에는 release thin LTO/codegen-units1과 num-bigint-dig dev opt3 예외가
있으며 이 작업은 변경하지 않았다.

이 cap은 자원 가정과 phase/poll admission이다. provider120초/512MiB, source2GiB 안의
native target, 전체7분/6GiB의 실제 peak·fit를 측정했다고 주장하지 않는다. 특히 preparation의
full SDK/도구/헤더 봉인 비용과 local RAM4GB 하에서의 compile 여유는 GHA에서 확인해야 한다.
fit하지 않으면 별도 dependency job에서 exact pinned source/flags로 준비한 provider+approver를
입력·원래 SDK·실제 ELF/DSO·source/build manifest와 함께 봉인해 인계하고, Fast에서는 그
artifact를 strict admission 후 사용하도록 별도 구성할 수 있다. 현재 helper는 이 대안을
자동 fallback으로 실행하지 않는다. cap/기한 증가나 stock provider 재시도는 없다.

실패는 `first-failure`, `cold.status`/`after.status` 및 각 단계 stdout/stderr에 남긴다.
loader/preflight 실패도 private proof 입장 뒤부터 기록한다. source/job 소유성 자체가
불명확하면 그 위치에 쓰거나 삭제하지 않으며, 부모의 기존 입장 증거를 보존한다.
현재 identity를 검증한 소유 leader/group만 KILL/direct wait한다. identity 불명은 signal/wait
대신 실패로 남긴다. 정상 종료 전후에도 group nonlive를 요구한다. finalizer 실패가 기존
실패를 성공으로 덮지 않고, cold 실패는 `after`에서 재승인되지 않는다.
helper는 source/job/provider를 삭제하지 않는다. 부모 cleanup은 upload와 source/SDK final
외에 **`fast_cold_final` 성공까지** 요구해야 한다. 실패/소유성 불명은 회수 승인으로 간주하지 않는다.

## 부모가 반영할 정확 변경

현재 `rust/tools/check-disk-space`는 bootstrap에 `CARGO_BUILD_JOBS=2`를 하드코딩한다.
외부 env1은 이를 덮지 못한다. 이 파일은 허용 쓰기 범위 밖이므로 변경하지 않았다.
부모가 다음 **한 줄**을 적용하고 관련 policy 검사도 함께 갱신해야 한다. 새 helper는
아래 실제 행이 없으면 preparation/Cargo 전에 실패한다. source를 private copy로 바꾸거나
다른 SHA로 위장하지 않는다.

```diff
-    CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 "${bootstrap_cargo[@]}" build \
+    CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 "${bootstrap_cargo[@]}" build \
```

WF는 수정하지 않았다. 기존 Fast native 첫 trusted step의 source/runner 검증 뒤에 다음을
추가하여 원래7분 clock을 시작한다. 기존 timeout-minutes7, job-temp/private checkout,
source precheckout2GiB admission, SDK before/after, 모든 후속 trusted 조건은 유지한다.

```bash
test -z "${COLD560_JOB_DEADLINE_EPOCH:-}"
printf 'COLD560_JOB_DEADLINE_EPOCH=%s\n' "$((EPOCHSECONDS+420))" >> "$GITHUB_ENV"
```

현재 긴 inline `Prepare native CI validation` run을 다음 step으로 연결한다. 기존 stock
tracer와 stderr collector를 이 helper와 함께 이중 실행하면 안 된다.

```yaml
- name: Prepare native CI validation
  id: fast_cold
  if: ${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}
  working-directory: ${{ env.DIAG_SOURCE }}
  shell: bash
  env:
    SOURCE_ALLOWED: ${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}
    FAST560_HELPER_SHA256: 645a23e3eed36c6663093cabd71f3618ba3f98af9ccf9848098659b25da1feec
  run: '"$DIAG_SOURCE/tools/ci/fast_native_cold_560.sh" cold'
```

이어지는 기존 routing step에서 기존6GiB warm admission을 유지한 뒤 실제 current auditor를
빌드·실행한다. cold보다 앞서 Cargo/테스트를 실행하지 않는다.

```bash
OPENPILOT_CI_PREPARED_RUST_ROOT="$CI_PREPARED_RUST_ROOT" \
  OPENPILOT_CI_PREPARED_RUST_MANIFEST="$CI_PREPARED_RUST_MANIFEST" \
  "$DIAG_SOURCE/rust/tools/check-disk-space" --path "$DIAG_SOURCE" --growth-mib 6144
CI_PREPARED_RUST_PROOF="$CI_JOB_TEMP/proof" \
  "$CI_PREPARED_RUST_ROOT/bin/cargo" test --manifest-path rust/Cargo.toml \
  -p openpilot-dev-tools --locked --test ci_disk_bootstrap \
  prepared_audit::prepared_audit_actual_cold_bootstrap_receipt -- \
  --ignored --exact --nocapture 2>&1 | tee "$CI_JOB_TEMP/proof/prepared-cold-native-audit.log"
grep -F 'test result: ok. 1 passed;' "$CI_JOB_TEMP/proof/prepared-cold-native-audit.log"
```

기존 나머지 routing 검사 뒤, SDK/source final보다 앞에 다음 final step을 둔다.
이 결과가 cleanup에 필수다. original failure를 무시하는 continue-on-error는 사용하지 않는다.

```yaml
- name: Seal Fast private cold provider after maintained checks
  id: fast_cold_final
  if: ${{ always() && (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && steps.fast_cold.outcome != 'skipped' && env.DIAG_SOURCE != '' }}
  working-directory: ${{ env.DIAG_SOURCE }}
  shell: bash
  env:
    SOURCE_ALLOWED: ${{ github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository }}
    FAST560_HELPER_SHA256: 645a23e3eed36c6663093cabd71f3618ba3f98af9ccf9848098659b25da1feec
  run: '"$DIAG_SOURCE/tools/ci/fast_native_cold_560.sh" after'
```

추가한 일반3개 회귀와 기존 parser 검사의 focused GHA 명령은 다음과 같다. ignored actual은
위 실제 cold 증거가 준비된 동일 job에서만 실행한다.

```bash
"$CI_PREPARED_RUST_ROOT/bin/cargo" test --manifest-path rust/Cargo.toml --locked \
  --no-default-features -p openpilot-dev-tools --test ci_disk_bootstrap prepared_audit::
"$CI_PREPARED_RUST_ROOT/bin/cargo" clippy --manifest-path rust/Cargo.toml --locked \
  --no-default-features -p openpilot-dev-tools --test ci_disk_bootstrap -- -D warnings
```

## 소스 불변 증명과 납품 pin

공유 library와 diagnostic은 이번 연결에서 변경0이다. 기존 Git blob
`a8c80cad02466ecaf9110d4e3e4dfaec9c136df6`의 네 함수 구간을 현재 diagnostic에 역투영하면
원본 **전체 bytes**가 같으며 SHA256은
`b3e7266fdc4d7275c7e9ef58eeb4fda66070bd09ab8eb6e5985974ad3a38f4d5`다.
따라서 원래18control/순서, marker10, protocol3, caps, strict audit body와 타이머를 보존한다.
Fast의 observer-health AWK도 diagnostic에서 byte 동일하게 가져왔다.
native 기존 parser/SDK admission prefix와 모든 기존 일반 테스트 원문도 보존했다.

| 파일 | SHA256 |
| --- | --- |
| `tools/ci/fast_native_cold_560.sh` | `645a23e3eed36c6663093cabd71f3618ba3f98af9ccf9848098659b25da1feec` |
| `tools/ci/prepare-cold-strace-560` | `6c7d99e6f5c1ee7fc4929c105a8bbb80334a47ec75e7e990ca5ea47c3d81bd58` |
| `tools/ci/cold_tracer_launch_560.sh` | `1d5d0ab9d736e5cf24844a38ac6b35e9a02a0a279b7abb2d25e6fff82aff926a` |
| `tools/ci/cold_tracer_diagnostic_560.sh` | `8a7383587589d01404f7e6573db8fd1ad458c22fbc6ff2755298b5a4a35ddc4b` |
| `rust/crates/dev-tools/tests/ci_disk_bootstrap/prepared_audit.rs` | `67c00cf93a2e73c40d6c0e43b938bcfd4f3379a2b2e3bde50478516ab83f47c7` |

로컬에서 `bash -n`, 단일 Rust 파일 `rustfmt --check`, source 역투영·원문 비교,
include 경로/50개 env key 대조 및 diff만 확인했다. 기능/ptrace/provider/helper source 실행,
Cargo/build/test/Clippy/GHA 실행은0이다. 이 자료는 Rust type-check나 runtime 성공 증거가 아니다.
부모가 WF/Focus/pins/원장/commit/push/GHA를 통합하고, exact-SHA의 새 회귀·실제 Fast cold
strict receipt·전체 자원 fit와 기존18/marker10/protocol3 수용을 확인해야 한다.

## 후속 좁은 정적 등록 — 2026-10-26

기준 HEAD: `83620dd1d74c03c84370079aada18bbcb9d2b48f`.
`ci_rust_isolation.rs`에
`fast_native_cold_bootstrap_uses_one_worker_and_pinned_closed_launch_helper`만 추가했다.
실제 `rust/tools/check-disk-space`의 jobs1 build 행이 정확히1회 있고 기존 jobs2 build 행은
없어야 한다. 새 helper가 그 실제 source 행을 확인한 뒤 provider120초 준비와 공유 STOP
승인 cold 호출을 순서대로 수행하는지, 같은 helper의 실제 absolute SDK/jobs1/debug0 환경,
raw exec trace·원래2048MiB 인자, private tracer/approver 선택도 검사한다.
library와 prepare source 전체 SHA가 helper의 정확한 literal pin에 연결되어야 한다.
parent의 check-disk-space jobs1 한 줄과 같은 batch로 반영해야 하며, 아직 jobs2인 source를
새 test가 성공으로 인정하지 않는다.

**지목된 기존 jobs2 literal은 Fast 검사가 아니었다.** 기존
`dbc_native_tools_are_built_before_every_required_consumer`의 literal은
`rust/tools/build-dbc-generator`를 검사한다. 실제 DBC helper도 jobs2이고 Darwin 등에서
사용한다. 이 기대행을 단순1로 바꾸면 변경하지 않은 DBC helper에 허위 실패가 생기므로,
기존 DBC jobs2 검증은 byte 그대로 보존하고 실제 Fast disk bootstrap에 jobs1 검증을
별도로 등록했다. DBC 소스나 정책을 약화·삭제하지 않았다.

허용 범위의 `ci_rust_isolation.rs` 및 `ci_disk_bootstrap.rs`/그 leaf에서 구 Fast inline/
stock strace에 의존하는 기대는 발견하지 못했다. `prepared_audit.rs`는 이미 실제 새
provider/source를 검사하므로 이번 좁은 등록에서 수정하지 않았다.
**실제 구 inline 의존은 허용 범위 밖 `ci_workflow_policy.rs`에 있다.** 이 파일도 수정하지
않았다. 부모의 WF 연결과 함께 다음 두 함수를 계속 옮겨야 한다.

- `native_fast_split_preserves_every_suite_and_build_check_once`: 기준 source507 부근의
  native용 `exec /usr/bin/strace ...` 전체행 기대를 fixed helper의 정확한1회 호출과 source/SHA
  binding으로 옮긴다. native 이외 worker의 기존 guard, 전체73 suite의 정확1회 분할,
  원래6144MiB admission 및 cold→admission→audit→기존 suite 순서 검사는 보존한다.
  actual ignored audit의 literal command도 부모가 변경하면 같은 실제 step source로 연결한다.
- `cold_bootstrap_stderr_collection_is_owned_bounded_and_preserves_failure_evidence`:
  기준 source593~790은 WF inline에서 `cold_collector_current`/`cold_provider before`/
  process-substitution collector/finalizer를 추출한다. 새 WF의 literal run에는 이 함수들이
  없으므로, source 검사/기능 harness의 대상을 새 고정 helper와 공통 launch 함수의 실제
  source로 옮겨야 한다. 기존 정상/command 실패/출력 실패/남은 프로세스/잘못된 startticks·
  parent/provider drift의 거부·상태 보존·after 실행 경계는 삭제하거나 skip하지 않는다.
  source가 없을 때 임의 fallback으로 옛 inline을 재구성하면 안 된다.

이 두 함수는 부모의 `ci_policy`/WF 통합과 함께 처리할 **남은 항목**이다. 새 helper의
기능 수용이 끝났다는 뜻이 아니며 GHA는 여전히0회다. 새 일반 정적 등록의 focused GHA 명령:

```bash
"$CI_PREPARED_RUST_ROOT/bin/cargo" test --manifest-path rust/Cargo.toml --locked \
  -p openpilot-dev-tools --test ci_rust_isolation \
  fast_native_cold_bootstrap_uses_one_worker_and_pinned_closed_launch_helper -- --exact
```

로컬은 새 함수만 stdin으로 `rustfmt --edition 2021 --emit stdout` 처리하고 source/diff를
확인했다. 파일 전체를 rustfmt하지 않았다. 새 함수 구간을 제거하면 기준 commit의 파일
**전체 bytes**가 일치하므로, 부모가 방금 반영한 SDK escape18을 포함한 기존 함수/검사/
baseline 형식은 변경0이다. Cargo/기능/GHA 실행, WF/Focus/ci_policy/SDK helper 수정은0이다.


### 6d0bd2 분리 변경의 후속 소스 보정

source `6d0bd2aaef1bcc0117f1419b9d93ddb7fb603b21`의 hosted routing
[121261950443](https://github.com/bin9208/openpilot-rust/actions/runs/40224180778/job/121261950443)은
changed-files 실제5개 중4개 성공 뒤 caller policy1개에서 실패했다. 회귀가 아직 기존
`native` step에서 자기 호출을 찾고 있었다. 실제 옮긴 `native-routing`으로 양성/변조 대상
두 곳을 함께 바꾸며 기존8종 음성과 실제 initial-push/absence/whitespace 검사는 보존한다.
이 실패 뒤 나머지 routing suite가 실행됐다고 주장하지 않는다.

Focus도 admission 단계에서 실패 상태다. 새 routing YAML의 parsed run과 모든 명령은
같았으나 emitter가 기존10-space Cargo raw 행을8-space로 출력했다. 원래 바이트 보존 조건을
그대로 두고 새 job의 sequence 들여쓰기를 원래 형식으로 복구했다. 소스 diff에서 baseline의
모든 보호 Cargo 행/count(기존 prefix 추가1 포함)가 그대로임을 확인했다. 이 정적 대조는
새 GHA 기능/전체 fit 결과를 대신하지 않는다. helper/감사 모듈/기한/자원은 변경0이다.


## 3b7492 실제 전체 Fast 수용 — 2026-10-26

source `3b7492faa24f7ad37945246d470a0c9447f7f3b5`의
[Fast40224700444](https://github.com/bin9208/openpilot-rust/actions/runs/40224700444)는
기존6개와 cold/routing을 합친8개 worker·aggregate가 전부 성공했다. native cold job
[121266542772](https://github.com/bin9208/openpilot-rust/actions/runs/40224700444/job/121266542772)은
전체308초로 원래7분 안 끝났다. cold140초, actual audit step63초, provider after18초와
SDK/source final·upload·두 cleanup이 성공했다. 감사 compile은12.44초, actual test47.89초였다.
앞선 source73의 compile163초와 비교되는 관측이며 일반적인 속도/peak 보장으로 확대하지 않는다.

| 실물 artifact | bytes | SHA256 | safe unique/non-symlink/CRC member |
| --- | ---: | --- | ---: |
| local cold12611247608 | 13268463 | `1b2fb853106ed2fde0840d636d2a9a6631d6cc1ce3554d4f2d099f514c36123c` | 1406 |
| hosted routing12610679097 | 374718 | `1313cd0814af47563645f4d8faa70115f735bd788ef301fc7fabe17163d07b50` | 47 |
| native tools12610752849 | 1585074 | `77d1b9fef43b8e2478e2e9d3d6f45a34717eff323d347a96d502ba9eb72ea761` | 44 |
| hosted diagnostic12610847299 | 11493618 | `00e1dca21ababf012f2335357379ef3f8d4e4fd49f36418c0918408d3c3e5eba` | 969 |

모두 해당 source3b7492를 보존한다. local receipt는 absolute cargo1/rustc37,
exec181/실패29, Python/Rustup0, SDK regular294다. cold/after의 original/final exit0과
추적한 owned groups nonlive=true, provider inputs/source/build/bin의 전후
SHA/roster/stable12쌍 및 `verify-complete`를 확인했다. source/index/SDK files·roster·program도
동일하다. 프로세스 전체의 추가 reap/closed DSO 또는 poll 사이 peak를 측정한 뜻은 아니다.

routing job121266542803은 changed-files5와 lib83, disk44/ignored1, process3,
isolation89, workflow42 및 Navi exact2가 성공했다. 전체73 suite/추가 회귀·13종 fail-closed와
private source roster77/ARM24 검증도 통과했다. routing source/index/SDK5쌍과 program roster도
동일했다. native tools run40224818843은 기존 prepared11과 새 thin11/ignored1·strictClippy를
포함한 모든6개 exit-code0 및 source/index/SDK6쌍 동일을 확인했다.

별도 hosted diagnostic40224700325는18control(7+4+7), marker10/protocol3와 실제 cold의
cargo1/rustc37/exec180/실패29/Python0/Rustup0/SDK165를 확인했다. 기존 root-fixed=false,
parent-link=false·옛seccomp/nommu 미측정과 retirement credit0은 유지한다.

따라서 #530의 prepared SDK/cold/routing 결함 범위는 수용 가능하다. #560은 새로운 Fast
관측 경계 수용과 원래 Ubuntu race 원인·정확 PR 통합 조건을 구분해 계속 추적한다.
PR343의 전체 통합·모델·recovery·기기 수용과 전체 #523 완료를 뜻하지 않는다.


## #597 간헐적 authored control 실패와 case 문맥 보완 — 2026-10-26

[#597](https://github.com/bin9208/openpilot-rust/issues/597)은 실제 provider 또는 모델 수치
비교와 구분하는 authored lifecycle control의 미해결 결함이다. 원문 로그 대조 결과는 다음과 같다.

- source `6c5d5a9be691d18ca850af6ef9b42e47b668d767`의
  [model-family40255341914/job121355876682](https://github.com/bin9208/openpilot-rust/actions/runs/40255341914/job/121355876682)
  로그1424행에서 `cold_bootstrap_stderr_collection_is_owned_bounded_and_preserves_failure_evidence`는
  PASS였다. 해당 job 전체는 당시 별도 workflow 크기 검사에서 실패했다.
- source `57e1761c5cac9ddee282550d8c079083e6fa9a5a`의
  [model-family40256938653/job121360705072](https://github.com/bin9208/openpilot-rust/actions/runs/40256938653/job/121360705072)
  로그1405~1406행에서 `ci_workflow_policy.rs:1045`의 `run_shell(...).unwrap()`이
  `native command stderr exceeds 16777216 captured bytes`로 실패했다. 정책 결과는
  41 PASS/1 FAIL이며, step22 업로드 실패는 앞선 중단으로 기대 파일이 없는 후속 실패다.
- 같은 source57의
  [Fast40256938652/job121360705476](https://github.com/bin9208/openpilot-rust/actions/runs/40256938652/job/121360705476)
  로그1196행에서는 같은 테스트가 PASS했고, workflow policy42/42와 ciisolation89/89도
  PASS했다. Fast 전체 worker와 aggregate도 API에서 SUCCESS를 확인했다.

6c/57 사이 해당 harness 본문과 cold launcher/helper 및 shared QA 소스는 byte 동일하다.
세 실행의 runner `2.338.0`, Ubuntu image `20261012.334.1`도 같았다. 따라서 특정 이미지 변경이나
Bash/kernel/부하를 원인으로 단정하지 않는다. 실패한13개 case 중 어느 case인지와 실제 stderr
생성 경로는 아직 미확정이다. `CaptureError::Limit`에는 원시 stderr가 보존되지 않는다.

이번 수정은 테스트의 해당 호출 한 곳에서만 오류에
`cold-control case={case}: {error}` 문맥을 붙인다. `run_shell`의 `env -i` 및 NativeRunner,
16MiB cap·5초 capture 기한·13개 case·harness 본문·명령·소유/회수·실패 sticky 조건은 그대로다.
production/shared QA/launcher는 수정하지 않았고, 추가 snapshot·로그 reader도 넣지 않았다.
이 보완은 다음 실패의 case 식별만 제공하며 stderr 원문 복원이나 #597 원인 해결을 뜻하지 않는다.
model-family caller의 push paths에 해당 `ci_workflow_policy.rs` 경로 한 개를 추가해 이 준비 gate
변경도 기존 Actions 실행으로 검증한다. job·명령·runner·기한은 바꾸지 않았다.

로컬은 변경 구간의 Rust2021 구문·rustfmt와 diff만 확인했다. 기능/Cargo/CI 실행 및 stage/commit은0이다.
부모가 후속 독립 diagnostic commit으로 Fast와 영향 model-family 준비 gate의 실제 case를 확인할 예정이며,
**새 case 문맥의 실제 GHA 결과는 UNRUN, #597은 미해결**이다.
