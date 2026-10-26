# #600 실행 세대의 소스와 출력 보존

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
자산·증분·복구 [#600](https://github.com/bin9208/openpilot-rust/issues/600)의 구현 기록이다.
기존 Python 모델 준비와 C/C++ provider를 유지하며 dev용 공통 도구만 변경한다.

## 구현 경계

- `openpilot-runtime-source --root CHECKOUT --store SOURCE_STORE
  --expected-growth-bytes BYTES`는 깨끗한 실제 Git commit/tree와 초기화된 중첩
  submodule을 별도 bare 저장소로 가져온다. 네트워크 fetch나 runtime release 다운로드는
  하지 않는다. 외부 경로와 현재 checkout의 `.git`에 의존하는 worktree는 거부한다.
- 각 source root는 처음 만든 경로를 유지하고 잠근다. worktree를 임의 rename하지 않는다.
  정상 receipt는 실제 source·submodule·독립 Git 소유권 확인 뒤 원자적으로 게시한다.
  같은 commit은 검증한 기존 root를 사용한다. 취소·전원 중단으로 receipt가 없는 시도는
  다음 정상 시도의 선택 대상이 아니며 이 명령이 임의로 삭제하지 않는다.
- `objects::import`는 변경 가능한 Cargo 출력에서 한 번 복사·SHA/크기 검사·fsync한
  읽기 전용 content object를 만든다. candidate에 hardlink할 수 있는 것은 이 복사본이다.
  Cargo 출력 자체는 hardlink하지 않는다. 동일 내용과 실행 mode는 다시 검증하고
  `stored_bytes=0`으로 재사용한다. 손상·symlink·특수 권한·공간 부족·동시 소유는 오류다.
- 공간은 해당 source 가져오기의 호출자 예상 증가량 및 출력 복사 크기로 판단한다.
  폐기된 고정25/35GiB 기준이나 다른 세션 cache 삭제를 사용하지 않는다.

이 두 기능은 아직 설치·활성화 명령이 아니다. 자체 빌드 전체 recipe, AGNOS/ABI와
provider/generated 검증, source-build manifest, 정상 세대 선택, launcher/updater 연결이
남아 있다. receipt가 있다는 이유만으로 기기 사용 가능한 전체 runtime으로 표시하지 않는다.
이전 A가 정상이고 새 B가 실패했을 때 A의 binary뿐 아니라 A의 source root를 실제 실행하고
A로 표시하는 것이 최종 연결의 기준이다. 로그와 사용자가 수정한 source를 삭제하지 않는다.

## 증분 빌드 경로

Cargo driver는 계속 고정된 빌드 checkout 경로와 내용 기반 target cache를 사용한다.
매 commit의 새 source worktree에서 직접 빌드해 절대 source 경로 변경으로 전체 crate가
다시 빌드되는 경로를 만들지 않는다. 보존한 source와 build report의 실제 source identity가
같은지 확인한 뒤 그 source root에 결과를 연결해야 한다. 서로 다른 SHA 결과를 같은 것으로
표시하거나 기존 release descriptor에 가짜 archive를 작성하지 않는다.

## 검사 계획과 현재 상태

로컬은 Rust 구문·서식·diff만 확인한다. 기능·빌드는 모든 job이 등록된 `bin-actions`에서
실행하는 Actions로 검증한다. ARM은 별도 QEMU/PRoot user space이며 물리 ARM 검증이 아니다.
기존 #605 공통 helper를 정상 merge하고 원래66개 job의 명령·needs·matrix를 유지했다.
추가 모듈 서식 검사는 별도 step이며 원래 Cargo 회귀 명령은 축소하지 않는다.

추가 Actions 회귀는 실제 Git과 실행 파일을 사용한다.

1. 새 source의 dirty 실패 및 정상 갱신 뒤 원래 mutable checkout과 `.git`를 제거해도
   A/B의 실제 SHA와 각 source의 실행 결과가 유지된다.
2. 중첩 submodule의 원본 저장소를 제거해도 보존본이 검증되고, 초기화 누락이나 변경은 거부된다.
3. 부분 시도 잔존·자원 부족·잠금 충돌에서 이전 정상 root와 파일을 유지한다.
4. 실제 source CLI에서 잘린 receipt·receipt/store symlink를 거부한다.
5. 변경 가능한 Cargo 출력을 덮어쓴 뒤에도 이전 candidate ELF가 실행되며 동일 출력의
   추가 복사는0바이트다. 기존 candidate 경로를 덮어쓰지 않는다.
6. content object의 손상·symlink·특수 권한·공간/잠금 오류는 게시·연결 전에 실패한다.

검증 대기 상태다. 정확 SHA와 Actions 결과는 실행 뒤 이 기록 및 #600에 추가한다.
실기 자체 빌드·정상 부팅·녹화·기존 업로드 수용은 완전한 후보 준비 뒤 사용자가 수행한다.
