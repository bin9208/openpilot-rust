# 자체 빌드 트랙의 지정 runner·ARM 실행 연결 — #605

기준일: 2026-10-26 KST. 상위 #598/#599, 실행
[#605](https://github.com/bin9208/openpilot-rust/issues/605).

## 적용 근거와 범위

이 채팅에 새로 제공된 AGENTS.md는 이후 모든 Actions를 등록된 `bin-actions`로 실행하고
ARM 검사를 별도 ARM user space에서 유지하도록 명시했다. 앞서 시작한 native PR #603의
실행은 보존하고, 이 지침에 맞는 후속 검증 경로를 준비한 뒤 새 PR/병합 검사를 수행한다.

기준은 dev `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`이다. 별도 branch
`codex/feat-605-local-native-validation`에서 공통 실행 경로만 수정한다. 기존 #599의
자산/tree 구현과 dev-md runtime을 이 branch에 섞지 않는다. #502의 공통 job-temp,
Rust 경로 및 고정 QEMU/PRoot ARM 실행 helper를 재사용하며 실시간 기능·모델·안전·주기·
유효성 정책과 원래 비교/빌드 명령을 변경하지 않는다.

## 보존과 변경

- 모든 workflow의 실제 `runs-on`은 등록된 runner의 labels로 고정한다. job-temp와 ARM
  transport는 실제 runner 이름·환경도 확인한다. fork PR에서 다른 저장소의 소스를
  self-hosted에서 실행하지 않도록 원본 요구 검사에 같은 저장소 조건을 적용한다.
- ARM matrix와 원래 ARM 명령·feature·입력 pin·depends-on 관계를 유지한다. 호스트
  setup-python Action을 ARM에서 사용하지 않고 고정 ARM Python을 guest에 준비한다.
  host/guest/source/hash와 `physical_arm_cpu=false`를 artifact로 남긴다.
- guest target/toolchain과 job별 도구 출력은 소유 임시 디렉터리에 둔다. host-side
  artifact 업로드는 실제 guest target 경로를 참조하고 원래 결과물도 유지한다.
  X64는 원래 `rust/target` argv를 유지하면서 job 소유 target으로 연결한다. 이미 존재하는
  target은 덮어쓰지 않고 소유 link만 정리한다. 준비된 고정 host toolchain cache는
  Rustup의 버전 관리 경로를 재사용하며 ARM toolchain은 guest에 분리한다.
- GitHub 기본 이미지용 Android/GHC/.NET 삭제 명령은 제거한다. 작업별 실제 예상
  증가량 검사는 유지하고 폐기된25/35GiB 공통 reserve를 성장량 admission으로 바꾼다.
  device installer의 별도 용량·무결성 정책은 변경하지 않는다.
- 원래 job/의존성/matrix/명령 hash를 고정 source에서 기록했다. 정책 회귀는 실제
  `git show`로 baseline을 다시 읽고, 검토한 hosted 삭제 제거·고정 reserve 제거 외의
  원래 명령 변경/누락·실행 skip·continue-on-error 추가를 거부한다.

## 검증과 제한

로컬은 YAML/Python/Bash/Rust 구문·서식·diff/읽기 감사만 수행한다. 집중 workflow는
X64와 별도 ARM guest에서 기존 실제 Cargo 재사용 회귀, native 공간 guard 및 bootstrap,
실행 경로/원래 명령 보존 정책을 검사한다. ARM은 emulated execution이며 AGNOS나 물리
기기 효과를 검증하지 않는다. 실행 결과를 확인하기 전 성공으로 기록하지 않는다.

필수 fast/integration/docs/Rust/ARM 검사를 유지한다. 정확 source의 PR 검사와 병합 후
검사는 별도 수용이며, 이 기반 변경이 #598의 AGNOS 자체 빌드·다운로드 제거·정상 부팅·
녹화·업로드·사용자 수용을 완료한 것은 아니다. 테스트 안내와 native build 후속은
`codex/feat-599-verified-input-trees`의 `docs/naver/native-build-test-guide-598.md`에 유지한다.

성능 트랙 #604는 source `dfe09f66238dbecb74c43aa9ee177c36c4ae40a3`의
[bin-actions40304295923](https://github.com/bin9208/openpilot-rust/actions/runs/40304295923)에서
library66·정확 경로16·Clippy를 통과했고 PR #494에 후속 `02006e0f2`를 연결했다.
이 별도 dev-md 상태를 common dev source의 기능 성공으로 대신 사용하지 않는다.
