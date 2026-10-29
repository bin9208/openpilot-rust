# #630 / #363 tombstoned 소유 자식 종료 순서 분리

작성일: 2026-10-29 KST. 상태: 최소 구현·회귀 분리 및 로컬 정적 확인 완료, 최신 후보의 Actions·통합·실기 검증은 미실행.

## 범위와 출처

- 실행 묶음: https://github.com/bin9208/openpilot-rust/issues/630
- 원래 결함·수용 조건: https://github.com/bin9208/openpilot-rust/issues/363
- 이전 수정·RED/GREEN 기록: https://github.com/bin9208/openpilot-rust/issues/363#issuecomment-6249908007
- 기준 `origin/dev` 및 분리 시작 HEAD: `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`.
- 읽기 원본: `origin/codex/feat-296-repository-python-retirement`, `126fdbea36598566d1040be0a35a365c909be4e4`.
- 원래 수정 SHA: `518f16573e6a05a02b61be3e5af4bbfaaf7e9681`.
- 작업 디렉터리: `/home/fma1/.codex/worktrees/support-services-630/openpilot-rust`.
- 분리한 `rust/crates/tombstoned/src/apport.rs`, `rust/crates/tombstoned/tests/apport_owner.rs`는 위 원래 수정 및 읽기 원본의 해당 파일과 바이트 단위로 동일하다. 전체 merge/cherry-pick은 하지 않았다.

## 변경과 보존한 계약

기존 `Child::try_wait()`는 shell leader를 reap한 뒤 UTF-8 해석 오류 또는 descendant-held stdout의 취소/timeout에서 숫자 process-group ID에 signal할 수 있었다. PID 재사용이나 다른 프로세스 피해를 재현했다는 주장은 하지 않는다.

`waitid(EXITED | NOHANG | NOWAIT)`로 종료만 관찰한다. 성공 경로는 EOF와 종료 관찰 후 UTF-8 해석·개행 정규화를 마친 다음 `Child::wait()`한다. 오류·취소·timeout 경로는 leader가 waitable인 동안 마지막 `kill_process_group(SIGKILL)`을 보내고 `Child::wait()`로 reap한다. 기존 `terminate()`의 오류 전달 정책은 그대로다.

새 의존성 없이 기존 `rustix`의 `process` feature와 기존 `tempfile`을 사용한다. 기본 30초 제한, 취소 때 최대 50ms poll 간격, EOF 후 10ms 대기, argv filename 처리, stdin/stderr 상속, 성공/실패/timeout 문자열 및 UTF-8 오류는 변경하지 않았다. 공용 process-supervision/helper, Cargo 설정·lock, CI, bootstrap/provider/installer/모델·안전 경로는 수정하지 않았다.

## 재사용한 회귀와 원본 비교

`tests/apport_owner.rs`의 실제 소유 자식 회귀 2개와 reexec worker를 그대로 재사용한다. `/usr/bin/strace -ff --kill-on-exit`가 필수이며, 없을 때 skip하지 않는다.

- invalid UTF-8: 오류 해석 이후 group signal 전 reap이 없는지 검사한다.
- leader-exited/descendant-held stdout 취소: 실제 종료 관찰 syscall을 확인한 뒤 stop을 설정한다. 예전 `wait4`도 barrier에서 인식하므로 취소 시점을 앞당겨 RED를 우회하지 않는다.
- 두 경우 모두 직접 fork한 leader임을 확인하고, `WNOWAIT` 관찰 → 마지막 group signal → 정확히 한 번 reap 순서와 crash 원본 보존을 검사한다. held descendant는 종료 또는 zombie 상태만 허용한다.
- 4초 취소 barrier, 10초 외부 worker 제한과 owned tracee 정리 정책을 그대로 유지했다.

기존 `rust/tools/check_tombstoned_reference.py`의 242개 원본 비교를 변경 없이 사용한다. 정상/실패 exit, invalid/invalid-exit, 100ms timeout·연속 stdout, 실제 기본 30000ms timeout, 파일·로그·보고 정책을 유지한다. `check_tombstoned_safety.py`의 #77 literal filename 차이와 `check_tombstoned_shutdown.py`의 실제 SIGINT/SIGTERM·5초 종료·자손 정리·미처리 crash 보존 조건도 그대로 사용한다. 현재 기준에는 원본 `openpilot/system/tombstoned.py` 및 관련 oracle 모듈이 존재하므로 checker 경로 수정이나 Python 도구 전환은 필요하지 않았다. 이후 원본 경로가 달라지면 담당 모듈 oracle 경로만 검토하고 공용 loader는 이 패치에서 변경하지 않는다.

이전 SHA의 기록상 package 10 test entries/strict Clippy/242 비교와 owned-child RED/GREEN은 과거 근거다. 이번 후보에서 실행한 결과로 대체하지 않는다.

## 로컬 확인

허용 범위인 읽기·서식·문법·diff만 수행했다. Rust 2021 담당 파일 `rustfmt --check`, 담당 Python checker 5개의 `ast.parse`, 원래 수정 및 읽기 원본과의 2개 파일 바이트 대조, owned-child Bash fixture의 `bash -n`, 담당 diff 공백 검사를 확인했다. 새 회귀 파일은 untracked이므로 `git diff --no-index --check /dev/null <파일>`로 별도 확인한다. local cargo test/build/clippy, oracle 실행, Actions 실행·취소, issue 상태 변경, commit/push는 하지 않았다.

첫 도구 실행에서는 `python` 실행 파일 부재로 복사가 실행되지 않았으며, 저장소 edition을 확인하기 전 2024 서식 확인도 완료하지 못했다. `python3`와 실제 workspace edition 2021로 바로잡아 담당 파일을 복사·확인했다. 소스 변경이나 검사 범위 완화는 없다.

## 필요한 정확 소스 CI 명령 — 상위 담당 실행

아래는 **GitHub Actions의 등록 `bin-actions` 환경에서만** 실행할 명령이다. 작업 디렉터리는 이 checkout이며, CI의 실제 checkout 경로가 다르면 아래 `cd`를 그 경로로 치환한다. Rust 1.94.0, Bash, `/usr/bin/strace`의 `--kill-on-exit`, capnproto/libcapnp, libzmq, Python 3.12 및 기존 telemetry pinned dependency 환경이 선행되어야 한다. `PARAMS_BINDING`은 기존 builder로 만든 원본 Params `.so`, `MSGQ_PYTHON`은 원본 msgq binding 디렉터리, `EVIDENCE`는 해당 run의 독립 증거 디렉터리다.

```sh
cd /home/fma1/.codex/worktrees/support-services-630/openpilot-rust
export RUSTUP_TOOLCHAIN=1.94.0
cargo test --manifest-path rust/Cargo.toml -p openpilot-tombstoned --all-targets --locked
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-tombstoned --all-targets --locked -- -D warnings
cargo build --manifest-path rust/Cargo.toml -p openpilot-tombstoned -p openpilot-logmessaged --bins --examples --locked
export PYTHONPATH="$MSGQ_PYTHON:$PWD:$PWD/rust/tools${PYTHONPATH:+:$PYTHONPATH}"
python3 rust/tools/check_tombstoned_reference.py rust/target/debug/examples/tombstone_trace "$PARAMS_BINDING" "$EVIDENCE/reference"
python3 rust/tools/check_tombstoned_safety.py rust/target/debug/examples/tombstone_trace "$PARAMS_BINDING" "$EVIDENCE/safety"
python3 rust/tools/check_crash_sdk_transport.py rust/target/debug/examples/tombstone_trace "$PARAMS_BINDING" "$EVIDENCE/sdk"
python3 rust/tools/check_tombstoned_runtime.py rust/target/debug/openpilot-tombstoned rust/target/debug/openpilot-logmessaged "$PARAMS_BINDING" "$EVIDENCE/runtime"
python3 rust/tools/check_tombstoned_shutdown.py rust/target/debug/openpilot-tombstoned "$PARAMS_BINDING" "$EVIDENCE/shutdown"
```

ARM 집중 빌드는 기존 aarch64 GNU cross toolchain·libzmq provider 준비 후 다음 명령을 사용한다. 이는 generic Linux ARM 빌드이며 AGNOS 실행·실기 수용의 증거가 아니다. 전체 필수 `rust aarch64 build` 및 PR/병합 후 gate를 대체하지 않는다.

```sh
cd /home/fma1/.codex/worktrees/support-services-630/openpilot-rust
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo +1.94.0 build --manifest-path rust/Cargo.toml -p openpilot-tombstoned --bins --examples --release --locked --target aarch64-unknown-linux-gnu
```

최신 source SHA·실행 명령·binary/source hash·원본 비교 manifest·owned-child 로그·Actions URL을 한 run의 증거로 남긴다. 242 비교 전부 일치, 원래 실패 반환·timeout 유지, syscall 회귀 및 daemon shutdown 통과를 확인해야 한다. ARM QEMU의 기존 missing-bash emulation 차이는 `EMULATION_LIMITS`로 남기며 통과 수에 넣지 않는다.

## 미해결 의존성과 인계

- 소스 수준 공용 변경 의존성은 발견하지 못했다. 담당 example도 추가 수정이 필요하지 않다.
- 현재 `.github/workflows/rust.yml`의 `workspace`는 필수 strace 및 workspace test/Clippy를, `telemetry-runtime`은 위 원본/SDK/runtime/shutdown 비교를 이미 연결한다. `arm64`는 generic ARM 빌드를 제공한다. 다만 이 기준 파일의 runner는 `ubuntu-24.04`이므로 최신 정책의 등록 `bin-actions` 실행 조율은 상위/#598의 공용 CI 담당이 처리해야 한다. 이 패치에서 workflow를 변경하거나 dispatch하지 않았다.
- 현재 후보의 기능·원본 비교·ARM·정확 SHA 필수 검사, dev 통합 및 병합 후 검사는 남아 있다. 이전 [Actions run](https://github.com/bin9208/openpilot-rust/actions/runs/39127259281)은 원래 댓글에서 pending으로 기록된 과거 링크이며 이번 성공 근거가 아니다.
- 기기 설치·부팅·녹화·업로드·주행 수용은 사용자 담당이며 확인하지 않았다. #363 및 #630 상태는 변경하지 않았다.
