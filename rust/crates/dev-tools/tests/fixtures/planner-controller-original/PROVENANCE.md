# Planner controller 원본 관측의 데이터 전용 fixture

2026-10-26. 소유 범위는 새 `tests/planner_controller_frozen.rs`, 이 문서와 아래 JSON5개다.
기존 controller/source5/Python/Cargo/workflow/공유 pin은 수정하지 않는다.
이 fixture와 reader의 추가는 원본 import 퇴역이나 새 native 테스트의 실제 PASS를 뜻하지 않는다.

## 원본 실행과 추출

- source: `73e17ff10ec9be7a7001ab9cbf415a779c436f78`.
- [run40220990602/job121252313388](https://github.com/bin9208/openpilot-rust/actions/runs/40220990602/job/121252313388).
- artifact12609602768:1595595B/SHA256
  `53128e493236911d6c7b7b8468f5708eb654bb7652a436da6a4b66b0604fd576`.
- 기존 private ZIP:
  `.analysis/scratch/2026-10-25-523-completion/ci/12609602768.zip`.
  부모가 safe unique/nosymlink/CRC 검사를 완료한49개 member의 보존물이다.
  작성자도 ZIP 전체 크기/SHA와 선택 member의 CRC·bytes를 읽어 확인했다.
- 원본 corpus의 설계와 실제 성공 감사는
  [기존 기록](/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust/docs/naver/python-rust-523-planner-original-corpus.md)에 있다.
  artifact의 `original-planner-controller.log`는 source5의5 passed/0 failed를 기록한다.
  동일 cohort의 native14 성공과 새 frozen reader의 아직 실행하지 않은5개를 구분한다.

다음 다섯 파일은 `planner-original-corpus/<test_name>.json`을 **그대로 복사**했다.
파일명만 짧게 정했고 JSON key·경로·코드 문자열·stdout·bit 정수·공백·마지막 newline은
바꾸지 않았다. 합계50422B다. JSON5 문법으로 변환한 것이 아니라 JSON 파일5개다.

| 이 디렉터리의 파일 | 원래 test_name | bytes | 전체 SHA256 |
| --- | --- | --- | --- |
| `scheduler-finite.json` | `source_scheduler_matches_native_protocol_without_reimplementing_the_oracle` | 16525 | `0542466a70770f8a04c9c81893ecfe4c3252daf00b2935eeba5c3c6a4305a7bb` |
| `scheduler-ieee.json` | `source_public_scheduler_preserves_python_ordered_comparison_at_float_boundaries` | 4661 | `5ef56d3d67666a172cbc18ee71229d7a6f31d9791bf7a1944f3fa588a64bc53a` |
| `runtime-plan.json` | `source_runtime_plan_matches_every_native_leaf_and_argument` | 17424 | `fe1d7e233882afda6214627e090f702f8498c0222ec39bc5e577b53adbdf6ebd` |
| `memory-plan.json` | `source_memory_subset_matches_every_native_sanitizer_leaf_argument` | 7212 | `b73af7d13a5d8dd2e7522d2e016bc21300d884792c16dd7ab2028539bb60d9ce` |
| `failure-forward.json` | `source_failure_forward_matches_native_controller_and_raw_log` | 4600 | `dddc951fd972043d0170e9ad3f90988245099974f594d2f7e52b61ea0aeccd3f` |

`original_result`와 raw first.log는 원래 Python 실행에서 나온 값이다. 새 native 실행값으로
expected를 재계산하거나 파일을 갱신하는 생성기는 없다. capture가 담은 Python snippet은
문자열 데이터로만 읽고 실행하지 않는다.

## source2 + Rust8과 canonical Python 봉인

다섯 관측의 schema_version1/kind/test_name/source73/source map/Python proof가 동일 계약을
만족하는지 확인했다. 아래10개 SHA는 exact source73 Git blob을 직접 해시해 일치시켰다.
새 reader는 이 **역사 provenance 상수**와 전체 파일 SHA를 확인한다. 현재 checkout에서
옛 Python/Rust source를 열거나 그 source로 expected를 만들지 않는다. 따라서 나중에 원본을
퇴역하더라도 fixture의 역사 SHA를 바꿀 필요가 없다.

| source | SHA256 |
| --- | --- |
| `rust/tools/check_plannerd_ci.py` | `dda14614fe042939bd6758cffda69da95dc141ca8ce4b0c7e67d046ebaff2157` |
| `rust/tools/plannerd_ipc.py` | `c2099127901a2883925305dc3bf0c563b338494cd5bd829cb7bd3e916ffdd031` |
| `rust/crates/dev-tools/tests/planner_ci_controller.rs` | `a5e38fcfff88611926b04c334988f05faaf63754362b6d4989d4771668a3493f` |
| `rust/crates/dev-tools/src/planner_ci_controller.rs` | `c5b738aaba3f219307611d2bdd6efee92c341487a0463e5a9d484394af069b62` |
| `rust/crates/dev-tools/src/planner_ci_controller/cli.rs` | `f336e849a7554ea154df1c55ee4f721a894eafd1585075ae97556d6cbe8bc87e` |
| `rust/crates/dev-tools/src/planner_ci_controller/plan.rs` | `c1dddd235346b8f415c9b131aba7a371cb67ae80d0db5f13a86b450c4d0a8b44` |
| `rust/crates/dev-tools/src/planner_ci_controller/scheduler.rs` | `a35ad7f6c4eab2348ef3ceae318e8bd37980e251e84e59d5d17480e7b9d55f0a` |
| `rust/crates/dev-tools/src/bin/check_plannerd_ci.rs` | `32d2b777eec4ff960ea70195aa0d7b6ac553aee8621658462033ddcf933985d9` |
| `rust/crates/dev-tools/src/qa_process.rs` | `aa9fda1ed867fe6dcf7befb19fc6857fa419bbaa33fcc518264276abef7c6633` |
| `rust/crates/disk-space/src/lib.rs` | `fa920db8d68ffbe2772c8c60a7e793346427b5d9add0a81184d3fa162ea34bd0` |

canonical executable은 `/usr/bin/python3.12`, SHA256은
`e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f`,
version proof 원문은 `Python 3.12.3\n`이다. capture5개의 python 객체와 동일 artifact의
`planner-oracle-python.sha256`/`.version` 원문이 일치한다. 새 reader는 이 값과 proof의
정확한 문자열을 확인하지만 그 실행물을 현재 host에서 찾거나 실행하지 않는다.

## 기대값과 명시적 경로 치환

공통 reader는 파일 전체 SHA를 먼저 확인하고 `deny_unknown_fields` 구조체로 schema를
읽는다. schema/kind/test_name/source/Python/outer returncode0 및 stderr 빈 값,
마지막 stdout JSON과 original_result의 일치도 확인한다. failure의 outer returncode0는
관측 wrapper의 성공이며 실제 child42와 구분한다.

finite76과 IEEE 특수9는 `cases.timestamps`의 u64를 `f64::from_bits`로만 복원한다.
expected는 `original_result`의 u64를 그대로 사용하고 부동소수 JSON으로 축소하지 않는다.
finite76은 실제 `scheduler::serve`의 입력/출력 protocol을, 특수9는 public
`scheduler::next_model_target`을 확인한다. NaN payload/signed-zero/무한대를 포함해
최종 `to_bits()` 전체 벡터가 원본과 같아야 한다. 수학식을 복제하거나 tolerance를 쓰지 않는다.

runtime12/memory3은 각각 실제 `plan::runtime_checks`/`plan::memory_checks`와
`Check::invocation().effective_argv()`의 **전체 순서·name·argv**를 비교한다.
성장량 map의 이름도 기록된 leaf에서 가져오며 expected 명령은 native builder에서 만들지 않는다.
이 테스트는 plan 생성만 하며 argv에 들어 있는 Python/leaf/solver를 실행하지 않는다.

각 plan의 원본 경로에서 새 합성 Inputs로 다음7개 치환만 허용한다.
`B`는 runtime의 `/planner frozen/runtime`, memory의 `/planner frozen/memory`다.

| 원본의 고정 필드 | native 입력 | 적용 범위 |
| --- | --- | --- |
| capture `argv[4]`의 exact source root | `B/repository` | 동일 경로 또는 `/` component 하위 |
| `python.executable` | `B/python` | exact file만 |
| `cases.binaries` | `B/bin` 또는 memory의 `B/wrappers` | 동일 경로 또는 component 하위 |
| `cases.artifact` | `B/artifact` | 동일 경로 또는 component 하위 |
| `cases.source` | `B/source` | 동일 경로 또는 component 하위 |
| `cases.binding` | `B/params.so` | exact file만 |
| `cases.output` | `B/output` | 동일 경로 또는 component 하위 |

이 source root는
`/home/actions/actions-runner/_work/openpilot-rust/openpilot-rust/.diag296-source-40220990602-1.4puVsL/source`다.
runtime cases는 `/tmp/openpilot-ci.PLvbgO/.tmpmDbI65` 아래, memory cases는
`/tmp/openpilot-ci.PLvbgO/.tmpT4nrBB` 아래의 명시된 개별 경로다.
그 temp 부모 전체를 지우거나 이름만 비교하지 않는다. 절대 argv마다 정확히 하나의
mapping만 맞아야 하고, 새 절대 경로나 모호한 치환은 실패한다. 옵션/비경로 값/순서/경로의
나머지 suffix는 보존한다. Python을 포함한 이 합성 경로는 실행이나 파일 생성 대상이 아니다.

## 다섯 번째 native process 검사의 범위

기존 `qa_process`와 controller unit에 있는 `/bin/sh -c` 실제 child fixture 방식을 재사용한다.
첫 child는 고정된 `printf 'out\n'; printf 'err\n' >&2; exit 42`이고 두 번째 child는 sentinel을
만드는 명령이다. 원본 recorded result의 code/name/첫 실패 행 수와 실제 `NativeRunner`/
`run_checks`의 결과를 비교하고, 첫 log bytes는 원본 `original_first_log_utf8`와 전체 비교한다.
후속 sentinel/후속 log/PASS receipt가 없어야 한다. 기록된 log나 returncode를 출력하도록
fixture 프로그램을 생성하지 않으며 Python script도 실행하지 않는다.

이 다섯 번째 검사는 **독립 native failure-runner 통합**이다. shell argv와 원래 Python
fixture argv가 같다는 주장은 하지 않는다. 역사 원본의 주입 free_before=2^60은 실제 host의
가용 공간과 같다고 비교하지 않으며, 현재 native는 자체 admission을 통과해야 한다.
원본 Python script의 실행 자체/전체 failure argv/기존 oracle import 폐쇄는 이 검사만으로
수용됐다고 기록하지 않는다. 기존 source5는 이번 변경에서 그대로 두며, 필요한 잔여 범위와
전환 시점은 부모가 실제 새 PASS 및 다른 소비자 조사 후 판단한다.

## 확인과 부모 통합 경계

로컬은 원문 byte equality·SHA/source map·JSON schema 확인과 source review,
지정 Rust1.94 rustfmt의 구문/서식·diff 공백 확인만 수행한다. 새 테스트5개는 **GHA UNRUN**이다.
Cargo/build/test/Clippy/oracle/모델/CI 실행0, commit/push/등록0이다.
기존 `planner_ci_controller.rs`, legacy Python, Cargo/WF/master 및 model 코드는 수정하지 않는다.

추출 후 JSON5개가 ZIP member 전체 bytes와 같음을 다시 확인했다. source-only mapping 감사에서
runtime12의 절대 argv70개와 memory3의18개는 모두 선언된 mapping 하나에만 속한다.
검사는 argv metadata만 읽으며 native plan이나 scheduler를 로컬에서 실행한 결과가 아니다.
새7파일의 공백은 `/dev/null`과의 `git diff --no-index --check`로 확인한다. 이때 정상적인
신규 파일 차이의 exit1과 실제 공백 오류 진단을 구분하고 stdout/stderr 진단이 없어야 한다.

부모가 source pin/selector를 등록한 뒤 사용할 새 target은 `planner_controller_frozen`이다.
다음은 GHA용 제안 명령이며 로컬에서 실행하지 않았다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --test planner_controller_frozen -- --nocapture
```

새 실제5 PASS가 오기 전에는 old source5/import를 이 fixture로 교체하거나 퇴역하지 않는다.
실제 기능 수용과 별도로 failure의 전체 Python argv 범위 및 다른 old-controller 소비자가
남아 있음을 위 한계와 함께 부모가 통합한다.
