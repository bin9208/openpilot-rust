# #523 F02 compiler: pipeline stage 경계 방출

작성: 2026-10-26. 추적: [#523](https://github.com/bin9208/openpilot-rust/issues/523).
작업 루트: `/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`.
읽기 기준 HEAD: `ad9f19222e6eff0c86b82e7ad7ac5ca22a410057`.
상태: 한 기능의 Rust 구현·검증 소스 전달, 부모 등록 및 exact-SHA Actions 수용 대기.

## 현재 코드에서 선택한 범위

`offline_schedule.py`, `compiler_launch.rs` 및 해당 테스트,
`compiler_onnx/{mod,schedule,memory,render}.rs`, `compiler_backend_cpu.rs`,
기존 `entrypoint.rs`를 읽었다. launch 추론과 정적 typed-plan 스케줄링 및
메모리 수명 계획은 이미 구현되어 있다. 이를 다시 포팅하지 않았다.

선택한 미전환 기능은 `export_pipeline`의 잔존 source-call 집합 기반 stage 끝점
집계와 `emit_schedule`의 `emitted_by_source`를 통한 최종 entrypoint 범위 계산이다.
기존 CPU backend의 `entrypoints: Vec::new()`와 구분되는 native API를 추가했다.
상수 계산 자체, UOp 생성·fusion, 전체 QCOM 그래프 방출 및 tinygrad 전체 전환은
이번 묶음의 구현·수용 범위가 아니다.

## 실제 변경

- `rust/crates/model-runtime/src/compiler_schedule.rs`: `PipelineStage`와
  `pipeline_entrypoints(stages, retained_calls, emitted_by_source)` 구현.
  source identity 집합 소속 여부를 원본처럼 각 출현마다 세며, 제거된 상수 호출과
  빈 stage를 반영하고 실제 방출 prefix로 범위를 변환한다.
  copy와 중복 kernel의 호출도 prefix에 포함해야 한다. kernel 종류 개수로
  호출 개수를 대체하면 안 된다.
  기존 `entrypoint::validate`를 재사용하여 v1/v2, 이름 고유성·빈 문자열 거절,
  연속 범위·전체 호출 포함·65,536개 제한을 검증한다. 원본은 공백 문자열 이름을
  허용하므로 여기서도 빈 문자열만 거절하고 공백을 임의로 정규화하지 않는다.
  prefix의 길이·0 시작·비감소와 stage 인덱스 초과는 native 입력 경계에서 거절한다.
- `rust/crates/model-runtime/tests/compiler_schedule.rs`: 일반 Rust 경계 테스트 7개와
  GHA 전용 ignored 원본 비교 테스트 1개. 73개 비교 입력에는 4개 identity의
  모든 제거 조합, stage 간 반복 identity, 빈 stage, 0개/복수 방출 prefix,
  v1/v2, 잘못된 이름·중복 이름·호출 누락·stage 초과, 한글 이름,
  65,536/65,537개 stage가 포함된다.
- 이 기록 문서.

원본 비교는 GHA에서 실제 `schema.py`의 `Entrypoint`·`entrypoint_version`과
`offline_schedule.py`의 두 경계 계산 블록을 AST로 추출해 변경 없이 실행한다.
원본 파일의 SHA256을 출력하고 native 결과의 전체 범위·version 또는 거절 여부와
비교한다. 입력은 합성 source identity와 방출 prefix다. 모델·GPU·컴파일러·상수
계산의 원본 실행이나 전체 pipeline 동등성 증거로 해석하면 안 된다.
비정상 prefix에 대한 강화된 native 계약은 원본 비교 대상과 분리했다.

## 부모 등록 및 GHA 실행

공유 파일은 부모 소유이므로 이 작업자는 `lib.rs`, Cargo, CLI, workflow,
Focus/공유 목록을 수정하지 않았다. 부모에게 다음 등록을 요청한다.

1. `rust/crates/model-runtime/src/lib.rs`에 `pub mod compiler_schedule;` 등록.
   새 의존성·Cargo feature·CLI는 필요 없다. 기존 `entrypoint` 모듈을 그대로 쓴다.
2. compiler가 보유한 원본 stage별 source-call identity 목록, 상수 준비 뒤 잔존
   source-call 목록, 실제 방출 prefix를 이 API에 전달하고 반환 entrypoints와
   version을 manifest에 연결한다. 호출마다 동일 identity를 유지하고 kernel
   interning으로 호출을 삭제하지 않는다. 현재 source가 호출당 1회 방출하더라도
   추정 호출 수 대신 실제 emission accounting을 전달한다.
3. 부모가 관리하는 GitHub-hosted Actions job에서 아래 명령을 실행한다.
   첫 명령은 기존 launch 회귀도 함께 확인하며, 두 번째는 원본 비교를 명시 실행한다.
   테스트 파일은 경로 모듈 방식이라 공유 등록 전에도 집중 target을 실행할 수 있다.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-model-runtime --locked --test compiler_launch --test compiler_schedule
cargo test --manifest-path rust/Cargo.toml -p openpilot-model-runtime --locked --test compiler_schedule original_python_pipeline_boundaries_match_native -- --ignored --exact --nocapture
```

원본 비교는 `GITHUB_ACTIONS=true`를 요구하며 Python 표준 라이브러리만 사용한다.
통상 GHA의 `python3`와 Rust 패키지 의존성 외 모델·SDK 다운로드는 추가하지 않는다.
부모의 기존 runner/자원 admission/소스 봉인·필수 gate 정책은 유지한다.
실제 수용 SHA와 Actions URL·결과는 부모가 실행 후 이 문서 및 #523에 기록한다.

## 로컬 확인과 남은 항목

로컬에서 새 Rust 두 파일의 `rustfmt --edition 2021`이 성공했고, 내장 GHA용
Python 문자열은 `ast.parse`로 구문만 확인했다. 원본 코드는 실행하지 않았다.
새 파일 세 개의 `git diff --no-index --check /dev/null <path>`는 whitespace
오류 출력이 없었다(새 파일 차이를 나타내는 종료코드 1). 변경 범위·diff를 검토했다.
Cargo/build/test/Clippy, Python 원본 경계 실행, 모델 실행은 수행하지
않았다. 일반 경계 및 73개 원본 비교의 통과 여부는 아직 확인되지 않았다.
commit/push/CI 실행은 모두 0회다.

대기: 부모의 공유 모듈·compiler 호출부 등록, exact-SHA GHA 두 명령과 기존 필수
gate 수용, 결과 링크 기록. 원본 삭제는 실제 GHA 수용 뒤 부모가 판단한다.
기존 `compiler_launch.rs`와 테스트 및 다른 작업자 소스는 수정하지 않았다.


## 2026-10-26 stage73의 실제 native 수용

source `a62109fa706012b91b24206e7d8c875653746aec`의
[run40241338985/job121313583472](https://github.com/bin9208/openpilot-rust/actions/runs/40241338985/job/121313583472)은
launch9·stage8/0ignored, preserve_order union1, 원본launch4411 corpus1과 strict Clippy를 모두
성공했다. 새 Python subprocess/현재 원본 AST 조회가 없는 고정 reader다. 원본73 raw 입력·
기대값·SHA와 과거13 witness를 유지했으며 native 결과로 expected를 만들지 않았다.
artifact12616208518은1,583,513B/SHA256
`ece5ed6e6be36855b4180fe3400b97bbf9aed2d9bb55eb0a72a5b42c0018acc4`,
40 safe unique regular member/전체 CRC, expanded6,929,918B다. command4개 exit0,
source/index/index-stage와 whole SDK file/program/roster 전후 동일, SDK install0,
업로드 뒤 source/owned temp cleanup까지 실제 성공했다.

추가 실행 원장은 **162개 =32교체/126대기/1기존native/2비실행/1제거**다.
기준3580은 **299교체/3부분/3278대기**, production offline emitter와 전체 F02 수용은 그대로
미완료다. 이 좁은 비교 실행경계의 제거만 인정하며 source-file baseline credit은0이다.
