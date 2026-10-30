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

## 실험 Actions 진행

초안 커밋 `57a5e5afefec02205cc387e5695114af72681435`의
[40956391986](https://github.com/bin9208/openpilot-rust/actions/runs/40956391986)에서 ARM
정책·원래 명령 보존·계정 mapping 검증, genuine QCOM 컴파일과 오프라인 스케줄링,
원본 AGNOS 이미지의 실제 sudo/private cgroup 검증과 logger source-reuse가 모두
성공했다. 기본/기존 작업 브랜치의 배정은 바꾸지 않았다.

generic aarch64 job은 원래의 보호 브랜치 push 조건 때문에 위 push 실행에서 생략됐다.
원래 조건을 보존한 채 해당 항목만 수동으로 실행한
[40956730256](https://github.com/bin9208/openpilot-rust/actions/runs/40956730256)에서 실제
전체 workspace release 컴파일이 진행 중이다.
다른 빌드를 재측정하도록 x86 private 러너에 배정하지 않았다.

logger의 실제 첫 빌드는76.723초, 반복0.879초, 문서 변경0.889초, 작은 소스 변경은
5.500초였다. 문서 변경 시 모든 산출물이 fresh이고, 작은 소스 변경 시 관련 logger만
재컴파일되며 다른 logmessaged 산출물은 유지되는 원래 조건을 확인했다.

원래 작업 브랜치는 별도 세션의 package 테스트 정합과 조기 검사 변경으로
`e416c5d07d21588a294cfdeb14104f1602354d4c`까지 진행됐다. 이번 고정 측정 소스는
변경하지 않고, 실제 전환 시 이 후속 변경을 보존해야 한다.

generic 검증을 추적하며 정적 C3X probe의 원래 QEMU wrapper 호출이 남아 있음을
발견했다. ELF·synthetic procfs·실제 IPC3프레임 검사를 유지하고 `--native` 선택을 추가해
ARM에서는 직접 실행하도록 수정한다. legacy `--runner` 호출도 유지한다. 전체 generic
job의 명령에 QEMU가 남지 않도록 정책 검사를 추가했다. 실험 workflow는 이후 수동
선택 실행만 허용해 해당 generic 검증만 재실행하고, 이미 성공한 다른 ARM job과 원래
전체 AGNOS 측정을 반복하지 않는다. 기존 실행 결과와 수정 후 실행 결과를 구분한다.

`40956730256`은 GNU workspace release16분52초와 musl workspace release17분52초를
완료한 뒤, UTC22:09:48에 `FileNotFoundError: /usr/bin/qemu-aarch64-static`으로 실패했다.
컴파일 실패와 probe 실행 어댑터 실패를 구분한다. 수정 코드가 포함된
`e7548176f131a3600112d09f1d27a377b673ae46`의
[40962761699](https://github.com/bin9208/openpilot-rust/actions/runs/40962761699)에서 원래
generic 전체 job을 다시 검증한다. 새 ARM 정책 검사는 성공했고 실제 컴파일이 진행 중이다.

같은 검증의 미시작 대기 건40962047383만 취소하고, 새 SHA별 독립 concurrency group으로
수정본 검증을 배정했다. 실행 중이던40956730256과 원래 전체 측정40947309362를
취소하지 않았다. 이미 성공한 다른 ARM job은 새 선택 실행에서 생략한다.

## 전체 AGNOS의 PRoot 권한 처리 수정

원래 f62 전체 측정은 seed export의 원본 `ldd`에서 실패했다. 별도 ARM 진단으로 고정
PRoot의 ARM64 syscall table에 누락된 `faccessat2`439번을 확인하고 한 줄을 보완했다.
진단 [40966682467](https://github.com/bin9208/openpilot-rust/actions/runs/40966682467)에서
원래 seed ABI 함수·ldd·기본 loader·실제 chroot와 정상/거부 파일 권한 검사22개가 모두
통과했다. AGNOS 이미지와 ABI 검사는 변경하지 않았다.

같은 수정을 적용한 전체 재측정은 별도 실험 source
`78fe1dd503f9b3a9727c78bcb407b491290d9d87`,
[40967438168](https://github.com/bin9208/openpilot-rust/actions/runs/40967438168)에서 진행한다.
전환 초안 `08bf18f23b6a30301ff537ad5a06a5897b7de007`의
[정책40967329083](https://github.com/bin9208/openpilot-rust/actions/runs/40967329083)도
성공했다. 이미 진행 중인 generic 수정 검증은 같은 Rust/IPC 수정 source의
40962761699를 계속 사용한다. 기본/기존 작업 브랜치 배정은 아직 변경하지 않았다.

## Generic ARM 전체 검증 성공

source `e7548176f131a3600112d09f1d27a377b673ae46`의
[40962761699](https://github.com/bin9208/openpilot-rust/actions/runs/40962761699),
job123759750537은 UTC22:03:26부터22:56:08까지3162초(52분42초)에 전체 성공했다.
실제 허용 CPU는0..3이며 1job 제한은 사용하지 않았다.

GNU/musl 전체 workspace release, native Card/selfdrived/navigation/Panda,
VisionIPC ION 경계, camera/encoder/UI의 generic·ION 빌드와 원래 ELF 검사를 모두
유지한 채 통과했다. 정적 probe 보고서는 `execution=native-aarch64`, `frames=3`,
`result=pass`이며 실제 기기 검증은 수행하지 않았다. GNU Cargo 컴파일 보고는16분28초,
musl은17분24초다. job 전체 시간과 준비/후속 빌드가 포함된 step 시간과 구분한다.

따라서 조건부 전환 초안의 QCOM·factory transport·logger reuse·generic ARM 및
배정 정책 검증은 통과했다. 전체 AGNOS 재측정40967438168의 결과와 사용자 조건을
확인한 뒤 실제 배정 전환 여부를 결정한다.

## 현재 작업 브랜치의 검증 보정 보존

별도 source2375949c1은 loggerd -> bootlog -> manager의 실제 전이 Cargo 재빌드와
최종 ELF 변경을 구분하고, 검증된 compiler-provider loader로 help를 실행하도록 고쳤다.
이 부모 변경을 측정 브랜치640f3afc2를 통해 초안에도 merge해 보존한다. namespace의
native caller ID mapping과 root/소유권/정리 검사는 유지한다. Source-entry의 최종 보고는
실제 host 판정으로 native ARM CPU 여부를 보완하며 장치/AGNOS kernel 증거로 바꾸지 않는다.

helper-tests 선택 실행으로 원래 배정 정책과 새 source-entry 단위 검사, DevTools lib,
full-generation 재사용 판정 검사를 GitHub ARM에서 검증한다. 이미 성공한 generic/QCOM/
factory/logger 실행을 불필요하게 다시 배정하지 않는다. 전체 비교의 새 기준 source는
`2375949c19c63667ad66de0f9ee7e2c90ad8ba70`, 기존 QEMU40961936699이며,
ARM source `640f3afc22dec7681e3645f5de2040271cdb0d37`의
[40976444856](https://github.com/bin9208/openpilot-rust/actions/runs/40976444856)에서 비교한다.
기존78fe 실행은 보존했고 새 x86 실행을 추가하지 않았다.

통합 source `1cdf4a3235f087a35fdd2d8613689d7d5d8811c7`의
[helper40977304844](https://github.com/bin9208/openpilot-rust/actions/runs/40977304844)은
성공했다. Python 배정/원래 명령/ID mapping/source-entry 검사7+11+4+20개와
Rust DevTools lib87개, full-generation 재사용4개 검사가 모두 통과했다. Source-entry
20개는 메타데이터·해결 fixture 단위 검사이며 공개 입력을 쓰는 전체 실행으로 확대하지 않는다.

일반 AGNOS workflow의 시간 기록은 실제 실행 브랜치·SHA를 사용하며 비교 기준 run은
비워 둔다. 고정 비교 브랜치·기준 SHA·기존 QEMU run은 실험 workflow의 명시적 환경
변수로만 제공한다. 개별 명령 시간은 artifact에 보존하고 콘솔에는 요약만 출력한다.
진행 중인640f/78fe 측정 실행의 source와 시간 기록은 바꾸지 않는다.

별도 rust-isolation 정책에 남아 있던 workspace 무의존성 가정도 새 필수 ARM QCOM
검사 연결로 갱신한다. 기존 나머지 runtime job을 선행 조건으로 추가하지 않는다.
해당 정책 파일 전체를 실험 policy 선택 실행에 포함해 Actions에서 확인한다.
