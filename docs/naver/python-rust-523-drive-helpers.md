# #523 F01 drive_helpers 제한 구현

작성: 2026-10-26. 상위 이슈: [#523](https://github.com/bin9208/openpilot-rust/issues/523).
검토 기준 HEAD: `ad9f19222e6eff0c86b82e7ad7ac5ca22a410057`.
상태: 소스 비교·패치·로컬 서식/diff 확인, 기능 실행은 부모의 GitHub Actions 통합 대기.

## 원본 전체와 기존 native 비교

`openpilot/selfdrive/controls/lib/drive_helpers.py` 전체,
`rust/crates/control-policy/src/drive.rs`, 기존 대응 테스트 `tests/math.rs` 및
`tests/native.rs`를 읽고 다른 crate에 내장된 동작까지 확인했다.

| 원본 함수 | 기존 native 대응과 남은 경계 |
| --- | --- |
| `is_volkswagen_meb` | `controlsd/src/config.rs::Config::meb`와 `plannerd/src/config.rs`에 brand/플래그 판정 존재. 이번 변경에서 차량 판정을 건드리지 않는다. |
| `apply_deadzone` | Rust 전체 소스 검색에서 대응 helper 없음. 이번에 `drive::apply_deadzone`으로 원본 분기 그대로 추가한다. |
| `get_lag_adjusted_curvature` | `drive::lag_adjusted`에 길이 불일치 시 영벡터, 속도/지연/거리 하한, 보간과 jerk 제한 존재. 원본 `psis_damping`은 계산 후 적용하지 않으므로 새 감쇠를 도입하지 않는다. |
| `clamp` | `drive::clip_curvature`가 `math::clip`과 전후 비교로 값/제한 플래그를 계산한다. 독립 tuple helper는 없다. |
| `smooth_value` | `modeld/src/action.rs::smooth`가 실제 호출의 `DT_MDL=0.05` 동작을 내장한다. `math::smooth`는 `0.01` 간격이므로 같은 기본값으로 취급하지 않는다. 임의 `dt` API는 아직 없다. |
| `clip_curvature` | `drive::clip_curvature`에 jerk, 80–120 km/h 가속도 보간, roll 보정, 절대 곡률 제한 및 플래그 존재. 기존 `tests/math.rs`는 저속 절대 곡률 제한 한 사례를 확인한다. |
| `get_accel_from_plan` | `plannerd/src/longitudinal_planner/target.rs::acceleration_from_plan`에 고정 17점 MPC의 가속도/정지 판정/현재속도 존재. `modeld/src/action.rs::from_plan`에는 고정 33점 모델의 최대속도까지 존재. 임의 slice 길이 불일치 시 원본의 영값 반환 경계는 이 고정 배열 API에 없다. |
| `curv_from_psis` | `modeld/src/action.rs::from_plan`에 속도 하한, yaw/초기 yaw rate 기반 곡률 계산 내장. 독립 scalar API는 없다. |
| `get_curvature_from_plan` | 같은 `from_plan`에 고정 모델 시간축 yaw 보간과 위 곡률 계산 내장. 임의 시간축 API는 없다. |

`CONTROL_N`과 모델 시간축은 기존 `drive.rs`에 있다. 다른 helper의 속도/jerk/가속도/
곡률 상수는 기존 대응 계산에 리터럴로 반영되어 있다. 이번 패치는 어떤 상수도 바꾸지 않는다.
이 비교는 소스 수준이며 모든 입력의 수치 동등성이나 파일 전체 전환 완료를 의미하지 않는다.

## 선택한 패치와 독립 회귀

쓰기 범위는 아래 세 파일뿐이다.

- `rust/crates/control-policy/src/drive.rs`: `apply_deadzone(error, deadzone)` 추가.
- `rust/crates/control-policy/tests/drive.rs`: 기존 drive 전용 파일이 없어 새 integration test 추가.
- `docs/naver/python-rust-523-drive-helpers.md`: 이 기록.

원본의 `error > deadzone`, `error < -deadzone`, 그 외 `0.0` 순서를 보존한다.
오차가 경계를 넘으면 폭을 빼거나 더한다. 기존 `controlsd/src/interface.rs`의
마찰용 deadzone은 범위 밖 오차를 그대로 사용하므로 이 함수와 합치거나 교체하지 않는다.
현재 원본에서 `apply_deadzone` 호출은 검색되지 않았으며 이번 변경은 native helper
누락을 메운다. 런타임 연결이나 Python 원본 삭제를 완료한 것으로 계산하지 않는다.

회귀는 원본을 실행하거나 구현식으로 기대값을 재계산하지 않고 명시한 고정 입력/결과를 사용한다.
이진 분수로 양·음 안쪽/경계/바깥 값, 영폭과 signed zero, 음수 폭의 분기 우선순위,
NaN 비교 및 무한대 경계를 확인한다. 이 비정상 입력 결과는 원본 helper의 비교 의미를
보존하는 것이며 제어 입력 유효성·안전 기준의 변경이 아니다.

## 검증과 부모 인계

로컬 결과: 지정 Rust 두 파일의 `rustfmt --check --edition 2021`과
`git diff --check` 성공, 소스/diff 검토 완료. 새 회귀는 4개 테스트에 27개 고정 입력을
담았으며 실행하지 않았다. Cargo, 테스트, 빌드, Clippy, Python 원본 및 모델 실행은
하지 않았다. 아래 명령은 부모가 GitHub Actions에서 실행할 selector다.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-control-policy --no-default-features --locked --test drive
cargo test --manifest-path rust/Cargo.toml -p openpilot-control-policy --no-default-features --locked --test math
```

`--test drive`는 새 helper 경계를, `--test math`는 기존 제어정책 회귀를 선택한다.
통합 SHA/Actions URL과 실행 결과는 부모가 기록한다. 이 단계에서는 실행 성공을 주장하지 않는다.
Cargo/mod/workflow/Focus/ledger/inventory, 커밋·push·CI 실행은 부모 전담이며 수정하지 않는다.

남은 coverage는 지연 보정과 곡률 제한의 세부 경계, 일반 시간축·길이 불일치 helper 계약,
기존 caller/fixture의 Python 제거다. 모델·레이더 구현이나 planner의 진행 중 테스트는
수정하지 않았다. 모델/레이더/실기·주행 수용 및 성능 개선의 증거를 추가하지 않는다.
