# #630 modem helper 수명 수정 인계 (#398)

2026-10-29 KST. 작업 위치는 `/home/fma1/.codex/worktrees/support-services-630/openpilot-rust`,
브랜치는 `codex/feat-630-independent-support`다.
[#630](https://github.com/bin9208/openpilot-rust/issues/630) 및
[#398](https://github.com/bin9208/openpilot-rust/issues/398)의 본문·최신 댓글을 읽었다.
기준은 `origin/dev=e445c9d19c3041bc79d8624589e2eb6ba7d061c6`이다.

**상태: modem 구현·15개 회귀 및 승인된 공용 API 2개·전용 회귀 3개 작성 완료, 현재 후보 CI 미실행.**
공용 API 부재에 따른 소스상 차단은 해소했다. 컴파일·실행 성공은 아직 주장하지 않으며 정확 SHA CI가 필요하다.
공용 CI, workspace, Cargo.lock, provider, bootstrap, installer, 모델·안전 경로는 수정하지 않았다.
커밋·push·Actions 실행·취소·이슈 상태 변경도 하지 않았다.

## 재사용 소스와 변경 파일

- 기존 수정 SHA: `edb42bf095d6a3e3b38cc0a4546976dfb710363e`.
  요청된 읽기 원본 `origin/codex/feat-296-repository-python-retirement`의
  `126fdbea36598566d1040be0a35a365c909be4e4`와 대조했다.
  런타임 소스는 동일하며, 후자는 마지막 원본 실행 회귀를 캡처 JSON 검사로 바꿨다.
  이번에는 요구대로 `edb42bf`의 실제 unchanged-source 실행을 유지한다.
- `rust/crates/modem/Cargo.toml`: 기존 `rustix`의 `process,event` feature와
  `helper-lifetime-tests` opt-in target만 재사용한다. 새 package는 추가하지 않았다.
- `rust/crates/modem/src/{helper,lib,main,observe,ppp,runtime}.rs`:
  위 수정 SHA의 파일을 바이트 그대로 재사용했다.
- `rust/crates/modem/tests/helper_lifetime.rs`, `tests/fixtures/helper-entry.sh`:
  같은 SHA의 15개 실제 회귀 및 실행 가능 immutable fixture를 그대로 재사용했다.
- `rust/tools/check_modem_helper_lifetime.sh`: 기존 Actions의 old-source EOF RED,
  고정 소스 GREEN 15개, strict Clippy 및 기존 두 비교 명령을 모듈 전용으로 묶었다.
  로컬에서는 실행하지 않았다. 공용 workflow에 자동 연결된 상태가 아니다.
- `rust/crates/process-supervision/src/launch.rs`: 상위의 실제 분리 검토에 따라 기존
  `edb42bf`의 `spawn_session_redirected_with_env` 및 Wi-Fi #395가 요구하는
  `spawn_session_inherited_with_env` 두 메서드만 재사용했다.
- `rust/crates/process-supervision/tests/session_redirected_630.rs`: 3개 실제 launcher 회귀.
  stdin 입력을 가진 별도 probe를 실행하고 child가 살아 있는 동안 PID=PGID=SID,
  부모 session/group 보존, 환경 전달, stdout/stderr 분리 및 상속을 확인한다.
  기존 `spawn_session_redirected`의 null stdin도 대조한다. 소유 leader를 NOWAIT로
  보존한 후 caller가 group 정리·reap한다. modem의 기존 실제 자손 정리 회귀도 유지한다.
- `docs/naver/support-630-modem.md`: 이 기록.

helper leader를 `waitid(NOWAIT)`로 보존하고 group 정리 후 reap한다.
정상 종료·timeout·취소·PPP 재연결/Drop에서 같은 소유권 규칙을 적용한다.
`ip addr`는 기존 2초 deadline에 stdout/stderr EOF를 모두 포함하고 양쪽 UTF-8을 확인한다.
일반 명령에는 새 timeout을 부과하지 않으며 nonzero interface 출력의 기존 수용은 유지한다.
종료한 PPP 상태를 보존해 `WaitExit`가 never-started와 구별하도록 한 15번째 회귀도 포함한다.
첫 signal 후 정상 route/service 복원을 유지하고 후속 signal은 막힌 복원 helper를 취소한다.
명시적으로 새 session으로 탈출하는 자손까지 소유한다고 주장하지 않는다.

## 실제 분리 검토에 따른 공용 API 예외 — 소스상 의존성 해소

상위는 현재 #611/#606과 `launch.rs`가 실제로 겹치지 않음을 확인하고,
공용 파일 일괄 제외가 보수적 선정 가정이었다고 설명했다. 사용자의 #630 진행 지시 범위에서 상위 담당자가 두 기존 API와
고유 회귀 파일을 최소 의존성으로 포함하고 그 근거를 #630에 기록했다. 이 예외를 적용해 다음 메서드만 추가했다.

- `spawn_session_redirected_with_env`: modem #398용. 분리된 출력과 stdin 상속을 유지하며 새 session을 생성한다.
- `spawn_session_inherited_with_env`: Wi-Fi #395용. stdin/stdout/stderr와 환경을 상속하며 새 session을 생성한다.

두 메서드는 `edb42bf095d6a3e3b38cc0a4546976dfb710363e`와 바이트가 동일하다.
기존 public API, 호출부, StreamMode 및 내부 구현은 바꾸지 않았다. 새 의존성과 lock 변경도 없다.
modem의 `helper.rs` 호출과 Wi-Fi가 필요한 공용 메서드는 이제 소스에 존재한다.
기존 `spawn_session_redirected`의 null stdin 동작은 유지한다.
상위가 #630 이슈와 공용 기록에 두 메서드의 최소 예외를 반영한다.

```bash
cd /home/fma1/.codex/worktrees/support-services-630/openpilot-rust
git diff -- rust/crates/process-supervision/src/launch.rs
```

## 원본 비교와 과거 증거

과거 [platform job 118816129959](https://github.com/bin9208/openpilot-rust/actions/runs/39477200877/job/118816129959)은
API 재조회 결과 head `edb42bf095d6a3e3b38cc0a4546976dfb710363e`, `completed/success`였다.
[#398 최신 결과](https://github.com/bin9208/openpilot-rust/issues/398#issuecomment-6298592672)는
`1364e48c` EOF RED, 고정 15개, strict Clippy, unchanged modem differential과 process boundary 성공을 기록한다.
이는 현재 #630 후보의 성공을 대신하지 않는다.

아래 파일은 현재 기준에서 수정하지 않았다. 로컬 읽기 SHA-256이 과거 기록과 동일하다.

| 파일 | SHA-256 |
| --- | --- |
| `openpilot/system/hardware/tici/modem.py` | `c193b3f5c669dab5e488ee982cbe565539a0410193b4302d524f68c157d47a13` |
| `rust/tools/modem_source.py` | `72c75ec2185003a861e1512edf06921352bb3ff02f5b27d3e1a5a743a3964060` |
| `rust/tools/modem_fixture.py` | `07ab85b2f6dcb66750a557c7b9ed4dae32503c8b2f482d2a28e984a65c63d22a` |
| `rust/tools/check_modem.py` | `6b7df54af0d948f9569f6e933f70a63fb30c696b6e7ef47d5ddbfe960e19a922` |
| `rust/tools/check_modem_process_boundary.py` | `2a3cd0907ddc175a5c753490b11784a458d904065236c520dffd949548cd2abd` |

원본 Python에도 direct-child 정리와 일반 helper의 무기한 대기가 있으므로 이 둘은 의도한 개선이다.
EOF 차이는 실제 옛 Rust의 RED를 별도로 요구한다.

## 필요한 정확 CI 명령 — 실행하지 않음

상위가 일괄 커밋한 **정확 후보 SHA**로 등록 `bin-actions`에서 실행한다.
Python 3.12와 `pyserial==3.5` 및 기존 platform-runtime source import 환경을 그대로 사용한다.
현재 `.github/workflows/rust.yml`의 source import/build/ARM 설정을 재사용하고 이 작업에서
공용 CI를 수정하지 않았다. `RUNNER_TEMP`, source 환경, cross compiler는 Actions가 제공한다.
작업별 baseline 소스 복사·host build·ARM build 예상 증가량을 먼저 산정/승인하는 기존 절차가 필요하다.
고정 25/35 GiB 기준을 이번 패치로 추가하지 않는다.

아래는 cloud에서만 실행할 명령이다. old baseline은 별도 비어 있는 폴더에 풀며 candidate의
checkout/index/branch를 바꾸지 않는다. 스크립트가 old baseline의 modem 소스 6개를 고정 SHA와
대조한 후 테스트 파일만 복사한다. baseline 빌드 실패를 RED로 인정하지 않으며 지정 assertion과
`test result: FAILED`를 모두 요구한다.

```bash
cd "$GITHUB_WORKSPACE"
set -euo pipefail
export MODEM_SOURCE_PYTHON="$(command -v python)"
export MODEM_HELPER_EVIDENCE="$RUNNER_TEMP/platform-runtime/modem-helper-lifetime"
export CARGO_TARGET_DIR="$GITHUB_WORKSPACE/rust/target"
export MODEM398_BASELINE_ROOT="$(mktemp -d "$RUNNER_TEMP/modem398-baseline.XXXXXX")"
git fetch --no-tags --depth=1 origin 1364e48c96e888fa5d178c5eb2132d135cf30765
git archive 1364e48c96e888fa5d178c5eb2132d135cf30765 | tar -x -C "$MODEM398_BASELINE_ROOT"
bash "$GITHUB_WORKSPACE/rust/tools/check_modem_helper_lifetime.sh"
```

모듈 전용 스크립트의 고정 소스 검사 명령은 다음과 같다. 위 스크립트 실행과 별개로
중복 실행할 목록이 아니라, 검토할 실제 명령의 명시다.

```bash
cd "$GITHUB_WORKSPACE"
cargo build --manifest-path rust/Cargo.toml -p openpilot-modem -p openpilot-process-supervision --bins --examples --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-process-supervision --test session_redirected_630 --locked
MODEM_PROCESS_CHILD_BINARY="$CARGO_TARGET_DIR/debug/openpilot-process-child" cargo test --manifest-path rust/Cargo.toml -p openpilot-modem --features helper-lifetime-tests --test helper_lifetime --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-modem --features openpilot-modem/helper-lifetime-tests -p openpilot-process-supervision --all-targets --locked -- -D warnings
"$MODEM_SOURCE_PYTHON" rust/tools/check_modem_process_boundary.py --target "$CARGO_TARGET_DIR" --evidence "$MODEM_HELPER_EVIDENCE/process-boundary"
"$MODEM_SOURCE_PYTHON" rust/tools/check_modem.py --binary "$CARGO_TARGET_DIR/debug/openpilot-modem" --trace "$CARGO_TARGET_DIR/debug/examples/modem_trace" --evidence "$MODEM_HELPER_EVIDENCE/source-comparison"
```

영향받는 ARM 빌드도 기존 cross compiler/sysroot 환경에서 필요하다.
기존 ARM 원본 비교/QEMU·PRoot gate 및 필수 PR/병합 후 검사를 이 한 명령으로 대체하지 않는다.

```bash
cd "$GITHUB_WORKSPACE"
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --manifest-path rust/Cargo.toml -p openpilot-modem -p openpilot-process-supervision --bins --examples --target aarch64-unknown-linux-gnu --locked
```

`MODEM_HELPER_EVIDENCE` 아래 baseline.log/fixed.log/session-630.log/fixture별 pid acknowledgement·호출·stderr와
두 비교의 JSON을 보존하고 후보 SHA/Actions URL을 상위가 추가해야 한다.
두 원본 비교 도구는 상태·reconnect·명령 순서와 descriptor closure/no-shell-fallback을 그대로 확인한다.
실제 모뎀·서비스·기기 설치·부팅·주행 검증은 수행하지 않았다.

## 로컬 검증 결과

모든 명령의 workdir는 `/home/fma1/.codex/worktrees/support-services-630/openpilot-rust`로 지정했다.

- 담당 Rust 7개 파일만 `rustfmt --edition 2021 --config skip_children=true --check`: 통과.
  crate 외부와 변경 없는 Rust 파일은 포맷하지 않았다.
- `bash -n rust/tools/check_modem_helper_lifetime.sh`,
  `sh -n rust/crates/modem/tests/fixtures/helper-entry.sh`: 통과.
- Python `tomllib`로 담당 Cargo.toml 구문 분석: 통과.
- 기존 `check_modem.py`, `check_modem_process_boundary.py`, `modem_source.py`,
  `modem_fixture.py`, `modem_fixture_command.py`는 Python `ast.parse`만 수행: 통과.
- `git diff --check -- rust/crates/modem rust/tools/check_modem_helper_lifetime.sh docs/naver/support-630-modem.md`
  및 새 파일 trailing whitespace 확인: 통과.
- 9개 재사용 파일은 `edb42bf` 바이트와 동일하고 테스트 선언은 15개임을 읽기 검사했다.
- `cargo test/build/clippy`, checker 기능 실행, 원본 subprocess 비교, ARM 검사는 로컬에서 실행하지 않았다.

## 최소 공용 API 보완 후 로컬 확인

- `launch.rs`는 기존 두 메서드 추가 외의 차이가 없고 각 메서드는 `edb42bf`와 동일함을 읽기 대조했다.
- `session_redirected_630.rs`를 해당 파일만 rustfmt로 정리하고 `launch.rs`와 함께
  `rustfmt --edition 2021 --config skip_children=true --check`에 통과했다.
- 수정한 모듈 전용 CI 스크립트의 `bash -n`, 새 테스트에 포함된 shell payload의 `sh -n`,
  담당 파일 `git diff --check`와 새 파일 trailing whitespace 검사가 통과했다.
- 새로운 기능·빌드·Clippy 실행은 로컬에서 하지 않았다. 미해결 소스 API 의존성은 없으며
  남은 조건은 두 crate의 정확 SHA CI(공용 회귀 3개 + modem 15개 + EOF RED + 원본 비교),
  strict Clippy, 영향받는 ARM 검사 및 상위 PR/병합 후 검증이다.
