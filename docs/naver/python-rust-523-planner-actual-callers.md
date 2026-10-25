# #523 Planner actual caller와 memory native 연결

## 최신 수용과 삭제 후 후보 — 2026-10-26

source425의 [actual host40227447987](https://github.com/bin9208/openpilot-rust/actions/runs/40227447987)은
native runtime12와 native memory ASan3 모두 실제0/PASS다. functional12612670103
55,186,524B/SHA256 `96b275577f6d9eec3fe106d65b258e00cbfcd3be8421d0659122e73a6f007252`의
34,282 safe/CRC, private12612575557 296,340B/SHA256
`73a1c564eec4b5f05b7894b505fcd90a84e5d0bc640c17b6eb7ec395ba62e845`의29 CRC와
source/index 전후2쌍·source425/exit0를 감사했다. generated C 계측 범위는 기존대로다.
ARM runtime12의 삭제 전 source6d 성공도 별도 기존 기록대로 유지한다.

현재 후보는 두 controller와 memory wrapper의 Python 실행 소스·테스트4경로를 제거한다.
이 중 baseline3580에는3경로, 추가 실행 원장에는 나머지 memory wrapper unittest가 있다.
따라서 삭제4개를 baseline credit4개로 세지 않는다. 현재 baseline296/3/3281과 추가 원장161개는
삭제 후 native14/frozen5/runtime guard6/memory15·strict Clippy·actual host/ARM 검증 전까지 유지한다.

main/reusable ARM/standalone ARM/host exact-copy의 pytest 실행은 같은 기존1024MiB admission
뒤 native_build_closure와 함께 policy21/controller14/frozen5/runtime guard6을 실행하도록 바꿨다.
나머지 native Planner test/build/Clippy·네 성장량·ASan/Miri 조건·기한·IPC160·실제12/3 leaf는 보존한다.
main499,336B이며 세 진단 copy는 기존 reviewed-source SHA step 하나를 제외하면 현재 main의
job/env와 전체 object가 동일하다. 새 source의 GHA는 아직 미실행이며 삭제 credit0이다.

아래는 최초 연결 당시의 역사 인계다. wrapper를 호출한다는 설명은 지금의 native
`memory --asan-library` 구현과 [후속 기록](python-rust-523-planner-native-memory.md)이 대체한다.


2026-10-26. 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
시작 HEAD는 `33923f8fc0b954b4797480e64b984200ac45a138`, 인계 준비 중 부모 변경을
읽은 HEAD는 `f1c730b029df77ed2731f86ddf2d224e74bb9f50`다.
본 변경은 **stage/commit/push하지 않았다**. main/reusable workflow, 공유 ci_policy,
Focus, 원장 및 다른 agent 파일은 수정하지 않았다. 아래 본문 치환은 부모 적용안이다.

부모 인계에 따르면 `33923f8`의 native11·adapter13(real Rust160)·strict Clippy와
canonical `PLANNER_REFERENCE_PYTHON`의 original4 검증
[40216487452](https://github.com/bin9208/openpilot-rust/actions/runs/40216487452)는 SUCCESS다.
본 작업은 그 cohort를 재실행하지 않았고, artifact 전후 독립 감사는 부모가 추후 수행한다.
이 성공을 아래 새 memory 코드나 actual caller의 기능 검증으로 대신하지 않는다.

## 찾은 실제 경로와 유지한 경계

요청의 `rust/tools/check_plannerd_memory.py`라는 파일은 없었다.
실제 ASan wrapper는 `rust/tools/check_plannerd_native_memory.py`다.

main `rust.yml`과 reusable `rust-native-arm.yml`의 `planner-runtime`을 읽었다.
host는 현재 `local-x64` 단일 row, job timeout45분이고 native ARM은 timeout90분이다.
둘의 비교 run 본문은 같은 Python controller 1행이다. sanitizer step 본문도 같으며,
native ARM의 기존 `matrix.runner == 'local-x64'` 조건은 false라서 ASan을 생략한다.
별도 `planner-memory`의 45분/네 Miri mode와 원래 명령·조건은 건드리지 않았다.

앞선 dependency/stager/solver/source/Params/msgq 준비, wheel·generator pin,
Planner test/build/Clippy argv와 성장 guard는 그대로 둔다. host의 기존 일반 SDK와
stager provenance, native ARM의 `arm530_sdk` before/verify/after 고정 검증을 각각 유지한다.
main host Planner에는 whole-SDK before/after 단계가 없으며 새로 요구하지 않는다.
trusted source 조건, openpilot-local-shell, source seal/upload/cleanup도 유지한다.
이 helper는 현재 두 maintained caller에서만 사용할 후보이며 옛 QEMU 경로로 확장하지 않는다.

## 새 실행 연결

`tools/ci/planner_native_controller_523.sh runtime`은 기존 private source/job/owner,
SHA, host, Python venv와 기존 SDK 경로를 확인한다. native-json11과 같은
`--locked --features ci-receipt-metadata-native --target "$NATIVE_TOOLS_HOST"`,
동일 target dir/profile/compiler 환경 정리를 사용해 `check-plannerd-ci` bin만 빌드한다.
기존 stager의 feature/cache를 재사용하며 새 build의 `CARGO_BUILD_JOBS=1`을 적용한다.
SDK cargo/rustc/rustdoc는 절대 경로로 실행하고 rustup 설치나 target 추가는 하지 않는다.
ARM prepared root/manifest/자동 설치 금지는 기존 계약을 요구한다.
ARM의 whole-SDK 전후 고정 검증과 transport 검증은 기존 caller shell/workflow가 계속 소유한다.
host는 기존 일반 SDK/stager 경로에서 절대 SDK 실행물과 `RUSTUP_AUTO_INSTALL=0`을 사용한다.
host에 whole-SDK 설치·봉인·검증 범위를 추가하거나 요구하는 변경은 아니다.

그 뒤 native 12-leaf controller를 호출한다. 기존 bin-dir/artifact/source/binding/output,
각 leaf의 전체 argv·순서와 IPC160 증거 요구를 유지한다. 실제 leaf interpreter는
기존 `$CI_JOB_TEMP/venv/bin/python`을 그대로 사용하며 그 경로를 canonical symlink
target으로 바꾸지 않는다. stdlib 전용 original4용 `PLANNER_REFERENCE_PYTHON`은 변경하지 않는다.

helper의 `memory`는 runtime에서 만든 source/host binding 및 controller SHA를 확인하고
같은 바이너리를 재사용한다. 재빌드하지 않는다. 현재 x64 ASan 역할만 허용한다.
실제 Python sanitizer wrapper는 원래 wrapper 생성과 최종 sanitizer receipt를 소유하면서
native `memory` CLI로 `abi/owner/fault` 세 leaf를 넘긴다.
old `check_plannerd_ci` import/`Inputs`/`runtime_checks`/`run_checks` 사용을 제거했다.

native memory 모드는 세 operation growth만 정확히 요구하고 full-runtime PASS receipt를
만들지 않는다. wrapper는 native exit0뿐 아니라 `commands.json`의 정확한 3개 name/순서/
returncode0을 확인한 뒤 기존 최종 receipt를 쓴다. exit42는 native CLI→wrapper→helper에서
42로 전달되며 뒤 leaf/PASS를 만들지 않는다. owned group 회수는 기존 `qa_process` 경로다.

원래 `LD_PRELOAD`, ASAN leak/abort/strict-string flags, UBSAN halt/stacktrace,
instrumented provenance 요구, solver/owner/runtime wrapper, 실행물/manifest/ASan SHA와
scope 문자열은 유지했다. source AST 검토에서 staging loop·provenance assert와
checks 필드를 제외한 모든 기존 receipt 값이 HEAD와 같음을 확인했다.
checks는 원래 필터 결과와 같은 `abi/owner/fault`다.

## 작업별 자원·기한

고정 25/35GiB 규칙을 복원하지 않는다. 다음은 측정 peak가 아닌 보수적 예상 증가량이며,
helper가 명령명별로 native CLI에 반드시 공급한다. 실제 첫 통합 Actions에서 사용량을 확인한다.

| operation | 기본 예상 증가량 MiB | 근거와 확인 위치 |
| --- | --- | --- |
| controller build | 2048 | 기존 native-json11 host tool build와 같은 package/feature/cache; source filesystem에서 사전 admission |
| abi/policy/radar/gap | 128 각각 | 파일/JSON trace 비교; 각 output filesystem에서 native inspect |
| owner/main/fault/volkswagen/stock-longitudinal/radar-off | 1024 각각 | 원래 360-frame 및 fault80-frame 입력/expected/actual/packet 기록; native inspect |
| ipc/lifecycle | 256 각각 | 원래 IPC40×4·bounded lifecycle 출력; native inspect |
| sanitizer wrapper 준비 | 128 | 기존 artifact를 복사하지 않고 3개 작은 exec wrapper와 metadata만 생성 |

`PLANNER_CONTROLLER_BUILD_GROWTH_MIB`, 각 `PLANNER_<OPERATION>_GROWTH_MIB`로 조정할 수 있다.
hyphen은 underscore로 바꾼 대문자 이름이다. 생산 helper는 빈값/0/음수/비숫자를 거부한다.
예산 누락/중복/다른 이름은 native CLI가 거부한다. 테스트의 growth0은 합성 fixture용이다.
명령 기록에 기존 `free_before`와 함께 새 `growth_bytes`를 남긴다.

per-command timeout은 host2700초/ARM5400초로 기존 job45/90분 상한과 맞춘다.
job timeout 자체와 각 leaf 내부의 publication/startup/회수 기한은 그대로다.
helper가 별도 짧은 timeout으로 기존 비교를 자르지 않으며 전체 job 상한도 늘리지 않는다.
helper 내부 새 Cargo는 jobs1이며 기존 앞선 Planner build args/기존 job env는 바꾸지 않았다.

build/source/host/SHA와 mode별 operation 계획·disk 전후·MemAvailable·exit code는
`$CI_JOB_TEMP/proof/planner-native-controller-523/`에 기록된다.
기존 private proof upload가 이 디렉터리를 포함하므로 artifact path 추가는 필요 없다.
memory 결과와 runtime 결과의 기존 upload 위치도 유지한다.
실패 후 관측 오류가 원래 양수 실패 코드를 덮지 않으며, 성공 후 관측 실패도 성공으로 숨기지 않는다.

## 부모 적용: 정확한 run 본문 치환

두 workflow의 step name/if/shell/working-directory/timeout/env/needs는 유지한다.

`jobs.planner-runtime`의 `Compare exact policies, main loops, real IPC and lifecycle`
현재 run 본문:

```sh
python -P rust/tools/check_plannerd_ci.py --bin-dir rust/target/debug --artifact "$RUNNER_TEMP/planner-artifact" --source-native "$RUNNER_TEMP/planner-source" --binding "$PLANNER_PARAMS_BINDING" --output "$RUNNER_TEMP/planner-comparison"
```

새 run 본문:

```sh
/usr/bin/bash "$CI296_SOURCE/tools/ci/planner_native_controller_523.sh" runtime
```

같은 job의 `Check generated C with ASan, UBSan and allocator leak detection`
현재 run 본문:

```sh
python rust/tools/build_plannerd_acados.py --architecture x86_64 --cc gcc --sanitize --python "$(command -v python)" --acados "$PLANNER_GENERATOR" --wheel "$RUNNER_TEMP"/planner-wheel/*.whl --output "$RUNNER_TEMP/planner-sanitized"
python -P rust/tools/check_plannerd_native_memory.py --bin-dir rust/target/debug --artifact "$RUNNER_TEMP/planner-sanitized" --source-native "$RUNNER_TEMP/planner-source" --binding "$PLANNER_PARAMS_BINDING" --asan-library "$(gcc -print-file-name=libasan.so)" --output "$RUNNER_TEMP/planner-native-memory"
```

새 run 본문은 **원래 sanitizer build 1행을 그대로** 유지하고 2행만 바꾼다.

```sh
python rust/tools/build_plannerd_acados.py --architecture x86_64 --cc gcc --sanitize --python "$(command -v python)" --acados "$PLANNER_GENERATOR" --wheel "$RUNNER_TEMP"/planner-wheel/*.whl --output "$RUNNER_TEMP/planner-sanitized"
/usr/bin/bash "$CI296_SOURCE/tools/ci/planner_native_controller_523.sh" memory
```

runtime 먼저, memory 나중이라는 기존 순서를 유지한다. 실제 ASan wrapper에는 새 필수
`--controller/--timeout-seconds/--growth-mib`가 생겼으므로 이 변경과 caller 치환을 같은
부모 통합에 넣어야 한다. **옛 WF의 memory 호출을 그대로 두면 필수 인자 누락으로 실패한다.**
native ARM에서는 기존 ASan if/skip을 그대로 보존한다. Miri 네 mode는 변경하지 않는다.

시작 시 main WF는499630B였고 부모 동시 commit 후 readback은499697B였다.
위 치환은 각 WF의 unescaped run scalar 기준 runtime238→80B(-158),
sanitizer537→311B(-226), 합384B를 줄인다. 최종 YAML의500000B 등록 한계는
부모가 기존 single-flow 직렬화 후 실제 UTF-8 bytes로 다시 확인한다.
본 작업은 workflow를 재직렬화하거나 저장하지 않았다.

부모가 함께 검토할 기존 기대값은 `tests/planner_ci_policy/contract.rs`의 비교 run literal과
`rust/tools/tests/test_plannerd_ci.py`의 같은 literal이다. 양쪽 actual/reference가 같은 helper를
쓰도록 갱신하고 기존 전체 기능 step 동등 비교·나머지 gate는 유지한다.
shared ci_policy/Focus/source pins/원장 갱신은 부모 소유다.

## 새 검사와 다음 필요한 Actions

native14/ignored source5로 늘었다. 기존11/source4 외에 다음을 추가했다.

- `memory_plan_requires_only_abi_owner_fault_and_preserves_sanitized_wrapper_paths`
- `registered_memory_controller_executes_three_checks_without_a_full_runtime_receipt`
- `registered_memory_controller_propagates_the_first_failed_leaf_and_skips_fault`
- `source_memory_subset_matches_every_native_sanitizer_leaf_argument` — 기존 원본 controller의
  실제 runtime plan에서 같은 세 leaf를 골라 모든 argv를 비교한다. 신규 original selector다.

새 Python wrapper unittest5개는 실제 wrapper main을 실행하여 sanitizer flags/해시/인자,
exit42 전파, 기록 없는 exit0, 불완전 subset, unsanitized 거부를 검사한다.
그 child는 통제 fixture이며 실제 ASan solver 합격의 증거가 아니다.

아래는 **새 변경용** 제안이며 이 세션에서 실행하지 않았다. 기존33923 원본4 성공 cohort는
취소/재시작/변경 없이 그대로 유지한다. 새 batch 통합 때 부모가 필요한 범위를 배정한다.

```sh
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_controller memory_
PLANNER_REFERENCE_PYTHON="$oracle_python" CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_controller source_memory_subset_matches_every_native_sanitizer_leaf_argument -- --ignored --exact --nocapture --test-threads=1
"$oracle_python" -I -B -S -m unittest discover -s rust/tools/tests -p test_plannerd_native_memory_controller.py -v
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin check-plannerd-ci --test planner_ci_controller -- -D warnings
```

`oracle_python`은33923에서 확정한 canonical stdlib interpreter 바인딩을 재사용한다.
전체 target을 새 batch에서 실행하면14 passed/5 ignored이고 `source_ -- --ignored`는5개다.
Focus의 종전11/4 count expectation을 바꾸는 권한과 새 actual host12/ARM12/x64 memory3
결과·artifact 감사는 부모에게 있다. 기존 adapter13 source가 바뀌지는 않았다.

## 원본 보존·검증 한계

`rust/tools/check_plannerd_ci.py`는 원본 plan/실패 전파 비교 oracle에 필요하므로 보존한다.
`plannerd_ipc.py`의 independent 원본 함수와 Python wire/reference/lifecycle 경로,
sanitizer wrapper 자체 및 나머지 Python leaf도 그대로 필요하다.
이번 삭제 파일과 retirement credit은 **0개**다.

로컬은 `bash -n`, Rustfmt, Python/fixture/원본 snippet의 AST 구문 검사와 diff 검토만
수행했다. memory staging/provenance/기존 receipt의 source AST 보존도 확인했다.
새 기능/ASan/native/Python reference/build/Cargo/Clippy 실행은 **0회**다.
WF/GHA/commit/push/stage도 **0회**이며 ready 상태로 부모 통합을 기다린다.

## 인계 source SHA256

아래7개 source는 최초 actual caller 인계 시점의 SHA 기록이다. 이후 부모 batch에서
변경한 source 전체의 현재 SHA를 뜻하지 않는다. ARM preflight 보정 helper의 최종 SHA는
다음 절에 별도로 고정하며, 이 문서의 최종 SHA는 인계 메시지에서 전달한다.

```text
a1416e3dc4ac16c31214b871576ec663564bcda8fcf0184f9285cd48175878ef  tools/ci/planner_native_controller_523.sh
4f36c97c584d11cccafa53a327893f0d0155f2da0e988ea774d2615396110782  rust/tools/check_plannerd_native_memory.py
c5b738aaba3f219307611d2bdd6efee92c341487a0463e5a9d484394af069b62  rust/crates/dev-tools/src/planner_ci_controller.rs
f336e849a7554ea154df1c55ee4f721a894eafd1585075ae97556d6cbe8bc87e  rust/crates/dev-tools/src/planner_ci_controller/cli.rs
c1dddd235346b8f415c9b131aba7a371cb67ae80d0db5f13a86b450c4d0a8b44  rust/crates/dev-tools/src/planner_ci_controller/plan.rs
4da95b95176d911bfc62de3b6250d913e92f22f2c3f9bf6219727d86c16d163e  rust/crates/dev-tools/tests/planner_ci_controller.rs
67006fee5e1a81e8350a4b514debd6f7e170af3541afa740c797fe0086016fb4  rust/tools/tests/test_plannerd_native_memory_controller.py
```

## source73 ARM preflight 보정과 최종 helper 고정

2026-10-26. run `40220990592`의 ARM job `121252313088` 로그를
`.analysis/scratch/2026-10-25-523-completion/ci/planner-arm73.log`에서 읽었다.
standalone `python-rust-523-native-arm-more.yml`은 prepared mode 없이
`RUSTUP_HOME=$job/rustup-home`과 hosted ARM의 `native-host` transport를 제공했다.
기존 helper가 aarch64 전체에 `arm530-sdk/rustup-home`을 요구해 controller 호출 전에
거부했다. main/reusable/standalone의 현재 Planner 환경과 실제 SDK 제공 단계를 비교해
helper의 host/mode 검증만 보정했다.

| caller | 허용하는 SDK 계약 |
| --- | --- |
| main host Planner | mode/root/manifest 없음, `$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu` |
| standalone ARM Planner | mode/root/manifest 및 reusable caller 표시 없음, `$job/rustup-home/toolchains/1.94.0-aarch64-unknown-linux-gnu` |
| reusable ARM Planner | `native-arm-530-v1`, `$job/arm530-sdk/rustup-home/toolchains/1.94.0-aarch64-unknown-linux-gnu`와 같은 prepared root, `$job/proof/arm530-sdk/sdk.before.sha256`, `RUSTUP_AUTO_INSTALL=0` |

ARM은 추가로 `CI296_NATIVE_ARM=1`, `github-hosted:Linux:ARM64`, `native-host`,
host/target architecture `aarch64:aarch64`를 요구한다. 미지원 mode, 섞인 metadata,
reusable caller의 prepared 누락은 오류를 출력하고 거부한다. 임의 SDK 경로나 prepared
오류 뒤 일반 SDK fallback을 허용하지 않는다. 공통 job/source 소유권·SHA·venv 검사와
SDK 실행물 검사부터 실행 끝까지의 본문은 이전 helper와 바이트가 같다. jobs1,
host2700초/ARM5400초, leaf 순서·growth admission·첫 실패 전파도 그대로다.

standalone의 현재 installer 경로를 유지하면 arm530 템플릿 이식은 필요하지 않다.
추후 이식한다면 reusable의 `arm530_sdk` before, dependency 단계 verify,
`arm530_sdk_final` after와 기존 always/outcome 조건을 함께 옮겨야 한다.
mode만 지정해서는 통과하지 않는다. standalone push paths에는 현재 helper 경로가
없으므로 helper 변경을 해당 workflow에 연결할 때 부모가 path filter도 함께 다룬다.
기존 ARM sanitizer의 `matrix.runner == 'local-x64'` false 조건은 유지한다.
main host에 whole-SDK before/after 범위를 추가하지 않는다.

최종 helper SHA256:

```text
40e07c4d3261197a4c1be5d5acf0959cc2450ec30ba7c6ff8c87633c0de32c8f  tools/ci/planner_native_controller_523.sh
```

부모가 안정된 위 값으로 `rust/tools/tests/test_plannerd_ci.py`와
`rust/crates/dev-tools/tests/planner_ci_policy/contract.rs`의 helper pin을 갱신한다.
부모의 nativeunitpush tools-only 라우팅에 따라 변경 없는 buffers/Planner 검사를
재실행하지 않는다. 이 작업에서는 공유 pin·workflow·master/corpus 문서를 수정하지 않았다.

부모 전달 host 실제 artifact 감사 결과는 다음과 같다. 이 세션의 독립 다운로드·감사
결과가 아니며, source73의 성공을 위 새 ARM preflight 보정의 실행 검증으로 대신하지 않는다.

| artifact | 부모 전달 SHA256 | 부모 전달 감사 결과 |
| --- | --- | --- |
| private `12609399553` | `5961fee67e933e0b84923cc1f5f9ab0079e06f8301245b67912983d6b5544f4e` | 29 members |
| functional `12609249847` | `e1d196ac2bb999867fd45fdff50702c6fa60f0917b7d1d251576c64a7e9df030` | 34,282 safe/CRC |

부모가 확인한 host job `121252313424`는 source73의 runtime12·memory3 모두 exit0,
result PASS다. 원본 보존과 retirement credit0은 계속 유지한다. 이번 보정의 로컬 확인은
`bash -n`, 소스 바이트 비교, diff에 한정했다. 기능/build/Cargo/Clippy·stage·GHA 실행은
0회이며 helper와 이 문서가 ready 상태다.
