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
