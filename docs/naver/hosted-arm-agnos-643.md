# GitHub 제공 ARM 러너와 기존 x86 AGNOS 기록 비교 (#643)

2026-10-30 KST 사용자가 태블릿 사용을 중단하고 GitHub 제공 ARM 러너로 비교 대상을
변경했다. 이어 기존 bin-actions 기록을 사용하고 해당 self-hosted 러너들에 새 빌드를
배정하지 말라고 명시했다. 새 비교 workflow는 `ubuntu-24.04-arm`만 선택한다.

태블릿 실행40937256101과 대기 중이던 호환성 확인40940667812는 모두 취소 완료했다.
라벨 등록 기록과 이전 전제 확인은 [태블릿 기록](tablet-agnos-native-643.md)에 남긴다.
미완성 태블릿 캐시 수정은 배포하지 않았으며 추가 태블릿 실행을 만들지 않는다.

## 비교 기준

- 기존 x86/QEMU: [40904508393](https://github.com/bin9208/openpilot-rust/actions/runs/40904508393),
  runner `bin-actions3`, job123556239177, artifact12924545511.
- 동일 소스: `9b8c6337b56b80b79788ebc389b6622b484fe0c7`.
- 동일 AGNOS19.8-carrot-bt1 이미지와 Rust1.94.0, 잠긴 Cargo 의존성을 사용한다.
- 원본의 `openpilot-runtime-build`와 `openpilot-runtime-plan` 컴파일 명령은
  x86/QEMU에서213.169038514초에 성공했다. 새 ARM에서도 동일 명령을 측정한다.
- 원래 전체 실행은 뒤쪽 bootstrap 메모리 제한에서 실패했다. 전체3시간과 새 ARM의
  부분 컴파일 시간을 나누어 전체 속도 향상으로 표현하지 않는다. 실제99개 runtime
  전체 빌드, 모든 provider/codec 검증의 완료를 이 비교로 주장하지 않는다.

자동 병렬화는 실제 컴파일 직전 CPU affinity 수를 사용한다. 사용자는1job 시험을
금지했다. 기존 호스트의 사용 가능 CPU는16개였고 GitHub ARM의 수는 실제 로그로
기록한다. CPU 수·CPU 성능·실행 방식이 모두 다른 두 환경의 소요시간 비교이므로
순수 QEMU 오버헤드만 분리한 수치라고 주장하지 않는다.

## 준비와 캐시

GitHub VM은 작업마다 새로 생성되므로 이미지·Rust 설치·Cargo 의존성 디렉터리를
`actions/cache`로 복원한다. 이미지와 rustup 실행 파일은 SHA256을 확인하고 Rust
버전/host를 검사한다. Cargo는 먼저 잠긴 의존성의 오프라인 준비를 시도하고 부족할
때만 받는다. 준비·이미지 추출 시간과 컴파일 시간을 별도로 기록한다.

새 AGNOS 실행 파일시스템과 컴파일 출력은 각 실행에서 새로 만들며, 같은 실행 안의
재빌드 시간도 따로 남긴다. 캐시가 없는 최초 GitHub ARM 실행의 다운로드 비용을
컴파일 시간에 합산하지 않는다. x86 self-hosted 작업은 새로 실행하지 않는다.

로컬 검사는 문법과 diff로 제한한다. 실제 ARM 실행과 캐시 준비 검증은 Actions에서
수행하며 결과 전에는 완료로 표시하지 않는다.

이슈: https://github.com/bin9208/openpilot-rust/issues/643

## 첫 실행과 전송 계층 수정

`fb0285c733c9970f3fa88fef17da5bae2e67e5f2`의
[40944913721](https://github.com/bin9208/openpilot-rust/actions/runs/40944913721)은 실제
GitHub Actions ARM 러너의4CPU를 확인했다. 이미지 다운로드8.582초, raw 변환11.687초,
파일시스템 추출10.720초에 성공했으나 기본 배포판 PRoot의 첫 guest 명령에서 signal11로
종료됐다. 컴파일 실패/성능 결과로 취급하지 않는다. 해당 SHA의 fast40944912982는 성공했다.

기존 x86 기준과 같은 PRoot5.5.0의 소스
`25dc6a3134891f98a79f57ce1c2c1b23ff15cad1`를 고정해 GitHub ARM 안에서 직접 빌드하고,
실제 PRoot 실행을 먼저 확인하도록 수정한다. AGNOS 이미지·소스·Rust·컴파일 명령은 유지한다.

## 별도 Fast checks 병행 실행 제거 — 2026-10-30 KST

사용자 지시로 현재 작업의 push마다 ARM 비교와 함께 실행되던 별도 `Fast checks`의
push 트리거를 제거했다. PR 필수 검사와 수동 실행은 유지하며, CI routing의 기존
검사도 새 이벤트 정책에 맞췄다. 로컬 확인은 YAML/Python 문법과 diff 검토로 제한한다.
진행 중이던 Fast checks40945498078만 취소하고 ARM 비교40945498124는 유지했다.

## 최종 측정 결과

비교 workflow source `0ada50ad0ce63c96e2c37d30e3ac6f8379c679b9`의
[40945498124](https://github.com/bin9208/openpilot-rust/actions/runs/40945498124),
job123699655185가 성공했다. GitHub 제공 ARM 러너의 CPU 로그는 `Neoverse-N2`,
affinity0..3, Cargo4jobs를 확인했다. 런타임 소스는 앞서 고정한9b8c6337... 그대로다.

| 동일 컴파일 구간 | 시간 |
| --- | ---: |
| 기존 bin-actions3 / x86 + QEMU + PRoot5.5.0 | 213.169초 |
| GitHub ARM / native ARM + PRoot5.5.0 | 19.940초 |
| ARM의 같은 출력 재빌드 | 0.365초 |

첫 컴파일의 x86/QEMU 대비 ARM 시간 비율은10.6905이며 약90.65% 감소했다.
이 구간은 두 실제 빌드 도구의 첫 컴파일이다. 서로 다른 CPU·병렬도 조건을 포함한
환경 간 비교로서 전체 AGNOS 실행이나 순수 QEMU 오버헤드에 비율을 그대로 적용하지 않는다.

ARM의 첫 준비는 이미지 다운로드22.777초, Rust 설치6.928초, 의존성 fetch15.491초였다.
측정 스크립트 전체165.247초, job 전체215초이며 준비 시간을 위 컴파일 시간에서 분리했다.
ARM ELF와 원본13그룹/99바이너리 catalog/feature 대조, 원본 Git source clean 확인이
통과했다. catalog의 `compiled=false`는99개 전체 runtime이 컴파일됐다는 뜻이 아니다.

최초 실행은 캐시 miss였으며 완료 뒤
`agnos-643-hosted-arm-inputs-v1-9b8c6337-rust-1.94.0` 키의 저장이 성공했다.
같은 job의 의존성 offline 재확인은0.515초에 통과했다. 별도 새 VM에서의 캐시 복원은
추가로 실행하지 않았다. 원래 x86 self-hosted 러너에 새 비교 작업은 배정하지 않았다.

해당 SHA의 fast40945498078은 표시된 검사 단계들은 성공했지만 최종 실행 상태가
`cancelled`이므로 전체 fast 성공으로 표현하지 않는다. ARM 비교는 성공이며
이 실험 브랜치를 dev/main에 병합하거나 장치에 배포하지 않았다.
