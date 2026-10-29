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
