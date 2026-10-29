# #630 Bluetooth prompt 소유권 수정 (#394)

- 추적: [#630](https://github.com/bin9208/openpilot-rust/issues/630),
  [#394](https://github.com/bin9208/openpilot-rust/issues/394),
  [최신 기존 구현 기록](https://github.com/bin9208/openpilot-rust/issues/394#issuecomment-6258311537).
- 작성일: 2026-10-29 KST.
- 작업 디렉터리: `/home/fma1/.codex/worktrees/support-services-630/openpilot-rust`.
- 브랜치: `codex/feat-630-independent-support`.
- 기준 `origin/dev`: `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`.
- 재사용 원본: `origin/codex/feat-296-repository-python-retirement`,
  `126fdbea36598566d1040be0a35a365c909be4e4`.
- 이전 수정 SHA: `2176c4a518ea087d3f69cd10b598a872592bc2f2`.
- 현재 상태: 담당 패치 작성 및 로컬 문법·서식·diff 확인 완료.
  현재 후보의 기능/원본 비교·strict Clippy·ARM 검사·통합은 미실행이다.
  과거 검사 결과는 이번 후보의 성공 근거로 사용하지 않는다.

## 분리한 변경

| 파일 | 변경 |
| --- | --- |
| `rust/crates/bluetooth/src/bluez/agent.rs` | interactive 요청마다 내부 UUID generation을 저장하고, 완료 시 자기 generation만 정리한다. 기존 결정적 회귀 3개를 재사용한다. |
| `rust/crates/bluetooth/src/bluez/state.rs` | `answer_owner`, `finish_answer`를 추가하고 Cancel/Release 및 전체 페어링 종료에서 소유권을 무효화한다. |
| `docs/naver/support-630-bluetooth.md` | 출처·변경 범위·검사 결과·후속 CI 명령을 기록한다. |

두 Rust 파일은 위 재사용 원본 SHA 및 이전 수정 SHA의 파일과 바이트 단위로
동일하다. 그 두 SHA 사이에서 해당 파일의 추가 변경은 없다. 전체 ref의
merge/cherry-pick은 하지 않았다. Cargo 설정·lock·공용 helper/provider·CI는
수정하지 않았고 새 의존성도 없다. 기존 `uuid` 의존성을 사용한다.

A의 응답이 접수된 뒤 A handler가 재개되기 전에 B가 prompt를 설치할 수 있다.
이전 코드는 A가 공유 prompt를 무조건 비웠다. 수정은 화면에 노출하는 prompt ID와
별개인 내부 generation으로 cleanup 소유권을 확인한다. B가 설치되면 A의 cleanup은
B의 prompt와 answer를 그대로 둔다. timeout/stop에서도 소유권 확인 후 정리한다.
Display 업데이트는 기존 interactive answer의 generation을 유지하므로 display ID로
기존 answer에 응답하는 동작을 유지한다.

D-Bus 응답 생성·passkey/PIN 변환, sender/interface/target 검사, pending 중복 거부,
60초 prompt timeout과 90초 Pair timeout은 변경하지 않는다. Python 원본에도 같은
경쟁이 있으므로 #394는 의도한 원본 결함 수정이다. wire 차이를 원본 동등성 성공으로
바꾸거나 deadline 허용 범위를 늘리지 않는다. 기존 NUL PIN 차이 #166도 별개로 유지한다.

## 재사용한 결정적 회귀

`bluez::agent::prompt_owner_tests`는 실제 `reply` future를 직접 polling하며 sleep,
실제 무선 기기나 system bus에 의존하지 않는다.

1. `previous_reply_cannot_clear_a_newer_interactive_prompt`:
   A 설치 → A에 응답(또는 Cancel/Release) → B 설치 → A 완료를 강제한다.
   B prompt ID 유지, A의 오래된 ID 거부, B 응답 성공 및 최종 prompt 정리를 확인한다.
2. `display_overlap_still_belongs_to_the_existing_answer`:
   RequestPasskey 대기 중 DisplayPinCode가 prompt를 교체한 뒤 display ID로 응답한다.
   실제 wire `u32` 응답 42와 prompt 정리를 확인한다.
3. `wrong_sender_or_target_cannot_install_a_prompt`:
   잘못된 sender와 target을 각각 거부하고 prompt/answer 미설치를 확인한다.

기존 #394 본문의 RED 기록은 2개 control 통과/경쟁 회귀 1개 실패이다.
현재 후보의 RED/GREEN 실행은 아직 하지 않았다. 상위 CI에서는 기준 소스의
`agent.rs`/`state.rs`에 위 test module만 적용한 별도 임시 소스로 RED를 확인하고,
후보 소스로 GREEN을 확인할 수 있다. 공유 checkout이나 다른 에이전트 파일을
되돌리는 방식은 사용하지 않는다.

## 로컬 검사 결과

모든 명령의 작업 디렉터리는 위 worktree였다. 기능 코드나 비교 시나리오는 실행하지 않았다.

- `rustfmt --edition 2021 --check`를 담당 `agent.rs`, `state.rs` 절대 경로에만 적용: 통과.
  이 검사는 두 파일의 Rust parsing/서식을 확인하며 타입 검사·빌드 결과는 아니다.
- 원본 `openpilot/selfdrive/carrot/bluetooth/bluez.py`와 기존 전용 비교 도구 4개,
  `rust/tools/bluetooth_bluez_fixture.py`를 `ast.parse`로 읽음: 6개 모두 문법 통과.
- 담당 파일의 `git diff --check`: 통과. 두 Rust 파일은 165줄 추가/3줄 삭제이며
  production 변경과 회귀 외 다른 모듈 변경은 포함하지 않는다.
- 파일 바이트 대조 및 diff 읽기: 기존 두 SHA와 동일함을 확인.
  `agent.rs` SHA256: `61851566de7fa98e7539e0d890533d788038443dd4c7bda807af233d70ff5a67`.
  `state.rs` SHA256: `b7b8d55f501a4810fbb1aade666c288bbe65b7ef29213eb0b4849eb9d3cdbc02`.
- cargo test/build/clippy, 원본 동작 비교, Actions 실행·취소, issue 상태 변경,
  git add/commit/push/merge 및 다른 에이전트 작업 변경은 수행하지 않았다.

## GitHub Actions에 전달할 정확한 명령

아래는 실행 요청 목록이며 로컬 실행 결과가 아니다. 상위 담당이 선정 9개 검사를
합쳐 기존 `bin-actions`에 등록한다. 각 step의 `working-directory`는 해당 실행에서
검증할 정확 SHA의 `$GITHUB_WORKSPACE`로 지정한다. 이 문서의 로컬 디렉터리를
러너에 복사하거나 로컬 산출물로 대체하지 않는다. 공용 CI/의존성 설치·runner 정책은
#598 소유이므로 여기서 수정하지 않는다.

빠른 브랜치 검사에서 syntax/format/diff와 집중 소유권 회귀를 실행한다.
전체 원본 실제 deadline 비교·패키지·strict Clippy·ARM은 PR 심화 검사에 둔다.
후보 커밋 SHA, 실행 URL, 각 결과 및 산출물 hash를 상위가 후속 기록한다.

### 집중 회귀 및 패키지 검사

Rust 1.94.0과 기존 capnproto/native prerequisite가 준비된 runner에서 실행한다.

```bash
cargo +1.94.0 test --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bluetooth --lib --locked bluez::agent::prompt_owner_tests
cargo +1.94.0 test --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bluetooth --all-targets --locked
cargo +1.94.0 clippy --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bluetooth --all-targets --locked -- -D warnings
cargo +1.94.0 build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bluetooth --bins --examples --locked
```

### 기존 원본·private-bus 비교

기존 `ui-connectivity`의 Python 3.12와 고정 의존성
`jeepney==0.9.0`, `anyio==4.12.1`, `pytest==9.0.2` 및 `dbus-daemon`을 재사용한다.
기존 job과 같은 기본 Cargo target 경로(`rust/target/debug`)를 사용하며,
별도 `CARGO_TARGET_DIR`를 쓰는 상위 runner는 binary 경로를 그 디렉터리로 지정한다.
결과 폴더는 각 run의 새 `$RUNNER_TEMP` 아래여야 한다(`exist_ok=False` 보존).

```bash
export PYTHONPATH="$GITHUB_WORKSPACE:$GITHUB_WORKSPACE/rust/tools${PYTHONPATH:+:$PYTHONPATH}"
python "$GITHUB_WORKSPACE/rust/tools/check_bluetooth_bluez_policy.py" --binary "$GITHUB_WORKSPACE/rust/target/debug/examples/bluez_policy" --output "$RUNNER_TEMP/support-630-bluetooth/policy"
python "$GITHUB_WORKSPACE/rust/tools/check_bluetooth_bluez.py" --binary "$GITHUB_WORKSPACE/rust/target/debug/examples/bluez_client" --output "$RUNNER_TEMP/support-630-bluetooth/private-bus"
python "$GITHUB_WORKSPACE/rust/tools/check_bluetooth_bluez_prompts.py" --binary "$GITHUB_WORKSPACE/rust/target/debug/examples/bluez_client" --output "$RUNNER_TEMP/support-630-bluetooth/prompts" --deadlines
python "$GITHUB_WORKSPACE/rust/tools/check_bluetooth_bluez_invalid.py" --binary "$GITHUB_WORKSPACE/rust/target/debug/examples/bluez_client" --output "$RUNNER_TEMP/support-630-bluetooth/invalid"
```

원본은 현재 dev에 존재하는 `openpilot/selfdrive/carrot/bluetooth/bluez.py`를
수정 없이 실행한다. SHA256은
`e4998d01bd8d614700e9a3ce027d7fe56e0dd870ea8c697da05e5228a51b5237`이다.
policy의 Unicode 전체 지점 비교, private bus의 sender/interface/target와 ordered calls,
실제 30초 discovery·60초 prompt·90초 Pair 범위, display overlap/변환/중복·취소,
invalid PIN의 명시적 #166 차이를 기존 assertion 그대로 검사한다. #394 경쟁 수정은
위 결정적 native 회귀로 별도 확인하며 unchanged Python과 같은 결과를 요구하지 않는다.
이 네 비교는 기존 `check_bluetooth_runtime.py`에도 포함되므로 상위가 그 전체 suite를
실행하면 원본 비교를 다시 중복 실행할 필요가 없다.

### 영향받는 ARM 검사

기존 `bin-actions`의 aarch64 QEMU/PRoot 환경에서 같은 정확 소스를 사용하여,
그 guest의 `$GITHUB_WORKSPACE`를 working-directory로 아래 명령을 실행한다.
host 교차 빌드만으로 ARM 실행 성공을 주장하지 않는다.

```bash
test "$(uname -m)" = aarch64
cargo +1.94.0 test --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bluetooth --all-targets --locked
cargo +1.94.0 build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-bluetooth --bins --examples --locked
```

## 미해결 의존성·인계

- 코드 차원의 공용 helper/provider·workspace·lock 변경은 필요하지 않다.
  기존 Bluetooth 전용 원본 비교 도구를 수정 없이 재사용한다.
- 현재 `.github/workflows/rust.yml`의 `jobs.ui-connectivity`는 원본 suite를 실행하지만,
  해당 job에 Bluetooth 단위 테스트/strict Clippy 명령은 명시돼 있지 않다.
  runner는 `ubuntu-24.04`로 선언되어 있어 이번 사용자의 `bin-actions` 정책에 맞춘
  실행 경로 지정 및 위 명령 등록은 공용 CI 소유 상위 담당의 작업이다.
  이 파일과 #598의 runner/helper를 수정하지 않았다. 기존 workspace 전체 검사와의
  중복 여부를 확인해 실행하며, 단순히 기존 원본 suite만 실행해 회귀 완료로 간주하지 않는다.
- 현재 후보 CI·원본 비교·ARM 결과, 정상 dev 통합과 병합 후 검사가 남았다.
  실제 기기 페어링 수용은 사용자가 수행하며 #394 종료 근거와 구분한다.
