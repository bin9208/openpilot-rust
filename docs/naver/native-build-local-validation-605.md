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

### PR #606의 준비 대기·APT·기존 CI 보존 보완

기준 `bfffa2d30d1e8037cc3f7eb80b5a2b5264c8070c`의 필수 Rust 실행
[40315420260](https://github.com/bin9208/openpilot-rust/actions/runs/40315420260)에는
CarrotMan ARM 준비 대기와 webcam X64 APT 실패가 확인되었다. 기존 실행은 보존한다.
후속 source에 #609의 실제 peer 준비 대기와 #610의 job 범위 native APT를 함께 반영한다.
원래 기능 deadline·입력 pin·ARM 명령과 명시적인 cross APT 인자는 그대로 유지한다.
#609의 별도 ARM 성공은 source `18e174c2788af1eb3d8de3e20826a3d84160cb43`의 근거이며,
합친 새 source의 전체 PR 검사 성공으로 대신 쓰지 않는다. #610의 집중 실행도 별도다.

후속 커밋이 기존 실행을 중단시키지 않도록 validation workflow9개의 branch별 group을
`-preserve-v1` 접미사의 새 이름 공간으로 옮기고 `queue: max`, `cancel-in-progress: false`를
적용한다. 기존의 중도 취소 group과 충돌하지 않아 전환 시점의 기존 대기도 보존한다. 재사용되는
integration 하위 tests/naver/APK와 문서 설정 검사도 포함한다. publish/sync는 변경하거나
활성화하지 않는다. concurrency가 없는 다른 workflow는 기본 실행을 유지한다.

[GitHub의 공식 concurrency 규칙](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)에
따르면 취소 false만으로는 기본 pending 한 개의 교체를 막지 못한다. `queue: max`는 같은
group에 최대100개 pending run을 보존하며, 그 한도 뒤의 새 run은 취소된다. job 수가 아니라
workflow run 대기 한도다. 검사 실행 순서/runner는 이 큐와 기존 job needs에 따라 유지된다.
기존 실행을 수동 취소하거나 검사 결과를 다른 SHA의 성공으로 바꾸지 않는다.

정책 회귀는 이9개 queue/취소 설정과 기존66개 job·needs·matrix·명령 보존을 함께 확인한다.
로컬은 YAML/Python/Bash 구문·diff/읽기만 확인한다. PR 갱신 후 기존 run의 상태를 다시 읽어
보존 여부를 확인하고, 정확 새 SHA의 필수 검사와 별도 병합 후 검사를 기다린다.

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

## 최초 실제 실행과 에뮬레이션 시간 제한 보완

source `b865767054f3415e7d5402060c7c8be6c88fc3e1`의
[40308571039](https://github.com/bin9208/openpilot-rust/actions/runs/40308571039)는
X64 전체 leaf가 성공했다. 실제 runner 이름은 양쪽 모두 `bin-actions`다.
ARM QEMU도 원래 offline Cargo 회귀4개와 strict Clippy, 명령·경로 보존 정책4개,
disk-space library4개와 CLI5개를 통과했다. 이후 dev-tools 공간 회귀의 새 의존성을
컴파일하던 중45분 제한으로 종료됐다. API의 job conclusion은 `cancelled`이며 annotation은
`The job has exceeded the maximum execution time of 45m0s`다. ARM 전체 성공으로
기록하지 않는다. 원래 Cargo 회귀는10:53:47~11:03:19 UTC에 성공한 별도 step이다.

집중 ARM 예산을90분으로 늘리고 기존 ARM 원본 검증 job은 에뮬레이션에 맞게180분,
더 큰 Xiaoge job은240분 상한을 적용한다. matrix의 X64 상한과 원래 명령·feature·pin·
의존 관계는 유지한다. 원래 ARM 비교를 생략하거나 hosted fallback을 도입하지 않는다.
정책 회귀도 ARM 예산과 원래 X64 상한 유지 여부를 검사한다. 소유 임시 출력·ARM proof
artifact와 정리는 취소 때도 성공했으며 다른 job/cache를 삭제하거나 중단하지 않았다.

dev 대상 정상 검토 PR은 [#606](https://github.com/bin9208/openpilot-rust/pull/606)이다.
최종 source의 빠른·집중·필수 PR/병합후 결과는 별도로 확인해야 한다.
