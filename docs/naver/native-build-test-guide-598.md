# 콤마 자체 빌드·다운로드 재사용 검증 안내 — #598

기준일: 2026-10-27 KST. 상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
캐시 [#600](https://github.com/bin9208/openpilot-rust/issues/600),
전환분·성능·전달 [#601](https://github.com/bin9208/openpilot-rust/issues/601).

## 현재 검증 범위

빌드 기반 PR #602는 source `13d7ce113bf602b2be5b5abedb55c4debeb83a8c`의 전체68개
검사 성공 후 `dev`의 `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`으로 병합했다.
[x64/ARM 집중 검사](https://github.com/bin9208/openpilot-rust/actions/runs/40274067028),
[필수 PR Rust 검사](https://github.com/bin9208/openpilot-rust/actions/runs/40274071261),
[필수 PR 통합 검사](https://github.com/bin9208/openpilot-rust/actions/runs/40274071270)는
성공했다. 병합 후 검사는 별도로 확인한다.

`openpilot-runtime-assets`는 대형 자산을 위한 내용별 캐시 준비 도구이며,
`openpilot-runtime-build`는 준비된 offline Cargo 의존성으로 지정 binary를 빌드한다.
전체 AGNOS recipe·toolchain 준비·ABI 봉인·세대 활성화·기존 startup/update 연결은
아직 완료되지 않았다. 아래 CI fixture 결과를 장치 설치 가능 증거로 사용하지 않는다.

## 개발 검증 실행 방법

생성 MPC 입력4662개와 원래 output/현재 source를 함께 묶어 기존 C/C++ solver를
재사용하는 조건, 변경·누락 거부 및 실제 ARM Rust/direct C ABI12 solve 비교는
[MPC 소스 결속 안내](native-mpc-source-bindings-599.md)를 따른다. 새 생성·전체 Python/
Cython planner 회귀와 실기 수용은 별도로 남으며 Actions 성공 전 실행 통과로 쓰지 않는다.

전체99개/13그룹의 전이/default feature에 따른 compiler 입력 목록과 필수 Tree 결속은
[compiler 입력 계약](native-compiler-input-contract-599.md)을 따른다. catalog audit의
compiler_inputs_bound와 compiled/activated를 구분하며 engineering plan을 전체 runtime
완료로 사용하지 않는다.

기존 OpenCV4.13.0의168개 source header와 원래 생성 설정 헤더2개 준비, 반복 압축 해제
생략/전송0/복사0과 ARM API object 검사는 [OpenCV 헤더 안내](native-opencv-headers-599.md)를
따른다. 같은 shared library5개와 headers/alias185행을 구성하고 실제 ARM Rust bridge를
검사하는 방법은 [OpenCV prefix 안내](native-opencv-prefix-599.md)를 따른다. 전체
webcam/Xiaoge 원본·모델 회귀와 실기 검증은 별도로 필요하다.

기존 UI raylib의 원래 헤더/static archive와 독립 shared provider 준비, 반복 전송/복사0,
캐시 영수증 위조 거부 및 실제 ARM loader 컴파일/정적 ABI 검사는
[raylib 입력 안내](native-raylib-inputs-599.md)를 따른다. GPU 화면 출력과 전체 runtime
시작 성공은 이 정적 검사로 대신하지 않는다.

기존 estimator Eigen3.4.0 입력을 원래 wheel에서 준비하고 필요한 wheel만 선택하는 방법과
전송/복사0·실제 C++ helper 검사는 [Eigen 입력 안내](native-eigen-inputs-599.md)를 따른다.
이 검사를 전체 locationd/paramsd 또는99개 runtime의 통과로 대신 쓰지 않는다.

전체 generation의 모든13그룹에 같은 실제 `/` SDK를 필수로 묶고, 그 세대 소스의
전체 SDK profile과 일치하는지 확인하는 회귀는 [SDK 검사 안내](native-system-sdk-599.md)를
따른다. profile 회귀와 실제 SDK namespace 검사, 전체 runtime 빌드/실기 수용을 구분한다.

FFmpeg6.1.1의 header/provider를 실제 Rust 인코더가 소비하는 단일 prefix와 원본5개
codec case의 정확 packet 비교는 [provider 검사 안내](native-provider-components-599.md)의
단일 prefix 절차를 따른다. 변경 SHA의 foundation 및 genuine 결과가 성공하기 전에는
실행 성공으로 판단하지 않는다. QEMU/PRoot AGNOS user space 결과는 실제 comma 3X의
설치·부팅·카메라/녹화·업로드 수용과 구분한다.

Rust compiler/Cargo/std 내용별 준비·전송0/복사0·stdlib 변조 검증은
[고정 native Rust 준비 안내](native-rust-toolchain-599.md)를 따른다. compiler 실행과
전체 runtime 수용은 별도 단계이며 새 source의 Actions 성공을 먼저 확인한다.

capnp SDK의 입력 pin·그룹별 도구 검사와 전체 원본 schema/wire 비교는
[SDK 안내](native-capnp-sdk-599.md)에 있다. 이 SDK 준비·예제는 CI 개발 검증이며,
장치 설치 가능한 전체 후보의 준비 완료를 뜻하지 않는다.
프로젝트의 `--prepare-capnp`는 같은 고정 .deb를 installer 실행 없이 준비한다.
검사에서는 독립 dpkg-deb의 vendor tree 전체 일치와 실제 source-built ARM 준비기의
초기/반복 전송0·반복 복사0·같은 tree SHA를 확인한 뒤 원본 wire 비교를 수행한다.

외부 Cargo crate의 원본 고정·내용별 준비와 driver의 directory-source 연결은
[Cargo 입력 안내](native-cargo-inputs-599.md)를 따른다. empty Cargo home의 실제 빌드,
반복/docs-only freshness와 source·lock 변조 거부를 Actions에서 확인한다.
genuine의 ARM 준비 검사는 host에서 만든 seed의 검증/재사용이며 새 기기 준비 실측이 아니다.

처음 Rust 입력 준비 도구를 얻는 순환은 [고정 bootstrap seed](native-bootstrap-seed-599.md)에
정리했다. 후보 추출·독립 descriptor pin·실제 AGNOS ABI·새 Rust prefix의 compiler 검사를
Actions에서 수행한다. 공개 seed/기본 시작과 전체 기기 수용은 아직 준비 완료가 아니다.

로컬에서는 `git diff --check`, scoped `rustfmt --check`, TOML/YAML 문법과 diff만
확인한다. 기능·빌드는 GitHub Actions에서 실행한다. 작업 branch push는
`Runtime source build foundation`의 x64/ARM matrix를 실행한다. 같은 변경의 성공한
검사를 반복하지 않는다. 의도적으로 별도 재현이 필요할 때만 다음 명령을 쓴다.

```sh
gh workflow run runtime-source-build.yml --repo bin9208/openpilot-rust --ref BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

CI의 실제 명령은 다음과 같다. 실제 fixture는 격리된 임시 Git checkout과 작은 입력을
사용하며 사용 중 runtime·기기·로그를 수정하지 않는다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-runtime-build -- --nocapture
cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-runtime-build --all-targets -- -D warnings
```

| 검사 | 합격 결과 |
| --- | --- |
| 같은 model/provider/toolchain 준비 반복 | 두 번째 전송 호출0, `transferred_bytes=0`, 기존 내용 동일 |
| provider 하나 변경 | 변경 provider만 전송, 기존 model/toolchain 전송0, 이전 provider 보존 |
| 잘못된 hash/길이·변조된 캐시 | 실패, 잘못된 blob 미발행, 다른 자산 보존 |
| 다운로드 중 실패·자원 부족·동시 owner | 실패, 기존 자산과 runtime 보존 |
| symlink cache/blob·비 HTTPS 입력 | 외부 파일 수정 없이 실패 |
| 문서만 수정한 새 source commit | Cargo `fresh=true`, 같은 binary hash, 보고서 source는 새 commit |
| runtime source 수정 | 수정 대상 재빌드·출력 변경, 관련 없는 target은 fresh/같은 hash |
| 미커밋 입력·빌드 중 source 변경 | 성공 보고서 발행 거부, 이전 성공 보고서 보존 |
| 압축 해제된 provider/도구체인 tree 변경 | 같은 크기의 파일 교체·권한 변경·파일 추가/누락·link 변경을 거부 |
| 변조 후 receipt만 재생성 | pinned receipt SHA가 다르므로 거부; 검증된 새 입력/receipt를 고정해야 별도 cache에서 빌드 |
| 의도적 compile failure | 이전 binary가 계속 실행되어 원래 출력 유지 |
| 빌드 frontend에 TERM 또는 KILL·worker KILL·IsOnroad 전환 | 실제 Cargo/build script/손자 프로세스만 종료, 정상 runtime 표식·로그 보존, workspace lock 반환 |
| 부팅 대기 기록의 부분 쓰기·다른 config·미검증 후보·동시 실행 | 기존 active pointer·대기 기록·로그 보존, 미검증 세대 활성화 거부 |
| updater용 Git checkout | 실행 소스와 다른 경로·자체 `.git` directory, source store의 object만 공유, dirty checkout 자동 reset 금지 |
| 실제 kernel memory 한도·파일 증가·frontend 부모 종료 | 한도 초과 또는 취소에서 detached 하위 작업 종료, 정상 프로세스·active/log 보존, 실패 기록 유지 |

자원 제한의 별도 집중 검사는 `native-resource-guard.yml`이다. 해당 작업 branch의 push로
실행하며 결과는 다음처럼 읽는다. 실제 root scope 권한 하강, 실제 메모리 할당/파일 증가,
worker/frontend/부모 강제 종료를 CI에서 검사한다. 준비·정리는 자기 job의 cgroup만
다룬다. production scope 준비는 자기 UID의 고정 경로만 허용하며 공유 controller는
변경하지 않는다. [자원 경계와 측정 한계](native-build-resource-guard-600.md)를 함께 확인한다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow native-resource-guard.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
gh run download RUN_ID --repo bin9208/openpilot-rust --dir EVIDENCE_DIRECTORY
```

artifact의 `native-resource-tests.log`와 시도별 `resources.jsonl`을 함께 읽는다.
kernel memory peak, 한도 event와 실패/성공 상태를 확인하고 최종 행 누락은 성공으로
판정하지 않는다. 디스크 값은 filesystem 가용 공간 감소의 관측값이며 hard quota가 아니다.
이 검사는 X64 host kernel의 격리 근거다. comma 3X kernel 검증은 아래 실기 수용에 남아 있다.

native 빌드 worker의 원문 stdout/stderr는 시도별 `build.log`에 보존한다. updater로 전체
출력을 전달해 그 프로세스의 메모리를 늘리지 않는다. 실패 메시지는 경로와 최대64KiB의
원문 tail을 제공하므로 자세한 원인은 해당 파일과 `resources.jsonl`을 함께 확인한다.
큰 synthetic 출력 flood의 CI artifact에는 제한된 tail만 포함하며, 그 검사를 정상 runtime
부팅 성공으로 판정하지 않는다. 실제 배포 전 이 로그 경로와 자원 제한 검사도 통과해야 한다.

자산의 보고 시간·저장 byte 수는 준비 작업의 수치다. HTTP `transferred_bytes`는 본문
크기이며 TLS/header를 포함한 네트워크 전체 사용량이 아니다. host fixture 시간과
메모리·디스크 값을 콤마의 전체 runtime 수치로 확대하지 않는다.

실제 압축 해제 입력을 recipe에 연결할 때는 `openpilot-runtime-inputs`로 tree 목록을
생성하고 `inputs`에서 receipt SHA를 고정한 다음 `input_trees`에 연결한다. 출력은
입력 tree 밖에 저장한다. 이 도구는 원본 자산의 출처를 스스로 증명하지 않는다.
새 receipt를 신뢰할 근거는 고정된 source 또는 hash 검증한 자산과 그 준비 과정이다.
`verified_input_trees`가 비어 있는 보고서는 전체 toolchain/provider를 검증했다는
증거가 아니다. AGNOS 시스템 라이브러리·ABI closure는 별도 대상 검사를 유지한다.

[AGNOS 시스템 SDK 검사](native-system-sdk-599.md)는 image에서 만든 독립 pin과 실제
compiler root `/`를 연결한다. foundation 회귀7개와 genuine의 실제 build 초기/반복,
같은 크기 헤더 변조 거부·이전 실행 파일 보존·전체 선언 범위 복구 검사를 확인한다.
이 새 검사의 성공은 정확 source Actions 결과에서 별도로 기록한다.

[Codec 입력 검사](native-codec-inputs-599.md)는 기존 FFmpeg/libyuv의 원본 wheel pin을 유지하고
내용별 준비 초기/반복, 한 provider 변경 시 무관 prefix 보존, 로컬 receipt 위조 거부와 실제
AGNOS의 두 C/C++ API 실행을 확인한다. 전체 encoder/FFmpeg closure와 기기 수용은 별도다.

전체 runtime의 실제 b555 기준은 [FFmpeg6.1.1 header 검사](native-ffmpeg-headers-599.md)에서
따로 확인한다.140개 원래 header의 초기/반복·위조/누락 거부와 실제 ARM module object를
검사하며, 별도7.1 codec wheel이나 host header로 대체하지 않는다.

[Shared provider component 검사](native-provider-components-599.md)는 원래 foreign provider만
생산·소비하며 전체 runtime/source 재표기를 거부한다. 독립 descriptor pin·왕복/반복0과
실제 ARM의 header/shared ABI·엄격한 ELF·foreign CLI 실행을 확인한다. 공개 발행/HTTPS는 별도다.

2026-10-26 추가한 generation 시작/업데이트 연결은 아직 Actions 검증 전이다.
`--stage-update`는 source-built 후보를 봉인하고 기존 active ID와 config hash에 묶어
`pending.json`을 발행한다. launcher가 boot lock을 잡은 뒤에만 다시 검증·선택하고,
이전 세대 라이브러리가 섞이지 않도록 새 세대 loader 경로로 기존 lock handoff를 쓴다.
protocol 응답은 이 기능의 지원 여부이며, 정상 manager 부팅·모델·기록·업로드가
완료되었다는 시험 결과가 아니다. 최종 전체 AGNOS recipe와 아래 실기 수용은 남아 있다.

## 최종 사용자 기기 수용 시험

정확 소스 `2b896bd0e32764ddc7f19bcda1ef935ac79ec2c7`의 실제 loggerd/logmessaged
ARM6개 비교는 [#608 결과](native-source-stamp-reuse-608.md)에 있다. 문서 변경은 두
출력 fresh/같은 hash, loggerd만 수정하면 무관한 logmessaged fresh/같은 hash,
legacy source stamp 보존을 확인했다. X64와 전체99 runtime/AGNOS/실기 수용은 별도다.

전체 계획의 컴파일 입력 연결 검사는 [native 입력 조합기](native-input-bindings-599.md)의
Actions 절차를 따른다. 실제9개 입력 역할의 pin·13그룹 환경·반복 plan·발행 거부를
검사한다. 실행 자산/모델·전체 build·기기 수용의 준비 완료 판정은 별도로 유지한다.

[실행용 foreign library 검사](native-runtime-companions-599.md)는 네 component의 독립
pin·반복 전송/복사0, 실제 ARM 수치·기하·압축 API와 ELF,13그룹 계획의 runtime Tree
연결을 확인한다. acados 생성 solver 두 개는 별도 필수 준비/회귀가 남는다.

전체 build 목록·현재 Cargo 소스 대조와 입력별 recipe 생성 명령은
[native runtime 계획 안내](native-runtime-plan-599.md)에 있다. 이 도구의
`compiled=false`/`activated=false`는 실제 의미 그대로이며, plan 생성 성공을 전체
runtime 빌드나 사용자 기기 시험 준비 완료로 판정하지 않는다.

사용자가 선택한 최종 수용 장치는 **comma 3X**다. C3X의 실제 AGNOS 및 QCOM/eGPU 경로,
모델·provider 구성을 기록하고 C4 또는 Ubuntu ARM 결과로 그 수용을 대신하지 않는다.

정확 소스의 전체 AGNOS build·seal·업데이트/복구 CI와 후보 준비 후 사용자가 수행한다.
현재 부분 도구를 차량에 설치하는 명령은 제공하지 않는다. 최종 후보의 branch·SHA와
실제 지원 명령은 준비된 후보의 기록으로 고정한다.

1. 기기 종류, AGNOS 버전, 시작 branch/SHA, model/provider/toolchain ID, 남은 공간을
   기록한다. 기존 정상 세대와 로그를 보존하고 주차 상태에서 시작한다.
2. 해당 SHA의 프리빌트 runtime release가 없는 소스로 처음 빌드한다. 전송 byte 수,
   빌드 대상·소요 시간·peak 메모리·디스크 증가량과 source/result manifest를 저장한다.
3. 같은 소스로 반복하고 문서만 변경한 커밋으로 갱신한다. 대형 자산 전송0, 전체
   runtime 재다운로드0, 빌드 대상 fresh, 결과 hash가 동일한지 확인한다.
4. 작은 runtime 수정 후 갱신한다. 변경 대상과 필수 의존 대상만 재빌드했는지 확인한다.
   provider 하나를 갱신할 때도 나머지 model/toolchain 전송0을 확인한다.
5. 준비된 실패 주입 절차로 build 실패·취소·동시 실행·자원 부족·잘못된 자산·전원 중단을
   확인한다. 검증되지 않은 세대로 전환되지 않고 기존 세대와 로그가 보존되어야 한다.
6. 정상 업데이트 후 재부팅해 기본 화면·manager·모델·카메라·녹화를 확인한다. 기존
   업로드 경로로 한 구간을 업로드해 결과 페이지와 실제 로그의 실행 SHA·모델·오류를
   대조한다. 기기별 결과를 분리하고 주행 수용은 사용자가 기록한다.
7. 같은 장치·모델·기능·입력·전원/냉각 조건으로 선정 성능 변경 전후를 비교한다.
   CPU·메모리·지연·프레임 누락과 기능 회귀의 실제 측정값/한계를 남긴다.

이슈 #598의 마지막 조건은 사용자의 실제 자체 빌드·정상 부팅·기록·업로드 수용이다.
후보 준비나 CI 성공으로 대신 체크하지 않는다. 모든 조건과 근거가 기록될 때만 닫는다.
