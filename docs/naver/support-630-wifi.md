# #630 Wi-Fi 독립 변경 기록 — 2026-10-29

## 범위와 현재 상태

- 실행 묶음: [#630](https://github.com/bin9208/openpilot-rust/issues/630).
- 담당: [#395](https://github.com/bin9208/openpilot-rust/issues/395),
  [#397](https://github.com/bin9208/openpilot-rust/issues/397).
- 작업 디렉터리: `/home/fma1/.codex/worktrees/support-services-630/openpilot-rust`.
- 브랜치: `codex/feat-630-independent-support`.
- 기준: `origin/dev=e445c9d19c3041bc79d8624589e2eb6ba7d061c6`.
- 읽기 원본: `126fdbea36598566d1040be0a35a365c909be4e4`.
- #397은 소스와 Wi-Fi 전용 의미/할당 회귀를 분리했다. 정확 소스 Actions는 미실행이다.
- #395의 기존 tether 수정과 실제 cleanup 회귀 3개를 분리 반영했다. 공용 API는
  별도 담당자가 제공하는 합의된 메서드를 사용한다. 현재 기능 CI 성공 근거는 없다.
- 공용 helper, CI, workspace, manifest, lock, provider, 설치·안전 경로를 수정하지 않았다.
  다른 에이전트의 변경과 Git 인덱스·브랜치·원격 상태도 변경하지 않았다.

## #397 적용 내용과 원본 대조

기존 수정 `2176c4a518ea087d3f69cd10b598a872592bc2f2`와 계측 근거
`1364e48c96e888fa5d178c5eb2132d135cf30765`를 읽었다. 생산용
`rust/crates/wifi/src/state.rs`는 읽기 원본의 해당 파일을 그대로 재사용했다.
`State::snapshot`에서 곧 덮어쓸 `networks`, `saved_ssids`,
`connecting_to_ssid`, `connected_ssid`의 깊은 복사를 없앴다.
`Snapshot`의 9개 필드는 직접 구성하며 `State::networks`의 정렬은 바꾸지 않았다.

기존 `wifi_snapshot_allocation_probe.rs`의 46개 의미 시나리오와 이전 알고리즘을
`rust/crates/wifi/tests/snapshot/cases.rs`로 이동했다. 일반 package 회귀
`tests/snapshot.rs`와 standalone probe가 이를 같이 사용한다. SSID/상태/요금제 조합,
직접 정한 정렬 결과, 대소문자·유니코드 동률의 안정 정렬, 오래된 파생값 교체,
반환값 변경 이후 원본 불변성, 빈 상태 검사를 보존한다. 기존 probe의 8/1,024개
네트워크·각 8회 측정과 allocator calls/요청 bytes의 정확 차이 검사는 유지했다.
비율 허용오차를 추가하지 않았다. 이 native 이전/이후 알고리즘 비교는 독립 Python
정책 oracle을 대체하지 않는다.

이전 [Actions](https://github.com/bin9208/openpilot-rust/actions/runs/39190411681)의
할당 수치는 과거 소스의 근거다. 현재 패치의 성공·실기 CPU·최대/상주 메모리 개선을
뜻하지 않는다. 기존 독립 비교 도구 `check_wifi_policy.py`, `check_wifi_runtime.py`,
`check_wifi_callback_ordering.py`와 private NetworkManager fixture는 변경하지 않았다.

## #395 적용 내용과 공용 API 조율 결과

사용자가 공용 `rust/crates/process-supervision/src/launch.rs`의 최소 예외를 확인했고,
modem 담당 Sartre가 기존 `spawn_session_inherited_with_env` 메서드를 제공하기로 했다.
이 Wi-Fi 작업자는 공용 파일을 수정하지 않았다. 최초 확인 시 메서드가 없었지만,
이번 사용자 지시에 따라 제공 합의된 API를 사용하는 원래 Wi-Fi 패치를 즉시 반영했다.
공용 API 실파일 반영 여부와 정확 소스 기능 CI 성공은 별도로 확인한다.

기존 수정 `2176c4a518ea087d3f69cd10b598a872592bc2f2`를 기반으로 읽기 원본
`126fdbea36598566d1040be0a35a365c909be4e4`에서 다음 두 파일만 재사용했다.

- `rust/crates/wifi/src/tether.rs`: inherited 표준 스트림을 유지하는 session spawn,
  `waitid(EXITED|NOHANG|NOWAIT)`로 leader를 reap하지 않는 상태 확인,
  group kill 뒤 마지막 leader reap. 실행 중 취소와 이미 종료한 leader 양쪽에서
  소유 group 정리를 먼저 수행하여 leader PID/group 식별자를 정리 완료까지 보존한다.
- `rust/crates/wifi/src/tether/cleanup_tests.rs`: 실행 중 leader의 취소,
  이미 종료한 leader와 남은 자손, 정상 nonzero 종료 세 경우의 실제 pidfd 회귀.
  초기 `2176c4a5` 이후의 atomic readiness 보강을 포함한다.
  private nonce/FIFO/helper와 실제 생산 `CommandChild` Drop 경로를 사용하고
  host sudo/sysctl/NM을 호출하지 않는다. 실패 시 pidfd/nonce 기반 fallback 정리를
  보존하며 timeout과 생존 assertions를 약화하지 않았다.

공용 메서드는 `StreamMode::Inherited`, 전달받은 child 환경, `new_session=true`로
기존 `spawn_with_stdio`를 호출하는 계약이다. `spawn_inherited_with_env`는 session을
만들지 않아 대체하지 않았으며 stdin EOF, PTY, 병합 pipe나 외부 setsid로 우회하지 않았다.

`sudo sysctl net.ipv4.ip_forward=0` argv, activation 뒤 5초 지연,
20ms 완료 polling, NetworkManager 설정·이벤트 순서는 그대로 보존했다.
새 의존성·manifest·lock 변경은 없다. 기존 process-supervision의 rustix
process/fs/event feature를 사용한다. snapshot 변경과 helper 수명 변경은 별도 파일과
회귀로 구분하며 두 변경 모두 정확 소스 Actions 결과는 아직 없다.

## 로컬 검증

로컬은 소스 읽기, 파일 분리, Rust 문법/서식, diff 확인만 수행했다.
`cargo test/build/clippy`, probe 실행, 원본 기능 비교, Actions 실행·취소는 하지 않았다.

```bash
cd /home/fma1/.codex/worktrees/support-services-630/openpilot-rust
rustfmt --edition 2021 --check rust/crates/wifi/src/tether.rs rust/crates/wifi/src/tether/cleanup_tests.rs rust/crates/wifi/src/state.rs rust/crates/wifi/tests/snapshot.rs rust/crates/wifi/tests/snapshot/cases.rs rust/tools/wifi_snapshot_allocation_probe.rs
git diff --check -- rust/crates/wifi/src/state.rs rust/crates/wifi/src/tether.rs
git diff --exit-code 126fdbea36598566d1040be0a35a365c909be4e4 -- rust/crates/wifi/src/state.rs rust/crates/wifi/src/tether.rs
```

위 검사는 통과했다. 신규 파일의 trailing whitespace와 마지막 개행도 읽기 검사했다.
과거 probe에서 분리한 함수 본문과 현재 함수 본문을 공백·가시성만 정규화해 비교했고,
할당 측정 함수와 assertions가 동일함을 소스 수준에서 확인했다. 기능 성공 근거는 아니다.

## GitHub Actions에 필요한 정확 명령

다음은 상위 담당자가 기존 `bin-actions`의 집중 검증에 넣을 명령이다.
현재 공용 `.github/workflows/rust.yml`의 ui-connectivity는 이 신규 standalone probe를
자동 실행하지 않는다. CI 소유자가 아래 명령을 집중 실행 경로에 연결해야 한다.
공용 CI 파일은 이 패치에서 수정하지 않았다. 모든 명령은 검증할 정확 SHA checkout의
저장소 루트에서 실행하고, `SOURCE_COMMIT`, 출력 및 실패 로그를 보존한다.
로컬 작업 디렉터리와 Actions checkout 경로는 서로 다르므로 runner에서는 아래
`GITHUB_WORKSPACE`를 사용한다.

```bash
cd "$GITHUB_WORKSPACE"
set -euo pipefail
export PYTHONPATH="$GITHUB_WORKSPACE:$GITHUB_WORKSPACE/rust/tools"
mkdir -p "$RUNNER_TEMP/support-630-wifi"
git rev-parse HEAD > "$RUNNER_TEMP/support-630-wifi/SOURCE_COMMIT"
cargo +1.94.0 build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-wifi --lib --examples --locked
cargo +1.94.0 build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-process-supervision --bin openpilot-process-child --locked
export WIFI_PROCESS_CHILD_BINARY="$GITHUB_WORKSPACE/rust/target/debug/openpilot-process-child"
cargo +1.94.0 test --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-wifi --all-targets --locked
cargo +1.94.0 clippy --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-wifi --all-targets --locked -- -D warnings
rustc +1.94.0 --edition=2021 "$GITHUB_WORKSPACE/rust/tools/wifi_snapshot_allocation_probe.rs" --extern "openpilot_wifi=$GITHUB_WORKSPACE/rust/target/debug/libopenpilot_wifi.rlib" -L "dependency=$GITHUB_WORKSPACE/rust/target/debug/deps" -o "$RUNNER_TEMP/support-630-wifi/snapshot-probe"
"$RUNNER_TEMP/support-630-wifi/snapshot-probe" --require-improvement | tee "$RUNNER_TEMP/support-630-wifi/snapshot-allocation.jsonl"
python rust/tools/build_params_python.py --output "$RUNNER_TEMP/support-630-wifi/params-binding"
python rust/tools/check_wifi_policy.py --binary "$GITHUB_WORKSPACE/rust/target/debug/examples/wifi_trace" --output "$RUNNER_TEMP/support-630-wifi/policy"
python rust/tools/check_wifi_runtime.py --binary "$GITHUB_WORKSPACE/rust/target/debug/examples/wifi_native" --launcher "$WIFI_PROCESS_CHILD_BINARY" --binding "$RUNNER_TEMP/support-630-wifi/params-binding"/params_pyx*.so --output "$RUNNER_TEMP/support-630-wifi/runtime"
python rust/tools/check_wifi_callback_ordering.py --binary "$GITHUB_WORKSPACE/rust/target/debug/examples/wifi_native" --launcher "$WIFI_PROCESS_CHILD_BINARY" --binding "$RUNNER_TEMP/support-630-wifi/params-binding"/params_pyx*.so --output "$RUNNER_TEMP/support-630-wifi/callbacks"
```

원본 비교의 Python/native 의존성은 기존 ui-connectivity job의 준비 단계를 사용한다.
새 provider/bootstrap을 추가하지 않는다. `CARGO_TARGET_DIR`를 별도로 지정한 runner는
위 `rust/target/debug` 경로를 실제 target 디렉터리로 일괄 조정해야 한다.

#395의 두 파일이 반영되어 위 all-targets 검사는 세 cleanup 회귀를 포함한다.
명시적인 집중 명령은 다음과 같다. fresh launcher를 사용하고 테스트 3개가 실제로
실행되는지 확인하며, 0개 통과를 #395 성공으로 보고하면 안 된다.

```bash
cd "$GITHUB_WORKSPACE"
export WIFI_PROCESS_CHILD_BINARY="$GITHUB_WORKSPACE/rust/target/debug/openpilot-process-child"
cargo +1.94.0 test --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-wifi --lib tether::cleanup_tests --locked -- --nocapture
```

영향받는 ARM build는 기존 aarch64 QEMU/PRoot 환경을 보존하여 다음을 수행한다.
QEMU/PRoot 안의 ARM native toolchain에서 루트와 target 경로를 동일하게 맞춘 뒤,
위 semantic/probe/private protocol 명령도 실행해야 하며 cross build만으로 대체하지 않는다.
ARM pidfd 또는 private D-Bus 전제 실패는 성공이나 skip으로 바꾸지 않는다.

```bash
cd "$GITHUB_WORKSPACE"
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo +1.94.0 build --manifest-path "$GITHUB_WORKSPACE/rust/Cargo.toml" -p openpilot-wifi --lib --examples --locked --target aarch64-unknown-linux-gnu
```

## 미완료 항목

- #395 공용 담당자의 inherited-session API 실파일 반영 확인 및 정확 SHA cleanup 회귀 3개 결과.
- #397의 정확 SHA 의미/할당·독립 Python 정책/private protocol/콜백·strict Clippy 결과.
- 영향받는 ARM 실행과 필수 PR·병합 후 검사. 개발·통합 완료와 사용자 실기 수용은 별도다.
- 최종 커밋 SHA·PR·Actions 링크는 일괄 검토·커밋 담당자가 추가한다.
