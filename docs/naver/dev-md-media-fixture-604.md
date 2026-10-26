# media/raylib 가상 회귀의 공간 관측 분리 — #604

기준일: 2026-10-26 KST. 상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
실행 [#601](https://github.com/bin9208/openpilot-rust/issues/601),
결함 [#604](https://github.com/bin9208/openpilot-rust/issues/604), 성능 PR #494.

## 확인한 실패

source `72ba387abaf54f9116409402c141faa5bb1ca54a`의
[workspace120412465878](https://github.com/bin9208/openpilot-rust/actions/runs/39936752664/job/120412465878)는
FFmpeg #521 보완 회귀를 통과했다. 그러나 dev-tools lib는52 passed/12 failed였다.
media_validation4개와 raylib_stage8개는 작은 합성 입력과 가상 compiler를 쓰는데도
실제 filesystem의1GiB 여유를 요구해 의도한 정상/오류 경로까지 도달하지 못했다.
단순 빌드 전 admission 실패나 CPU runtime의 기능 회귀로 표현하지 않는다.

## 수정 범위

- 공개 provider 검증/빌드 entry는 기존 실제 filesystem 관측과1GiB 예상 증가량을
  사용한다. 공개 CLI/env 우회·ignored test·무조건 성공을 추가하지 않는다.
- private 구현에 공간 관측만 주입하고 fixture는 같은 `Space::require` 판정에
  고정 관측을 전달한다. source/hash/허용 경로/header/license/실행 명령/오류/원자성
  검사를 유지한다. 테스트에 필요한 실제 작은 ELF compiler의16MiB 검사는 유지한다.
- entry/준비 후 link·probe 단계에서 부족 공간 또는 관측 실패를 주입한다. 명령이
  실행되지 않고 기존 raylib 세대의 inode/내용 및 media의 이전 report가 보존되어야 한다.
- runtime delivery에는 검토한7개 source/test 파일·집중 workflow·이 기록만 정확 경로로
  허용한다. 접미사·하위 경로·검토하지 않은 media 파일은 계속 거부한다.

## 검증과 남은 수용

로컬은 Rustfmt·Python AST/YAML 구문·diff만 확인한다. 기능/빌드/Clippy는
`.github/workflows/media-fixture-space.yml`에서 실행한다. 새 지침에 따라 모든 후속
Actions는 등록된 `bin-actions`/`dev-md-local`을 사용한다. 최초 조회에서 runner22는
online/idle이었다. 다른 작업을 취소하거나 cache/로그를 삭제하지 않는다.

집중 실행은 X64 합성 fixture/실제 소형 ELF parser 증거다. ARM은 기존 전체 PR 경로의
QEMU/PRoot 별도 ARM user space에서 유지하고 실제 ARM CPU·AGNOS·기기 수용과 구분한다.
실행 전에는 성공으로 계산하지 않는다. 정확 source의 집중/필수 PR/병합 후 결과와
실제 AGNOS 배포·성능·사용자 수용은 별도다. #604와 #598은 모든 완료 조건 전까지 연다.

최종 사용자 자체 빌드·정상 부팅·녹화·업로드 절차는 native-build 트랙의
`docs/naver/native-build-test-guide-598.md`에 유지한다. 이 수정은 dev-md 성능 트랙이며
dev/main에 runtime을 섞지 않는다. 모델 준비 Python과 기존 native library를 유지한다.
