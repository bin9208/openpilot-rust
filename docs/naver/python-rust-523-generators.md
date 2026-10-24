# #523 camera·paramsd generator 실패 조사

작성: 2026-10-24. 상위 작업: [#523](https://github.com/bin9208/openpilot-rust/issues/523),
기존 [#296](https://github.com/bin9208/openpilot-rust/issues/296)·[PR #343](https://github.com/bin9208/openpilot-rust/pull/343).

## 기준과 판정

- 작업 트리: `/home/fma1/.codex/worktrees/python-rust-523/openpilot-rust`
- 브랜치: `codex/feat-523-python-rust`
- 조사 기준 HEAD: `7a3727e445575a8142ed2e92880368256f79dc4c`
- 실패 실행: [39844308544](https://github.com/bin9208/openpilot-rust/actions/runs/39844308544).
  실행 메타데이터의 head는 `adff0392e61608c9a576550be78dd7eefb8b9065`이며,
  PR job의 실제 checkout은 merge SHA `6f25f161aae5eb3f887df6d0147172e4f85d935c`이다.
- 두 실패의 원인을 처리하는 기존 수정이 현재 HEAD의 조상으로 포함되어 있다.
  해당 두 파일은 각 수정 커밋 이후 현재 HEAD까지 추가 diff가 없다.
  generator 코드나 테스트를 중복 수정하지 않고 이 조사 기록만 추가한다.
- 이 판정은 로그·코드·이력의 대조 결과다. 현재 HEAD의 기능/빌드/clippy 통과나
  원본 동등성 재검증 완료를 뜻하지 않는다. 부모 작업의 `bin-actions` 검증이 남아 있다.

## camera: 단계 이름과 실제 실패 구분

[job 120055270729](https://github.com/bin9208/openpilot-rust/actions/runs/39844308544/job/120055270729)의
실패 단계는 `Build and validate native camera source generators`다.
실제로 실패한 항목은 다음 lifecycle 비교 테스트다.

```text
2026-10-22T20:12:27.5423658Z
crates/dev-tools/tests/camera_lifecycle_check.rs:325:9
original_process_environment_sanitizer_failure_and_artifact_order
assertion `left == right` failed: disk-boundary
```

로그의 `left: Object`와 `right: Object`를 각각 해석해 비교했다.
양쪽 모두 257개 산출물이 있고, 다른 파일은 `report.json` 하나뿐이다.
그 JSON의 유일한 필드 차이는 `free_bytes`이며 실제값은 `1073741824`(1 GiB),
기대값은 `27917287424`(26 GiB)다.

원인은 기존 growth-only 정책 전환 때 테스트의 주입값은 `26 GiB - 25 GiB = 1 GiB`로
바뀌었지만, 원본 캡처의 보고서 기대값에는 여전히 26 GiB가 남았기 때문이다.
실제 디스크 부족이나 camera source 추출 오류가 아니다.
이 실행은 lifecycle 테스트에서 종료되어 `camera_source_generate` 테스트 실행까지
도달하지 않았으므로, 해당 run을 source generator 통과 증거로 사용할 수 없다.

기존 수정 [3e79829c4d851b3bf1c435f7b168eacbc6b457f9](https://github.com/bin9208/openpilot-rust/commit/3e79829c4d851b3bf1c435f7b168eacbc6b457f9)는
`disk-boundary`의 기대 보고서를 복제한 뒤 `free_bytes` 한 필드만 실제 주입값으로
변환한다. 기존 필드가 정확히 한 번 존재하는지 확인하며, 나머지 산출물의 전체 바이트
비교를 유지한다. 원본 fixture를 수정하거나 비교 범위를 줄이지 않는다.
현재 controller의 1 GiB 작업별 growth admission도 이번 작업에서 변경하지 않았다.

## paramsd: CarParams 빌드 이후 함께 빌드된 estimator의 모듈 오류

[job 120055270784](https://github.com/bin9208/openpilot-rust/actions/runs/39844308544/job/120055270784)의
실패 단계는 `Build native paramsd atomic-gate CarParams generator`다.
로그상 `generate-paramsd-car-params`의 명시적 `cargo build`는
`2026-10-22T20:17:34.5126510Z`에 성공했다.
이후 `cargo test --test paramsd_car_params_cli`가 함께 빌드한
`check-paramsd-estimator-native`에서 다음 오류가 발생했다.

```text
error[E0583]: file not found for module `identity`
error[E0583]: file not found for module `numeric`
error[E0583]: file not found for module `reference`
  --> crates/dev-tools/src/bin/../paramsd_estimator_check.rs:22..24
error[E0282]: type annotations needed
  --> crates/dev-tools/src/bin/../paramsd_estimator_check.rs:120:9
error: could not compile `openpilot-dev-tools` (bin "check-paramsd-estimator-native")
```

binary는 `#[path = "../paramsd_estimator_check.rs"] mod controller;`로 controller를
불러온다. 당시 controller 내부의 세 `mod` 선언에는 명시적 경로가 없어 실제 파일이 있는
`src/paramsd_estimator_check/` 대신 `src/identity.rs`, `src/numeric.rs`,
`src/reference.rs`를 찾았다. `bound.verify()`의 E0282도 함께 보고되었지만,
이 로그는 CarParams 생성값이나 모델 수치가 잘못되었다는 증거가 아니다.

기존 수정 [606b1419618092c18c25a079e925935a70f1da20](https://github.com/bin9208/openpilot-rust/commit/606b1419618092c18c25a079e925935a70f1da20)는
각 모듈에 `#[path = "paramsd_estimator_check/<module>.rs"]`를 추가했다.
현재 세 경로가 모두 존재하며 binary와 integration test 양쪽의 controller 포함 경로에서
동일한 하위 파일을 가리킨다. 별도 타입 주석이나 generator 동작 변경은 추가하지 않는다.

## 수행한 확인과 남은 focused CI

실제 job 로그는 다음 API로 읽었다. `gh run --log-failed`의 빈 출력을 근거로 추정하지 않았다.

```bash
gh api repos/bin9208/openpilot-rust/actions/jobs/120055270729/logs
gh api repos/bin9208/openpilot-rust/actions/jobs/120055270784/logs
```

로컬에서는 수정 커밋의 조상 관계, 현재 파일과 수정 커밋 사이의 diff,
모듈 파일 존재, 로그 산출물 차이, Rust 1.94.0의 아래 syntax/fmt 확인을 수행했다.
`rustfmt --check`는 진단 없이 끝났다. 코드 실행·타입 검사·기능 테스트는 수행하지 않았다.

```bash
rustfmt +1.94.0 --check --edition 2021 --config skip_children=true \
  rust/crates/dev-tools/src/camera_source_generate.rs \
  rust/crates/dev-tools/tests/camera_source_generate.rs \
  rust/crates/dev-tools/tests/camera_lifecycle_check.rs \
  rust/crates/dev-tools/src/paramsd_estimator_check.rs \
  rust/crates/dev-tools/src/bin/check_paramsd_estimator_native.rs \
  rust/crates/dev-tools/src/bin/generate_paramsd_car_params.rs \
  rust/crates/dev-tools/tests/paramsd_car_params_cli.rs
git diff --check
```

부모 작업이 `bin-actions`에서 실행할 최소 기능·빌드 selector는 다음과 같다.
기존 작업별 growth admission(camera 1024 MiB, paramsd 512 MiB), 원본 비교 단계와
모든 필수 gate를 유지해야 한다. 아래 명령은 이 작업에서 실행하지 않았다.

```bash
# 실제 camera 실패 항목
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features camera-runtime-native --test camera_lifecycle_check original_process_environment_sanitizer_failure_and_artifact_order -- --exact --nocapture
# 아직 이 실패 run에서 실행되지 않은 source/lifecycle generator 비교
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features camera-runtime-native --test camera_source_generate --test camera_lifecycle_generate
# paramsd의 실제 실패 binary와 원래 단계의 generator를 명시적으로 포함
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features paramsd-fixture-native --bin generate-paramsd-car-params --bin check-paramsd-estimator-native
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features paramsd-fixture-native --test paramsd_car_params_cli --test paramsd_estimator_check
```

`paramsd_car_params_cli`의 ignored 원본 wire 비교와 exec 감사 테스트는 기존 hosted
proof 준비 단계 이후에만 실행한다. 빈 proof나 `--ignored` 없는 통과로 대체하지 않는다.
원래 camera/estimation 전체 job과 clippy도 부모의 exact-SHA 검증에서 별도로 확인해야 한다.

이번 작업은 Cargo/lib registry, workflow, 원본 소스·fixture, 모델 수치·안전 기준·gate를
변경하지 않았다. push, Actions 실행, 원격 이슈·PR 변경도 수행하지 않았다.

## 추가 독립 source review: focused workflow·musl publisher

2026-10-24 부모 요청으로 `.github/workflows/python-rust-523.yml`과
`rust/crates/model-runtime/src/catalog_publish.rs`의 미커밋 변경을 읽었다.
검토 스냅샷 SHA-256은 각각
`3cc51e6386c24d61a3618559db9b8486b5083a300f1fd57e0bbf3ce9271fc0e5`,
`b9bcbe9f5d43f40af37915988b224266cbbe3a40c0018d64a8de96b63f3ddeec`다.
공유 코드/workflow는 수정하지 않았고 기능·빌드·clippy도 실행하지 않았다.

소스 기준으로 실행을 막는 결함이나 필수 코드 수정은 발견하지 못했다.

- `syscall(SYS_renameat2, AT_FDCWD, from, AT_FDCWD, to, RENAME_NOREPLACE)`는
  libc의 `renameat2` wrapper 심볼 의존만 제거하고 원자적 비덮어쓰기·errno 전달을 유지한다.
  실패 시 덮어쓰기 rename으로 우회하는 경로도 추가하지 않았다.
- 새 테스트는 성공 이동 1개와 기존 directory/file/symlink 충돌 보존 1개다.
  integration test `tests/catalog_publish.rs`가 수정 파일을 직접 포함하므로
  x64의 `--test catalog_publish`가 두 테스트를 포함한다.
  ARM의 `no_replace_tests::`는 현재 두 테스트 이름에 일치한다.
- target별 `RUSTFLAGS`의 `linker=<sysroot>/lib/rustlib/<host>/bin/rust-lld`와
  `linker-flavor=gnu-lld`는 C linker driver 없이 링크하는 조합이다.
  Rust 1.94.0의 [AArch64 musl target](https://github.com/rust-lang/rust/blob/1.94.0/compiler/rustc_target/src/spec/targets/aarch64_unknown_linux_musl.rs)은
  `crt_static_default=true`이고, [link.rs](https://github.com/rust-lang/rust/blob/1.94.0/compiler/rustc_codegen_ssa/src/back/link.rs)의
  `InferredForMusl`은 `crt_static`에 따라 self-contained CRT를 선택한다.
  따라서 현재 기본 target 설정에서 linker 변경만으로 musl CRT가 해제되는 문제는 없다.
  실제 publisher binary도 별도로 cross-build하고 ELF AArch64·INTERP 부재를 확인하도록 되어 있다.
- `kbhit::`는 feature가 활성화된 library unit을 선택하며,
  `developer_ssh`, `developer_auth`, `github_utils`, `paramsd_car_params_cli`,
  `paramsd_estimator_check`와 camera의 exact 실패 selector는 현재 소스에 존재한다.
  원래 paramsd 실패 binary도 test binary 의존 빌드에 포함된다.
- 기존 private Fast source의 checkout·HEAD/index/내용 봉인과 업로드 후
  경로·소유자·inode·marker 확인을 거친 삭제 구조를 유지한다.
  새 workflow는 추가로 source 봉인 성공 후에만 cleanup을 허용한다.

이 focused job의 범위는 회귀·컴파일 확인이다. ARM에서는 helper 두 테스트만 QEMU로
실행하며 publisher CLI의 ARM 실행까지 증명하지 않는다. paramsd의 ignored 원본 wire/exec
비교와 NumPy 비교, camera source generator 테스트도 이 workflow에서는 실행하지 않는다.
이들을 전체 전환·원본 동등성 완료 근거로 확대하지 않는 한 focused 실행의 blocker는 아니다.
또한 현재 trigger는 workflow 파일 변경에만 반응하므로 Rust 코드만 후속 push하면 재실행되지
않는다. 후속 코드 push도 이 job으로 검사하려면 부모가 `paths`에 대상 Rust 경로를 추가하거나
branch 내 path 필터를 제거해야 한다.

## 추가 독립 source review: browser auth·KBHit adapter

2026-10-24 `developer_ssh/login.rs`, `login/native.rs`, `http.rs`, `kbhit.rs`,
`tests/developer_auth.rs`와 원본 `openpilot/tools/lib/auth.py`, `api.py`,
`auth_config.py`, `kbhit.py`를 읽어 대조했다. 공유 파일 수정이나 기능 실행은 하지 않았다.
일반/JWT CLI·caller 미완료와 원본 21개 synthetic 입력 비교의 명시된 범위는 결함으로
계산하지 않았다. P1은 발견하지 못했으며, 구체적인 P2 지적은 다음 1건이다.

- `rust/crates/dev-tools/src/kbhit.rs:97`: `from_stdin()`의 `rustix::io::dup()`는
  CLOEXEC를 설정하지 않는다. atexit 상태가 복제 FD를 계속 보유하므로 이후 Rust
  `Command`가 자식을 실행할 때 stdin을 null로 지정해도 원래 터미널의 추가 FD가
  상속된다. 원본 Python은 fd 0을 참조할 뿐 이 추가 inheritable FD를 만들지 않는다.
  수정안은 `rustix::io::fcntl_dupfd_cloexec(rustix::stdio::stdin(), 0)`로 원자적으로
  복제하는 것이다. 실제 stdin을 건드리지 않는 private PTY 기반 복제 경계 테스트에서
  `fcntl_getfd()`의 `FdFlags::CLOEXEC`를 확인하도록 제안한다. 현재 bound-reader 테스트는
  `from_stdin()`을 통과하지 않으므로 이 누락을 검출하지 못한다.

로그인 검증 후 저장 순서, callback의 첫 결과 보존, provider/code 검증, HTTP POST의
301/302/303 GET 전환 및 307/308 body 보존에서 추가 P1/P2는 발견하지 못했다.
이는 정적 검토 결과이며 네트워크/브라우저/터미널 및 CI 통과 증거는 아니다.

## 추가 독립 source review: 실제 HTTP redirect 비교

2026-10-24 미커밋 `tests/developer_http.rs`(SHA-256
`cd3bf67f3648cf58c38ee003e0134b8ebb68658e2992410f9720f0bf42d2bea8`)와 Cargo 등록을
읽었다. 등록의 feature 조건과 POST form bytes·301/302/303 GET 전환·307/308 body 보존,
cookie/auth/user-agent 비교에서 P1은 발견하지 못했다. 기능 실행이나 공유 파일 수정은 없다.

- P2: Python peer만 `Session.trust_env=False`다. native는 ureq 기본
  `Proxy::try_from_env()`를 사용하므로 proxy 환경이 있는 runner에서 loopback까지 proxy로
  우회해 비교가 실패할 수 있다. focused 실행의 환경에 `NO_PROXY`와 `no_proxy`를 모두
  `127.0.0.1,localhost`로 설정하거나, native client를 별도 자식 프로세스에서 proxy 환경을
  제거하고 실행하도록 제안한다. 병렬 Rust 테스트 프로세스 내부의 전역 환경 변경은 피한다.
- P2: 검토 시점 focused workflow의 명시적 test/clippy 목록에 `--test developer_http`가
  없다. Cargo 등록만으로 이 목록에 포함되지 않으므로 후속 커밋에 두 selector를 추가해야 한다.
- 앞선 `KBHit::from_stdin()`의 CLOEXEC 지적도 검토 시점에는 아직 수정되지 않았다.

첫 focused run `40078269062`의 checkout 전 실패는 부모가 전달한
`1512144896 < 2147483648` 용량 사전검사 결과로 기록한다. 이 검토에서 run을 재실행하거나
진행 중 metadata run `40078386189`의 결과를 추정하지 않았다.

## 기존 ignored 경계 검사 2개를 위한 fresh 캡처 진단

2026-10-24 `tools/ci/paramsd_native_boundaries_523.sh`를 추가했다. 상태는 **UNRUN**이다.
로컬에서는 shell/embedded Python 문법과 diff만 확인하며 NumPy·pycapnp import, 원본 캡처,
native CLI 및 Rust test executable은 실행하지 않는다. workflow와 Cargo 등록은 수정하지 않는다.

이 진단은 부모가 준비한 현재 source의 실행 파일 3개를 경로·SHA-256으로 받아 held FD로
실행한다. 스크립트 안에는 apt/pip 설치, Cargo build, Cargo test의 암묵적 재빌드, 네트워크,
기기·인증정보 탐색이 없다. 원본 Python 모듈은 현재 checkout의 source bytes 전체에서 로드한다.
`paramsd_source.load()`와 `check_paramsd.main()`은 실행하지 않는다.

- 원본 `paramsd_fixture.car_params()`와 실제 `generate-paramsd-car-params`를 각각
  3개 profile × fingerprint `default`/`empty`/`custom`으로 호출하여 9쌍을 새로 만든다.
  custom 문자열에는 UTF-8 문자를 포함한다. 두 결과 모두 새 proof 디렉터리에 저장하며
  기존 fixture/과거 산출물을 복사하지 않는다.
- NumPy **2.5.3**의 실제 `np.isclose(..., equal_nan=True)` 결과를 f64 입력 비트와 함께
  9,600개 저장한다. 32개 비트 패턴의 순서 있는 9개 tolerance 전수 조합 9,216개와
  현재 두 tolerance의 threshold/인접 ULP·역방향 관측 384개다.
  signed zero, subnormal/normal 경계, 큰 유한값, infinity, NaN, 비대칭과 overflow/underflow를
  포함한다. 기대값을 Rust 출력이나 복사한 판정 수식으로 생성하지 않는다.
- 현재 tolerance `1e-9`와 `2e-6`의 2,432개 관측은 원본 `check_paramsd.compare()`에도
  실제 전달하여 NumPy 결과와 원본 path별 정책이 일치하는지 확인한다.
  경계 입력 생성의 산술은 후보 선택에만 쓰며 판정 기대값은 항상 실제 NumPy에서 얻는다.
- 캡처 뒤 아래 **기존 ignored selector만 각각 1개씩** 실행한다. 이름·exit 0·
  `1 passed; 0 failed; 0 ignored`를 모두 확인한다. 전체 suite나 이미 성공한 다른 항목은
  실행하지 않는다.

```text
paramsd_car_params_cli::all_cli_profiles_and_fingerprints_match_fresh_python_fixture_bytes
paramsd_estimator_check::matches_fresh_numpy_comparison_boundary_observations
```

위 표기의 `::` 앞은 Cargo test target 이름이다. 실제 libtest에는 뒤 함수명과
`--ignored --exact --nocapture --test-threads=1 --color=never`를 전달한다.
원본/runtime tolerance나 Rust 수학 구현은 변경하지 않는다. `np.errstate`는 이 캡처 안에서
경계 입력의 경고만 억제하며, 실제 bool 결과와 모든 입력 비트를 그대로 기록한다.

부모 통합에 필요한 환경변수는 다음과 같다. 모두 현재 source로 새로 준비해야 하며
과거 archive의 compiled executable을 전달하면 안 된다.

| 환경변수 | 부모가 전달할 값 |
| --- | --- |
| `GITHUB_WORKSPACE`, `EXPECTED_SOURCE` | 실제 checkout의 절대 경로와 현재 40자리 HEAD SHA |
| `GITHUB_ACTIONS`, `GITHUB_REPOSITORY`, `RUNNER_NAME`, `RUNNER_OS` | `true`, `bin9208/openpilot-rust`, `bin-actions`, `Linux` |
| `RUNNER_TEMP`, `PARAMSD_BOUNDARY_PROOF` | private temp와 그 하위의 아직 없는 절대 proof 경로 |
| `CARGO_TARGET_DIR` | 부모가 사용한 현재 fresh target의 절대 경로 |
| `PARAMSD_ORACLE_PYTHON` | NumPy 2.5.3/pycapnp가 준비된 기존 venv의 절대 Python 경로 |
| `PARAMSD_GENERATOR_BINARY`, `PARAMSD_GENERATOR_SHA256` | 실제 새 `generate-paramsd-car-params` executable과 그 SHA-256 |
| `PARAMSD_CAR_TEST_BINARY`, `PARAMSD_CAR_TEST_SHA256` | 새 `paramsd_car_params_cli` test executable과 SHA-256 |
| `PARAMSD_NUMPY_TEST_BINARY`, `PARAMSD_NUMPY_TEST_SHA256` | 새 `paramsd_estimator_check` test executable과 SHA-256 |

부모의 준비 명령 예시는 다음과 같다. 이 문서 작성 중 실행하지 않았다.

```bash
cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features paramsd-fixture-native --bin generate-paramsd-car-params
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features paramsd-fixture-native --test paramsd_car_params_cli --test paramsd_estimator_check --no-run --message-format=json
# compiler-artifact의 target.name/profile.test/executable로 각 정확 test executable을 선택하고 해시를 고정한다.
bash tools/ci/paramsd_native_boundaries_523.sh
```

스크립트는 source/원본 pin, 실행 파일 FD·identity/hash와 실제 import/NumPy·pycapnp 공유
provider를 전후 확인한다. proof 64 MiB의 작업별 growth admission, 파일별 8 MiB 제한,
GNU timeout 240초 그룹과 자식별 60초 kill/reap을 적용한다. native/test 자식은 최소 환경과
`PATH=/nonexistent`를 쓰고, 실패 산출물도 보존한다. 부모는 실패 시에도 proof 전체를
upload하고 기존 whole-source/build receipt 봉인을 유지해야 한다.

`complete.json`은 fresh 캡처 9쌍·9,600개와 ignored selector 2개 및 최종 guard가 모두
성공한 뒤에만 생성된다. **현재 이 성공 증거는 없다.**
`maintained_atomic_gate_uses_exact_native_generator_exec`는 실제 startup gate를 실행하지
않으므로 제외한다. 좁은 CLI 실행 기록을 그 gate의 증거로 만들지 않으며 해당 세 번째
ignored 항목과 runtime/실기 수용은 계속 미완료다.
