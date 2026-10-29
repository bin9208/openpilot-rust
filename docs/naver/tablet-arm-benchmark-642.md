# 태블릿 ARM 러너 병렬 빌드 측정 (#642)

사용자가 등록한 `bin-Arm-tablet`에서 ARM 네이티브 실행과 PRoot 멀티코어 활용을 확인한다.
별도 `codex/feat-642-tablet-arm-benchmark`의 기존 등록 `rust.yml` 경로만 수동 측정용으로 바꾸며 dev/main에는 적용하거나 병합하지 않는다.

외부 crate 없는 동일 16-library workspace를 새 target 디렉터리에 1,2,4,4,2,1 순서로 빌드하고 결과 바이너리 출력 일치를 확인한다.
환경/도구 설치는 측정 이전에 분리한다. 각 빌드 wall time과 자식 프로세스 CPU 시간 합계를 기록한다.
`CPU 시간 / wall time`은 평균 사용 코어 수의 근사이며 순간 core occupancy나 affinity 측정은 아니다.

OS/표준 라이브러리 파일 캐시, 순서 효과, 발열과 다른 태블릿 앱의 영향을 배제하지 못한다.
온도 센서는 본 probe에서 측정하지 않는다. 합성 빌드 결과는 프로젝트 전체·AGNOS 실행 성능이나 검증 완료의 근거가 아니다.

로컬 Python 문법, shell 문법, YAML parse 검사를 통과했다. 실제 결과는 이 브랜치의 Actions 실행에서 확인한다.
이슈: https://github.com/bin9208/openpilot-rust/issues/642

## 실제 측정 — 2026-10-30 KST

정확 실행 커밋: `518fae1b5cb6df59c955d538e06ba65518fc0b72`.
[성공 Actions](https://github.com/bin9208/openpilot-rust/actions/runs/40935482146), job `123664767669`.
태블릿은 `aarch64`, 논리 CPU8개·affinity0~7, Ubuntu의 cargo/rustc1.75.0, GCC13.3.0으로 실행됐다.
누락된 기본 Rust 패키지는 준비 단계에서 설치됐고 그 시간은 아래 빌드 시간에 포함하지 않았다.

| Cargo jobs | 벽시계 시간 중앙값(초) | 1 대비 배속 | 자식 CPU시간/벽시계 평균 |
|---|---:|---:|---:|
| 1 | 7.779 | 1 | 0.827 |
| 2 | 4.153 | 1.873 | 1.448 |
| 4 | 3.231 | 2.408 | 2.082 |

6회 모두 실제 ARM 바이너리를 실행했고 출력이 일치했다. 4jobs는 1jobs 대비 약2.4배 빨라 이 구성의 단일 코어 제한은 관찰되지 않았다. 평균 코어 수는 tracer CPU시간을 포함하는 완전한 시스템 프로파일이 아니며 8개 코어를 모두 포화시켰다는 증거로 쓰지 않는다.

첫 [실행40935213809](https://github.com/bin9208/openpilot-rust/actions/runs/40935213809)은 step 실행 전 upload-artifact 액션의 tar 추출에서 ENOENT로 실패했다. 두 번째 실행은 같은 측정을 외부 Action 없이 순수 셸로 수행해 기본 ARM 빌드/실행을 분리 검증했다. 일반 checkout/upload Action 호환성은 아직 해결·검증되지 않았다. 프로젝트가 요구하는 Rust1.94.0으로의 실제 프로젝트 빌드도 별도 검증이 필요하다. 이 합성 진단용 rust.yml은 dev/main에 병합하지 않는다.
