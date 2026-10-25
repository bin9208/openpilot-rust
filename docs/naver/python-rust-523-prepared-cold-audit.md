# #523 prepared cold audit 경량 패키지 인계

2026-10-26: `openpilot-prepared-cold-audit`는 기존
`rust/crates/dev-tools/tests/ci_disk_bootstrap/prepared_audit.rs`를 `#[path]`로 읽는다.
감사 구현·정책·계산·실패 전달을 복제하거나 수정하지 않는다. 기존 dev-tools의
`ci_disk_bootstrap` 45개 테스트와 원본 모듈 등록은 그대로 보존한다.
직접 의존성은 `serde_json`, `sha2`이고 dev 의존성은 `tempfile`뿐이다.

두 패키지의 `CARGO_MANIFEST_DIR`는 모두 `rust/crates/<package>` 깊이이므로
원래 `root()`와 동일한 `../../..`가 저장소 루트다. `#[path]`는 원본 파일을
모듈로 읽으므로 원본 안의 `include_bytes!` 네 곳은 계속 그 파일 기준으로
`tools/ci/fast_native_cold_560.sh`, `tools/ci/cold_tracer_launch_560.sh`,
`tools/ci/prepare-cold-strace-560`를 가리킨다. 마지막 launcher는 두 번 참조된다.
새 모듈도 `prepared_audit::` selector와 원본 11개 일반/1개 ignored 테스트를 유지한다.

부모 등록: `rust/Cargo.toml` workspace members에 `crates/prepared-cold-audit` 추가,
`Cargo.lock` 패키지 등록, workflow/Focus/pins 연결. 이 작업에서는 공유 파일을 수정하거나
stage하지 않는다. 부모의 native routing 별도 worker 배정과 기존 native 7분/6 GiB,
jobs 1, observer 120초/512 MiB 및 작업별 growth admission을 유지한다.
SDK·provider 전후 검증이나 실제 증거 환경·receipt 계약도 원본을 그대로 사용한다.

부모 Actions 실행 명령(로컬 실행 금지):

```bash
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-prepared-cold-audit --lib prepared_audit::
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-prepared-cold-audit --lib prepared_audit::prepared_audit_actual_cold_bootstrap_receipt -- --ignored --exact --nocapture
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-prepared-cold-audit --all-targets -- -D warnings
```

첫 명령은 일반 11개를 실행하고 actual 1개는 ignored로 남긴다. 두 번째는 기존
실제 source/SDK/trace/proof 및 job 환경이 준비된 동일 감사 경계에서만 실행한다.
사용자 전달 증거: job `121252313245`에서 실제 감사 compile 163초와 audit 48초가
성공했고, 전체 7분 안 routing 준비 중 취소되었다. 이 패키지는 compile 의존성 축소가
목적이며 시간 개선량·새 패키지 빌드 성공은 아직 측정하지 않았다.

로컬 확인은 TOML 파싱, 원본/경로/테스트 선언 수 확인, 새 `lib.rs`의 Rustfmt 구문·서식,
diff 검사에 한정한다. Cargo/build/test/Clippy 및 실제 감사 실행은 0회다.

원본 감사가 Unix fs metadata API를 사용하므로 새 wrapper의 root/module도 `cfg(all(test, unix))`로 묶었다. 기존 Linux/Darwin 감사 selector와 구현에는 영향이 없고 non-Unix workspace에 Unix import를 추가하지 않는다.
