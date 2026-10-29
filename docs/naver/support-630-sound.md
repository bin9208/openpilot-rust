# #630 soundd 클립 경계 수정 분리 (#392)

작성일: 2026-10-29 KST. 상태: 패치 작성·로컬 문법/서식/diff 확인 완료,
정확 소스 Actions 및 dev 통합 대기. 실기 청취·정상 시작 수용은 사용자 담당이다.

## 범위와 출처

- 실행 묶음: [#630](https://github.com/bin9208/openpilot-rust/issues/630).
  결함·수용 기준: [#392](https://github.com/bin9208/openpilot-rust/issues/392),
  [기존 최신 증거 댓글](https://github.com/bin9208/openpilot-rust/issues/392#issuecomment-6256402670).
- 기준 `origin/dev`: `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`.
  작업 브랜치: `codex/feat-630-independent-support`.
- 기존 수정: `134d44eb8024472483344448d0b1aaf309ab7921`.
  `policy.rs`와 기존 7개 playback 회귀는 후보
  `126fdbea36598566d1040be0a35a365c909be4e4`에서도 동일하다.
  두 파일만 해당 후보에서 가져왔다. 전체 merge/cherry-pick은 하지 않았다.
- 후보 `126fdbea…`에는 `rust/tools/check_soundd.py`가 없다.
  현재 dev의 원본 비교 도구에 필요한 독립 샘플 oracle 변경은 위 `134d44eb…`에서
  분리해 재사용했다. 원본 Python 파일이나 공용 binding/helper는 수정하지 않는다.
- #393의 `runtime.rs` 잠금 해제, `tests/logging_progress.rs`, manifest,
  dev-dependencies 및 `Cargo.lock` 변경은 포함하지 않는다. #598 소유 공용 경로,
  workspace, CI, installer, bootstrap, 모델·안전 경로는 수정하지 않는다.

## 변경과 유지 계약

`rust/crates/soundd/src/policy.rs`는 각 연속 복사 구간마다 offset/loop를 다시
계산한다. 유한 반복 한도에 도달하면 남은 callback은 처음 채운 0으로 남긴다.
`[1,2,3,4]`, frame=2, frames=5의 이전 결과 `[3,4,3,4,3]`는 결함이다.
수정 계약은 finite 1회 `[3,4,0,0,0]`, infinite `[3,4,1,2,3]`이다.
반복 총길이를 곱해서 production counter를 판정하지 않으며 기존 `checked_add`와
`sample counter overflow` 오류를 유지한다.

`rust/crates/soundd/tests/playback.rs`는 기존 유한/무한 경계, 서로 다른 callback
크기의 연속성, 0회, 빈 callback, checked overflow, 음수 볼륨의 IEEE-754 음수 0
검사를 보존한다. 실제 배포 `engage`/`prompt`/`promptRepeat`/두 warning 자산도
일반·tizi 각각 마지막 2샘플에서 시작해 4096샘플 callback 두 번으로 검사한다.
샘플별 선형 인덱스 기대값은 production의 구간 복사를 재현하지 않으며 float32
bits와 frame을 비교한다. 자산 로더·engage gain 0.37·출력 volume 0.73을 사용한다.
파일 헤더 읽기로 `sounds_eng`의 실제 길이 24000/31538(engage/tizi), 72299(prompt),
76860/34131(warning soft/immediate), 48kHz mono를 확인했다.

`rust/tools/check_soundd.py`는 미수정 원본의 `*-source.json`, 독립 샘플 계약의
`*-intended.json`, 후보의 `*-native.json`을 따로 보존한다. samples/frame에만 의도한
변화를 허용하며 나머지 원본 상태는 그대로 비교한다. 후보 샘플은 독립 계약과
float32 bits로 비교하고 volume/filter의 기존 1e-12 허용오차를 유지한다.
원본 일치 샘플 및 의도한 수정 샘플/frame이 모두 실제로 포함되어야 통과한다.
기존 6개 시나리오(합성 mono/stereo/resampling·일반/tizi·실제 자산), 141단계씩은
유지한다. 과거 기록의 846단계/6348샘플, 수정 1113샘플/27 frame 행은 과거 증거이며
현재 후보의 성공으로 대신하지 않는다.

`rust/tools/check_soundd_daemon.py`는 운영 바이너리/소유 PortAudio C ABI의 기존
7개 시나리오를 유지한다. live에서 engage/MAIN prompt의 유한 클립 이후 무음과
반복 prompt/timeout warning의 4096샘플 전체를 fixture PCM에서 독립 계산해
bit 단위로 검사한다. 기존 첫 샘플·20Hz·5초 timeout·retry/stop 검사를 유지하며
timeout을 늘리지 않는다. 실제 오디오 하드웨어는 열지 않는다.

alert 선택, 볼륨 필터·언어·기기 정책, 48kHz, 4096 callback, 상태 timeout,
PortAudio 열기·재시도 정책 및 checked counter 계약은 변경하지 않는다.

## 로컬 확인

공유 worktree `/home/fma1/.codex/worktrees/support-services-630/openpilot-rust`에서
담당 파일에만 다음 검사를 수행했다.

- `rustfmt --edition 2021`로 `policy.rs`, `tests/playback.rs` 처리 성공.
- 위 Rust 파일의 `rustfmt --edition 2021 --check` 성공.
- 두 Python 비교 도구에 `ast.parse` 성공. import·실행·bytecode 생성 없음.
- 담당 5개 경로 `git diff --check` 성공, 내용 diff 검토 완료.
- 로컬 cargo test/build/clippy, 기능 회귀, 원본 비교, Actions 실행은 하지 않았다.
  공유 변경을 stage/commit/push하거나 다른 담당 파일을 수정하지 않았다.

## Actions에서 필요한 정확 명령

아래는 **상위 담당자가 등록된 bin-actions의 정확 후보 SHA에서 실행할 명령**이다.
이 담당 작업에서는 workflow를 수정하거나 Actions를 실행·취소하지 않았다.
`SOURCE_SHA`와 출력의 `git rev-parse HEAD`가 같아야 한다. 기존 공용 source binding과
IPC 준비는 `.github/workflows/rust.yml`의 `sensor-audio` 경로를 사용한다.
운영 계약·원본 비교를 빼고 cargo 검사만 성공시키지 않는다.

```sh
# 작업 디렉터리: bin-actions에서 정확 SOURCE_SHA를 checkout한 저장소 루트
# 기존 sysroot/native dependency/binding 준비를 재사용한다.
git rev-parse HEAD
rustfmt --edition 2021 --check rust/crates/soundd/src/policy.rs rust/crates/soundd/tests/playback.rs
cargo +1.94.0 test --manifest-path rust/Cargo.toml -p openpilot-soundd -p openpilot-portaudio --locked
cargo +1.94.0 clippy --manifest-path rust/Cargo.toml -p openpilot-soundd -p openpilot-portaudio --all-targets --locked -- -D warnings
cargo +1.94.0 build --manifest-path rust/Cargo.toml -p openpilot-soundd -p openpilot-portaudio --bins --examples --locked
PYTHONPATH="$RUNNER_TEMP/sensor-msgq-python:$PWD:$PWD/rust/tools" python rust/tools/check_soundd.py --binary "$PWD/rust/target/debug/examples/sound_trace" --binding "$RUNNER_TEMP"/sensor-params-binding/params_pyx*.so --output "$RUNNER_TEMP/support-630-sound/source"
PYTHONPATH="$RUNNER_TEMP/sensor-msgq-python:$PWD:$PWD/rust/tools" python rust/tools/check_soundd_daemon.py --binary "$PWD/rust/target/debug/openpilot-soundd" --output "$RUNNER_TEMP/support-630-sound/daemon"
```

원본 binding은 기존 `build_msgq_python.py`/`build_params_python.py`의 출력과
인접 provenance를 그대로 사용한다. 소스/모듈 hash 검증을 생략하지 않는다.
별도 의존성 추가는 필요 없다. 회귀 재현(RED)은 독립 Actions 임시 소스 사본에서
새 `tests/playback.rs`를 유지하고 `policy.rs`만 기준 `e445c9d…` 버전으로 놓은 뒤
다음 명령이 실패함을 보존한다. 공유 worktree에서 교체하지 않는다.

```sh
cargo +1.94.0 test --manifest-path rust/Cargo.toml -p openpilot-soundd --test playback --locked
```

기존 경계 회귀 4개와 새 실제 자산 회귀가 실패하고 나머지 제어 회귀는 통과하는지
확인한다. GREEN은 정확 후보 정책의 위 package 전체 검사이며 RED 실패를 일반
빌드 실패로 대신하지 않는다.

영향받는 ARM 검사는 기존 AGNOS sysroot를 쓰는 aarch64 QEMU/PRoot 실행 환경에서
package 회귀와 위 source/runtime 비교를 보존한다. 최소 ARM 빌드 명령은 다음과
같다. generic cross-build만으로 AGNOS 실행·원본 비교 통과를 주장하지 않는다.

```sh
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo +1.94.0 build --manifest-path rust/Cargo.toml -p openpilot-soundd -p openpilot-portaudio --bins --examples --target aarch64-unknown-linux-gnu --locked
```

## 남은 증거와 의존성

새 패키지·공용 소스 변경 의존성은 없다. 기존 Params/msgq binding, native build
환경, ARM sysroot/실행 harness 및 공유 Actions 배치는 상위/#598 소유 그대로다.
집중 검사 묶음의 cache/출력 예상 증가량에 맞춰 상위에서 admission을 수행한다.
정확 candidate SHA/PR/Actions URL은 아직 없고 상위가 커밋·실행 후 이 기록에 남긴다.
필수 PR 검사와 병합 후 별도 검사, dev 통합 및 사용자 실기 수용은 미완료다.
#393은 별도 미포함 항목이며 본 패치로 진단 backpressure 교착이 해결됐다고 하지 않는다.
