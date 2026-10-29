# 조건부 ARM 러너 전환의 실험 검증 (#643)

2026-10-30 KST 전체 AGNOS 비교가 준비 단계에서 오래 진행돼, 대기 시간에 전환 초안을
별도 `codex/feat-643-arm-routing-draft`에서 검증한다. 기본 브랜치와 기존 작업 브랜치의
ARM 배정 전환은 전체 비교 결과를 확인한 뒤 진행한다. 현재 초안을 전환 완료로 취급하지 않는다.

실제 측정은 원래 `codex/feat-643-agnos-arm-full`의
`f62e361d59afa2e12125d416f372a1042bcbdfb9`,
[40947309362](https://github.com/bin9208/openpilot-rust/actions/runs/40947309362)에서 계속한다.
실험 브랜치 변경은 이 실행을 변경·중단·재시작하지 않는다.

## 초안과 검증 범위

- 원래 AGNOS, generic aarch64, logger source-reuse의 ARM 항목, factory transport와
  source-entry를 GitHub ARM에 배정한다. X64 전용 검사는 기존 배정을 유지한다.
- QCOM frontend는 같은 compiler SHA256과 ELF 의존성 검증을 유지하며 네이티브 실행을
  명시할 수 있게 한다. 기존 QEMU 호출 형식도 보존한다.
- genuine QCOM 테스트 두 파일은 전용 ARM job에서 전부 수행한다. workspace는 해당
  ARM 검증이 성공해야 수용하고, 원래 나머지 테스트를 유지한다.
- factory namespace 안의 실제 comma uid/gid1000, 원본 sudo, read-only proc,
  cgroup과 정리 소유권 검증을 유지한다. GitHub runner uid/gid가1001인 경우 private
  namespace mapping과 소유한 추출 이미지의 ID를 대응시킨다. 전역 ID나 설정은 바꾸지 않는다.
- 같은 ARM job들의 generic build, source-reuse, genuine QCOM, 실제 factory sudo/cgroup
  검증을 실험 workflow에서 실행한다. 정책 검증은 원래 명령 보존과 QCOM 테스트 분리 후의
  필수 검증 연결, ARM 입장 조건과 ID mapping의 전체 범위를 확인한다.

source-entry의 전체 공개 입력 기반 빌드는 실제 committed input index와 공개 metadata가
필요하다. 이 실험의 factory transport 통과를 source-entry 전체 빌드 통과로 표현하지 않는다.
원래 preflight와 실제 공개 입력/자원/타임아웃 검증은 유지한다.

로컬 Python/YAML/Bash 문법, rustfmt, diff 점검을 수행했다. 기능·빌드는 GitHub ARM에서만
검증한다. 조건이 충족되고 검증이 끝나기 전 기본/기존 작업 브랜치에 적용하지 않는다.

이슈: https://github.com/bin9208/openpilot-rust/issues/643
