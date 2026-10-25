# #523 Planner controller 두 Python 파일 퇴역 후보

2026-10-26. 지정 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
이 변경은 기존 native14 보존·live source5/collector 제거·runtime 부재 guard·새 회귀6개와
정확한 두 파일 삭제만 준비한다. **삭제 후 host/ARM 수용은 미완료이며 credit은0**이다.
부모가 workflow/contract/pin/Focus/원장과 전체 소비자 폐쇄를 source commit 전에 통합한다.

## 재사용한 실제 수용

[run40227415620/job121271640924](https://github.com/bin9208/openpilot-rust/actions/runs/40227415620/job/121271640924)의
source는 `425f29d5ffa4ae0e603ceddf5dce8c605e49bc35`다. 부모가 frozen5·native14·원본 source5·
Clippy 실제 PASS를 수용했다. 보존 artifact12612286621은1605293B/SHA256
`41abeb085169ae55372ab2cb665175dbf1ac4a7f320915f1f7682e53b83aa6c1`이며 이번에 inert ZIP
크기/SHA/SOURCE_COMMIT과 `planner-controller-frozen.log`의5 passed/0 failed를 읽었다.
원본/native를 재실행하지 않았다. 부모가 전달한 host425 runtime12/ARM6d runtime12 성공도
기존 수용이며 이번 삭제 후 gate를 대신하지 않는다.

`tests/planner_controller_frozen.rs`는16337B/SHA256
`99407e5b45dade375d3987367b367a82d4fc4ac7de81f8683098f919e1be1c0d`를 그대로 유지한다.
fixture JSON5개와 PROVENANCE도 수정하지 않는다. 역사 source2/Rust8의 SHA와 원본 stdout,
IEEE bits, complete argv/raw log는 frozen 자료에 남고 현재 Python source를 읽는 의존은 없다.

## 정확한 삭제와 runtime guard

삭제 대상은 다음 두 경로뿐이다. source 삭제 자체가 caller 폐쇄나 실제 gate PASS를 뜻하지 않는다.

| 삭제 경로 | 기존 bytes | 기존 SHA256 |
| --- | --- | --- |
| `rust/tools/check_plannerd_ci.py` | 4733 | `dda14614fe042939bd6758cffda69da95dc141ca8ce4b0c7e67d046ebaff2157` |
| `rust/tools/tests/test_plannerd_ci.py` | 25953 | `50ad835c258dfd9dbf27cf26bbafd15cd63032eab988fedcdb3e2e8d0f40b2b7` |

기존 `tests/planner_ci_controller.rs`의 live `reference`, SHA/관측 collector와 ignored source5를
제거했다. native14의 **모든 본문과 helper bytes**는 보존했다. 필요한 import 정리 외 변경은 없다.
기존 Python subprocess는 제거됐고, native14의 첫 실패·memory fixture는 원래의 shell 대역을 유지한다.

`planner_ci_controller/cli.rs`의 private `require_retired_controller_absent`는 canonical
`--root` 아래 위 exact 두 경로만 `symlink_metadata`로 검사한다. regular file·directory·
실제 target이 있는 symlink·dangling symlink 모두 거부하고 NotFound만 허용한다.
그 외 I/O 오류도 검사 실패로 반환한다. glob이나 인접 IPC/leaf source 삭제 조건은 추가하지 않는다.

default runtime은 leaf 실행 전에 검사하고, `run_checks`의 성공/실패 어느 경우에도 반환값을
전파하기 **전에** 다시 검사한다. root에 두 경로가 없으면 첫 실제 child42가 기존대로 CLI42로
전파된다. child가 경로를 복원하면 사후 guard가 CLI1로 거부하며 `commands.json`의 실제 child42와
raw log는 그대로 남긴다. 복원 위반 뒤 PASS receipt를 쓰거나 원래 실패를 성공으로 바꾸지 않는다.

`schedule`, memory core 및 `memory --asan-library`의 guard 범위/처리는 바꾸지 않는다.
특히 shared `planner_ci_controller/memory.rs`는 이 작업의 쓰기 대상이 아니다.
runtime guard 블록·helper/import만 역투영하면 원래 cli4681B/SHA256
`b06b4fed1e59cf63c449c7dbec34b221e9cfba0ec74d8bdaa65544127d147680` 전체가 정확히 복원됨을
source-only 비교로 확인했다.

## 새 native guard 회귀6개

새 `tests/planner_controller_retirement.rs`는 실제 registered Rust CLI를 실행하는 검사다.
`--python` 인자는 임시 `#!/bin/sh` native leaf 대역을 가리킨다. 전달된 Python leaf 경로는
열거나 실행하지 않으며 새 테스트에는 Python oracle/subprocess가 없다. 동적 environment
변경도 parent process에 적용하지 않고 각 `CommandSpec` overlay에만 넣는다.

| test | 확인하는 계약 |
| --- | --- |
| `empty_runtime_root_reaches_first_native_leaf_and_preserves_status42` | 빈 root에서 부재 검사가 통과하고 첫 native leaf만 실행, CLI42/abi42 행·raw log 보존, 후속 실행 없음 |
| `runtime_rejects_restored_files_and_directories_before_any_leaf_or_output` | 두 경로 각각 file/directory 복원 시 leaf와 output 생성 이전 거부 |
| `runtime_rejects_live_and_dangling_symlinks_before_any_leaf_or_output` | 두 경로 각각 live/dangling symlink 복원 시 사전 거부 |
| `runtime_rejects_file_restoration_even_after_the_first_leaf_failed` | 첫 child가 파일을 복원하고42로 실패해도 사후 guard가 거부, 실제 실패 행/로그 보존 |
| `runtime_rejects_symlink_restoration_even_after_the_first_leaf_failed` | 첫 child가 live/dangling symlink를 복원한 경우의 사후 거부 |
| `memory_mode_keeps_controller_path_presence_outside_the_runtime_guard` | 같은 controller 파일/symlink가 있어도 memory core는 기존 first42 처리 유지 |

각 복원 검사는 두 exact 경로를 모두 다룬다. 실행 중 복원 fixture의 출력/exit42는 고정된 native
shell 동작이고 guard의 expected를 계산하는 Python 대역이 아니다. file·symlink의 원래 source는
데이터 marker여서 복원된 파일도 실행되지 않는다. 새 회귀는 **GHA UNRUN**이며 정적 구문 확인을
실제 child/guard 동작 PASS로 기록하지 않는다.

## 실제 소비자 검색과 부모가 닫을 잔여 범위

파일명만 출력하는 `rg -l`로 actual caller/import를 먼저 분류하고, workflow는 YAML로 파싱해
해당 Planner step과 pin/selector만 읽었다. 삭제 대상 pytest의 `from check_plannerd_ci import …`와
Rust controller target의 source5 `runpy`/Python 실행 의존은 이번 쓰기 범위에서 제거했다.
다음 부모 소유 소비자는 조사 시점에 남아 있으므로 **이 WIP 상태는 아직 commit 가능한 완전 폐쇄가 아니다**.

| 부모 소유 경로 | 잔여 작업 |
| --- | --- |
| `.github/workflows/rust.yml` | planner-runtime compile step의 옛 `pytest … test_plannerd_ci.py`를 maintained policy21/controller14/frozen5 및 새 guard 회귀로 연결 |
| `.github/workflows/rust-native-arm.yml` | reusable ARM의 동일 pytest 실행 폐쇄 |
| `.github/workflows/python-rust-523-native-arm-more.yml` | standalone ARM의 동일 pytest 실행 폐쇄 |
| `.github/workflows/python-rust-523-planner-caller.yml` | host exact-job 진단 복사본의 동일 pytest 실행 폐쇄 |
| `.github/workflows/python-rust-523-native-units.yml` | `original-planner-controller`의 live `source_ --ignored` 실행 제거, 새 retirement target6/Clippy 등록; 원래 memory source capture는 별도 담당 |
| `.github/workflows/python-rust-523.yml` | 삭제 pytest pin/허용 목록과 변경된 controller test/CLI pin 정합성 |
| `tests/planner_ci_policy/contract.rs` | 옛 pytest 명령을 expected로 요구하는 구조 계약을 부모의 새 maintained Rust 명령과 맞춤 |

위 workflow/contract/공유 pin·Focus/원장은 수정하지 않았다. `tools/ci/planner_native_controller_523.sh`의
`PLANNER_REFERENCE_PYTHON` 표기는 leaf venv와의 역할 설명이며 old controller 실행이 아니다.
`planner_memory_controller.rs`의 해당 경로 쓰기는 이웃 controller가 존재해도 memory 자체의
정확한 두 memory 경로 guard만 적용됨을 확인하는 fixture이고 actual import/caller가 아니다.
이를 이번 controller 소비자라고 잘못 세거나 수정하지 않는다. memory source capture와 다른 두
memory Python 퇴역은 Ohm 담당으로 유지한다.

frozen JSON5/PROVENANCE/source SHA 상수와 manager-qa 역사 before/after manifest의 경로는 보존할
provenance다. 실제 import/실행이 아니므로 잔존 문자열을 없애려고 과거 자료를 수정하지 않는다.
`plannerd_ipc.py`와 각 runtime/IPC/lifecycle leaf도 이 삭제 범위에 포함되지 않는다.

## 로컬 확인과 남은 수용

- native14 본문/helper 구간13201B/SHA256
  `80a9ee8febf97de524e2e46f597b8f81944fe58802d2d069a7e5d8fc6dccff28` 보존, live ignored source5/collector 제거.
- 남은 native14, 새 retirement6, 기존 frozen5를 구분한다. frozen reader와 JSON5 bytes는 그대로다.
- Rust1.94 rustfmt의 구문/서식과 diff 공백, source-only 역투영·actual consumer 검색만 수행한다.
- 로컬 Cargo/build/test/Clippy/oracle/프로젝트 기능·새 GHA 실행0, commit/push/공유 등록0이다.

부모 source pin 통합용 후보는 다음과 같다.

| 파일 | bytes | SHA256 |
| --- | --- | --- |
| `rust/crates/dev-tools/tests/planner_ci_controller.rs` | 13605 | `1fa58a1df003913c8b16d5a580d511ef3264eb60096d76901507507f3becce1a` |
| `rust/crates/dev-tools/src/planner_ci_controller/cli.rs` | 5732 | `27884bcdbc4ea6e9a655191a57093dbc36570474b311cbb57c1df2619599f172` |
| `rust/crates/dev-tools/tests/planner_controller_retirement.rs` | 8268 | `ef35ac85e5e3e235804bde7c3879ae889f79220c42b8f382ed9bd0857a3fbcec` |

부모가 전체 caller/contract/source pin을 통합한 뒤 다음 target들을 기존 host/ARM 정책에 맞게
등록한다. 아래는 GHA selector 설명이며 로컬에서 실행하지 않았다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features -p openpilot-dev-tools --test planner_ci_policy --test planner_ci_controller --test planner_controller_frozen --test planner_controller_retirement -- --nocapture
```

삭제 후 exact-source host/ARM 및 새 guard6의 실제 수용과 독립 감사가 남아 있다.
이전425/6d 성공을 삭제 후 성공으로 재분류하지 않으며, 두 파일 삭제 credit은 그때까지0이다.
