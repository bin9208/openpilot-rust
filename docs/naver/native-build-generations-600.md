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

2026-10-27 cache 입력 경계를 추가 검토했다. package/bin 선택 자체를 cache key에 넣으면
같은 compiler/provider/feature/environment의 직렬 그룹도 각각 target tree를 만들어
공통 의존성을 중복 보관한다. v2 cache key는 이 선택과 작업별 증가량 예측만 제외한다.
도구체인·실제 linker 도구 hash·target·feature·환경·고정 input/tree는 그대로 포함한다.
전체 recipe SHA는 package/bin 선택까지 별도로 묶어 원래 build/generation 검증을 유지한다.
이전 cache 디렉터리는 삭제하거나 새 결과로 표시하지 않는다.

Actions의 실제 offline Cargo fixture는 공통 Rust library를 쓰는 두 package를 따로
빌드한다. 두 그룹이 같은 target을 쓰고 공통 library의 hash/수정 시각과 build script의
한 번 실행을 유지하는지 확인한다. 공통 소스 변경 뒤에는 두 소비자의 실제 출력이 바뀌고
둘 다 재빌드되어야 한다. provider/feature 변경의 cache 분리는 기존 회귀로 유지한다.
전체 runtime/기기의 디스크 절감량이나 시간 개선은 이 코드 변경만으로 추정하지 않는다.

Cargo driver는 계속 고정된 빌드 checkout 경로와 내용 기반 target cache를 사용한다.
매 commit의 새 source worktree에서 직접 빌드해 절대 source 경로 변경으로 전체 crate가
다시 빌드되는 경로를 만들지 않는다. 보존한 source와 build report의 실제 source identity가
같은지 확인한 뒤 그 source root에 결과를 연결해야 한다. 서로 다른 SHA 결과를 같은 것으로
표시하거나 기존 release descriptor에 가짜 archive를 작성하지 않는다.

`openpilot-runtime-prepare --root CHECKOUT --plan PLAN_JSON --cache CARGO_CACHE
--toolchain NATIVE_RUST_BIN --source-store SOURCE_STORE --object-store OBJECT_STORE`는
이 순서를 실제로 연결한다. 그룹별 기존 Cargo driver를 직렬 실행하고, 서로 다른 target이나
중복 binary 소유를 거부하며, 실제 결과 roster·SHA·크기와 immutable object를 대조한다.
최종 준비 보고서에는 보존 source, 그룹별 toolchain/feature/input/result 보고서, binary별
object와 실제 추가 복사 바이트, 전체 경과 시간 및 원문 plan이 남는다. 검증 시 각 그룹의
recipe SHA와 binary roster를 원문 plan에 다시 대조하고 혼합 compiler/Cargo/linker 보고서를
거부한다. compiler/헤더/provider 입력은
각 recipe의 검증 정책을 그대로 적용한다. 이를 AGNOS 전체 설치 가능한 seal로 간주하지 않는다.

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
7. 실제 prepare CLI에서 독립 binary 두 개를 빌드한다. 문서만 변경하면 모든 Cargo 결과가
   fresh이고 추가 출력 복사가0바이트다. 한 binary만 수정하면 다른 binary는 fresh를 유지하고
   변경 출력 크기만 복사한다. mutable source와 Cargo cache를 제거해도 이전 준비본 검증 및
   이전 ELF 실행 결과가 유지된다.

검증 대기 상태다. 정확 SHA와 Actions 결과는 실행 뒤 이 기록 및 #600에 추가한다.
실기 자체 빌드·정상 부팅·녹화·기존 업로드 수용은 완전한 후보 준비 뒤 사용자가 수행한다.

## source-build 세대 구성과 선택 연결 (검증 대기)

`openpilot-runtime-native --config CONFIG_JSON --build --root REQUEST_CHECKOUT`는
고정된 store 소유 `build-workspace`에서 기존 그룹 driver를 실행하고 실행용 source root를
별도로 만든다. 처음 설치한 checkout과 updater overlay의 절대 경로가 달라져도 Cargo 입력
경로는 고정한다. 작업 중인 workspace는 별도 잠금을 유지하고, dirty/외부 workspace는
reset하거나 덮어쓰지 않는다. 실행용 root는 같은 commit 재시도 때도 새로 만들어 이전
정상 세대에 파일을 쓰지 않는다.

컴파일 입력은 그룹 recipe에 유지한다. 모델·실행 데이터처럼 해당 binary 컴파일에 영향을
주지 않는 내용은 plan의 `runtime_inputs`와 별도 검증 결과에 기록한다. 이를 변경해도
무관한 Cargo cache key가 바뀌지 않는 실제 CLI 회귀를 추가했다. 컴파일에 영향을 주는
헤더·정적 provider·생성 입력은 여전히 관련 recipe에 고정해야 한다. Report schema3은
실제 compiler bin 디렉터리도 남기며 전체 toolchain receipt root와 대조한다.

세대 구성은 기존 manager inventory sealer와 `runtime-elf-audit`를 같은 소스에서 빌드한
실행 파일로 호출한다. 기존 installer의 source/경로/크기/SHA/AArch64 ELF 검사를 공개
API로 재사용하고 release archive 항목은 만들지 않는다. 각 ELF와 외부 AGNOS library,
soname의 실제 선택 경로와 hash를 묶으며 보고서가 모든 runtime ELF를 다루어야 한다.
large tree는 봉인 경계의 시작/끝에서 각각 한 번 확인하고 그룹·library마다 다시 읽지 않는다.
actual `/AGNOS`·`/VERSION`과 offroad 상태가 없으면 해당 native 작업을 거부한다.

실제 Git blob을 읽어 `lane.onnx`/`v_asm_model.onnx`가 LFS 포인터임을 확인했다.
source asset 해제는 그 커밋의 OID/크기와 정확히 일치하는 내용만 허용한다. 같은 길이의
다른 모델이나 receipt 재작성, 일반 소스 변경은 허용하지 않는다. 기존 sealer에는 검증한
source root·LFS 목록을 전달하는 명시적 입력만 추가했고 기본 release 경로는 유지했다.
build.json은 원래 요청 source의 branch/origin/commit/date를 보존하고 descriptor로 hash를
묶는다. 다른 SHA로 실행한 결과를 새 요청 SHA의 결과로 표시하지 않는다.

완전히 검증한 descriptor만 게시하고, `--activate ID`는 offroad 재확인 뒤 active JSON을
원자적으로 전환한다. 이전 정상 후보를 최대2개 함께 기록하며 `--resolve`는 실제 source와
출력이 검증되는 후보만 반환한다. 오류·잠금·잘못된 상태는 pointer와 로그를 보존한다.

**아직 배포 후보가 아니다.** 실제 pinned AGNOS toolchain/header/provider 준비와 전체
그룹 recipe, 자동 bootstrap 연결, 전체 AGNOS 회귀·실제 자원 계측·사용자 기기 수용이
남는다. 아래 launcher/updater 연결은 구현했으나 Actions 검증 전이다. protocol gate는
유지하며 launcher가 같은 source-built updater의 protocol도 확인해야 통과한다. 이 응답을
manager·모델·녹화·업로드의 성공 결과로 해석하지 않는다.

## 빌드 중단과 다음 부팅의 업데이트 (검증 대기)

`72bb0618a32893edf1e94f5d9ca43351829906fc`는 전용 frontend/worker process group,
Linux parent-death signal, offroad 감시를 추가했다. frontend TERM/KILL, worker KILL,
IsOnroad 전환, 정상 종료 후 실제 Cargo/build script/손자 프로세스가 남지 않고
관련 없는 실행·이전 active 표식·로그·workspace 잠금이 보존되는 회귀를 Actions에 보냈다.
[정확 SHA 집중 검사](https://github.com/bin9208/openpilot-rust/actions/runs/40329594561)는
대기 중이며 아직 통과 근거가 없다.

이어서 추가한 `--stage-update --root MERGED --previous ID`는 봉인된 후보의 descriptor,
기존 active ID, config SHA를 `pending.json`에 묶는다. 빌드 중에는 active pointer를 바꾸지
않는다. 다음 launcher가 기존 boot lock을 잡은 뒤 후보를 다시 검증·선택하고, 기존 lock
handoff로 후보 launcher를 실행한다. pointer 변경 뒤 전원이 끊겨 journal 삭제가 안 된
경우에도 같은 후보로 이어서 처리한다. 손상·다른 config·다른 active 상태·미검증 후보는
journal과 이전 실행·로그를 보존한다.

updater는 실행 root를 overlay lower로 쓰지 않는다. source store의 object만 공유하는
자체 `.git` directory의 clone을 별도로 만들며, `.git`가 외부를 가리키는 retained worktree를
overlay lower로 재사용하지 않는다. clone의 source/branch/origin/object 소유를 확인하고,
dirty/foreign checkout을 자동 reset하지 않는다. generation별 staging도 분리한다. 후보
시작 전에 선택된 세대의 loader library 경로를 고정해 이전 세대 provider를 물려받지 않는다.
native 경로는 기존 release downloader를 호출하지 않으며 legacy 경로는 유지한다.
update clone 준비 실패는 native updater를 중단하고 검증된 정상 runtime 시작을 유지한다.

로컬은 구문·서식·diff와 TOML 경로 의존성의 읽기 감사만 수행했다. Actions는 실제
source-built launcher/updater, 원래 전체 회귀·strict Clippy, protocol 응답/도구 누락 거부를
X64와 별도 ARM QEMU/PRoot에서 검사한다. 전체 AGNOS에서 세대 A→B의 정상 시작·복구와
실제 녹화·업로드 성공은 여전히 별도 증거가 필요하다.
