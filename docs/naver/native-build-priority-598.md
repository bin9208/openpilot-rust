# 성능 개선·콤마 자체 빌드 우선 실행 — #598

사용자 결정: 2026-10-26 KST.
[대단원 #598](https://github.com/bin9208/openpilot-rust/issues/598)의 모든 완료 조건을
충족한 뒤 종료한다. 이 이슈의 종료를 우선 작업의 완료로 사용한다.

## 범위

성능 개선과 콤마 자체 빌드, 작은 소스 변경마다 전체 runtime을 재패키징하고
약1.1GB를 다시 내려받는 구조의 제거가 우선이다. 모델 준비는 Python을 유지한다.
기존 C/C++ 라이브러리를 재사용하며 비용이 크고 이득이 작은 언어 전환은 뒤로 미룬다.
전체 전환은 이 우선 작업 완료 후 점진적으로 진행한다.

기존 Rust 전환은 버리지 않는다. 검증된 구현을 각 트랙의 정상 PR 경로로 반영한다.
미검증 코드의 무조건 병합과 dev-md→dev/main 통합은 하지 않는다. 전체 전환
#523/#296는 남겨 두고 우선순위 변경 댓글을 추가했다. 전체 관리 #522에 이 대단원을
native 하위 이슈로 연결하고 최신 실행 범위를 기록했다.

| 실행 단위 | 이슈 | 완료할 결과 |
| --- | --- | --- |
| AGNOS 자체 빌드 | [#599](https://github.com/bin9208/openpilot-rust/issues/599) | 실제 native toolchain/provider·Python 모델 준비와 전체 runtime 소스 빌드 |
| 다운로드·빌드 재사용 | [#600](https://github.com/bin9208/openpilot-rust/issues/600) | 대형 자산 캐시·영향 대상 재빌드·검증 후 세대 전환과 복구 |
| 기존 전환·성능·전달 | [#601](https://github.com/bin9208/openpilot-rust/issues/601) | 기존 구현 반영·집중 성능 회귀·최종 전달 및 사용자 수용 |

## 첫 조사 결과

- stable `origin/dev`는 `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`다. 이 기준에서
  `codex/feat-598-device-native-build`를 만들었다. 기존 dev-md의 미커밋 작업과
  다른 worktree는 변경하지 않았다.
- `ensure_runtime.sh`는 Git HEAD별 `runtime-<SHA>` release를 찾는다. launcher와
  installer 모두 package source_commit과 checkout identity를 검증한다.
  이 검사를 삭제하는 것은 새 source의 올바른 runtime을 만드는 대체 수단이 아니다.
- PR343의 조회 head는 `126fdbea36598566d1040be0a35a365c909be4e4`, dev 대비776개
  커밋이다. 최신 #523 실행 branch는 별도다. source/검증 범위를 감사하지 않고
  모두 merge하지 않는다. 이 커밋 수를 진척률이나 일정으로 쓰지 않는다.
- 기존 성능 PR494는 dev-md 전용이며 복사·할당 후보가 있다. 정확 소스 필수 검증과
  전달·실기 수용은 남는다. 이 변경을 dev에 가져와 원본 반영 트랙을 섞지 않는다.

## 첫 구현: 오프라인 Cargo 빌드와 영속 target cache

`openpilot-runtime-build`는 이미 준비된 native Rust1.94.0과 Cargo 의존성으로
지정 runtime target을 빌드하는 초기 도구다. Cargo의 source dependency 추적을
재사용하고 commit SHA를 target cache 경로에 넣지 않는다. 문서 변경이나 새 커밋이라는
이유만으로 compiled crate를 버리지 않으며 도구 체인 binary·target·feature·명시 외부
입력이 다르면 캐시를 분리한다. 자원 증가량 추정 변경은 compilation identity가 아니다.

- `--locked --offline`, `CARGO_BUILD_JOBS=1`로 직접 Cargo를 호출한다.
  compiler incremental과 별개로 Cargo의 crate freshness를 재사용한다.
- 명시 외부 입력을 전후 hash로 확인하고 작업별 예상 디스크 증가량으로 admission한다.
  원래 source/provider binding을 대체하는 완성된 seal/수용 증거는 아니다.
- cache owner lock으로 같은 cache의 동시 빌드를 거부한다. 소유가 다른 runtime이나
  source tree를 정리하지 않고 build/report만 만들며 설치·활성화는 수행하지 않는다.
- 마지막 성공 보고서는 Cargo executable/fresh 결과다. artifact hash/전체 source를
  봉인한 설치 manifest가 아니며 이후 빌드로 target 내용이 달라질 수 있다.
- 모델 준비, provider/toolchain 다운로드, offline Cargo cache 준비, 전체 runtime recipe,
  source-bound generation 봉인·시작·업데이트 연결은 아직 미구현이다. 자동 시작 경로는
  바꾸지 않았다. 이 도구만으로1.1GB 다운로드 제거나 자체 빌드 완료를 주장하지 않는다.

첫 CI는 독립된 실제 Cargo fixture를 사용해 문서 수정의 binary freshness, 소스 수정의
실제 재빌드/실행 출력, provider 변조·갱신, 동시 cache 소유 및 컴파일 실패를 검증한다.
fixture의 host 결과를 AGNOS 자체 빌드나 장치 성능으로 표현하지 않는다.

## 검증 및 다음 작업

- 로컬: scoped Rustfmt와 diff 확인. 기능·Cargo·빌드·Clippy는 로컬 실행하지 않는다.
- 첫 기능 검증은 `.github/workflows/runtime-source-build.yml`의 실제 Actions 결과로
  기록한다. 실행 전 상태는 **미검증**이다.
- 다음은 AGNOS native toolchain/표준 라이브러리·linker·provider receipt/cache 준비,
  각 기존 runtime의 실제 Cargo build/feature/asset roster 확정, 전체 recipe와
  기존 source-matched manifest/activation 경로의 연결이다.
- clean/no-op/변경 후 빌드의 시간·메모리·공간·전송량과 사용자 정상 부팅·녹화·업로드
  수용을 구분한다. 이 문서나 이슈 생성만으로 #598/#599/#600/#601을 종료하지 않는다.

4~8주는 초기 계획 예산이며 장치 native 빌드 실측 전 확정 일정이 아니다.
