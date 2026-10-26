# 전체 native runtime 빌드 계획 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
세대·캐시 [#600](https://github.com/bin9208/openpilot-rust/issues/600).

`rust/native-runtime-catalog.json`은 기존 dev 계열 전체 runtime의 실제
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` 빌드 목록89개/11그룹을 보존하고,
자체 빌드 도구8개와 기존 sealer/ELF audit2개를 별도 그룹으로 추가한다. 현재 목록은
99개/13그룹이며, 과거의 cross-build 결과를 현재 native 성공으로 사용하지 않는다.
dev-md의 별도 runtime 목록이나 compiler 전환을 가져오지 않았다.

## 실제 소스 확인과 계획 생성

`openpilot-runtime-plan`은 clean checkout의 정확 commit/tree를 읽고 실제 Cargo로
`metadata --locked --offline --no-deps`를 실행한다. catalog의 package가 workspace에
있는지, binary의 단일 소유 package가 선택되었는지, 선택한 feature와 binary의 필수
feature가 존재하는지 확인한다. package 내부의 default/local feature 별칭도 추적한다.
전체 의존성의 feature resolution·link·실제 실행은 뒤의 Cargo 빌드 검사를 유지한다.

다음은 **Actions 또는 준비된 장치 환경용 개발 명령**이다. 현재 로컬의 기능 검사
금지는 그대로다. `--check`는 빌드·봉인·활성화·다운로드를 수행하지 않는다.

```sh
cargo run --manifest-path rust/Cargo.toml --locked --offline \
  -p openpilot-runtime-build --bin openpilot-runtime-plan -- \
  --root "$GITHUB_WORKSPACE" --cargo "$(rustup which cargo)" --check
```

출처가 확인된 실제 native 입력을 준비한 뒤 `Bindings` schema1 JSON으로 연결한다.

| 필드 | 값과 검사 |
| --- | --- |
| `input_sets` | 이름별 `files`(절대 경로→SHA), `trees`(절대 root→receipt 경로), `environment` |
| `common` | 모든 compiler 호출이 쓰는 Rust toolchain·native SDK 등의 input set 이름 |
| `groups` | catalog와 정확히 같은 그룹 키00…12, 각 그룹이 추가로 쓰는 input set 이름 배열 |
| `group_growth_bytes` | 같은 그룹 키별 작업 예상 디스크 증가량; 고정25/35GiB floor 없음 |
| `expected_source_growth_bytes`, `remaining_object_growth_bytes` | 소스 보존·결과 저장의 작업별 증가량 |
| `runtime_inputs` | compiler와 무관한 model/data/실행 companion의 기존 `files`·`trees` |

각 tree receipt도 `files`에서 독립적으로 신뢰한 SHA로 고정해야 한다. 빈 input set,
누락/여분 그룹, 알 수 없는/중복/미사용 set, 상충하는 hash·환경, 변조한 파일/tree는
실패한다. 모든 recipe의 `OPENPILOT_NATIVE_INCREMENTAL=1`은 유지한다. provider의
실제 compiler 소비 그룹을 조사해 배정해야 하며, 편의상 전체 model/provider를 모든
recipe에 붙이면 무관한 target cache도 바뀐다. 실행 시에만 쓰는 자산은 `runtime_inputs`에
연결해 compiler identity에서 분리한다.

```sh
openpilot-runtime-plan --root "$SOURCE_ROOT" --cargo "$NATIVE_CARGO" \
  --bindings "$VERIFIED_NATIVE_INPUTS" --output "$NEW_PLAN_DIRECTORY"
```

`NEW_PLAN_DIRECTORY`의 부모는 이미 있어야 한다. 도구는 새 디렉터리에만
`build-plan.json`과 `catalog-audit.json`을 fsync 후 원자적으로 발행한다. 기존 출력이나
symlink는 대체하지 않는다. audit에는 source, catalog, 실제 metadata/Cargo, bindings,
최종 plan SHA가 있으며 `compiled=false`, `activated=false`를 명시한다. source/catalog/
Cargo/bindings가 검증 중 변경되면 발행하지 않는다. 생성한 `build-plan.json`은 기존
native generation Config의 `build_plan`에 연결한다. layout/resources/전체 SDK/provider
준비 및 세대 build/seal/activation은 여전히 별도 필수 검사다.

## 검증과 남은 범위

- Actions의 원래 foundation x64/ARM QEMU 검사에 실제 catalog CLI와 전체 workspace
  metadata 대조를 추가한다. 원래 빌드·회귀·Clippy·시작/업데이트 명령은 유지한다.
- 같은 crate의 실제 Cargo 회귀는 작은 package2개를 사용한다. provider 변경은 실제
  출력42→43과 해당 cache 변경을, 무관한 그룹은 fresh/같은 hash를 확인한다.
  runtime model 변경만으로는 두 compiler recipe/cache가 바뀌지 않아야 한다.
- CLI의 전체 plan 발행은 실제 workspace metadata를 사용하고 plan/bindings SHA,
  기존 출력·symlink 보존, 변조 입력의 발행 거부를 검증한다. 이 시험의 작은 입력 표식은
  SDK가 아니며 최종 generation의 전체 toolchain 검사를 통과할 수 없다.
- genuine AGNOS probe는 고정 원본 이미지 안에서 실제 planner도 native 빌드하고
  99개 목록을 대조한다. 이 결과 역시 목록/도구 실행이며 99개 runtime의 빌드 성공이 아니다.

기존 보관 이미지에 대한 2026-10-27 읽기 감사에서 Ubuntu24.04.5 user space와 GCC13,
Clang18, CMake, Python3.12, 개발 헤더가 관찰되었다. 새 Actions에서 원본 image SHA를
확인하고 실제 실행한 결과와 구분한다. 아직 native SDK의 전체 compiler/header/library
closure, Python 모델 준비 환경, provider별 출처·준비·영향 그룹과 full runtime
build/seal/start/update/recovery, comma3X 사용자 수용은 미완료다. 새 계획 생성기로
이 완료 조건을 대신하지 않는다.
