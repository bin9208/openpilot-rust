# 당근파일럿 원본 설치·빌드 흐름으로 정리 — #598 / #675

## 2026-11-05 USB 연결 실기 SDK 권한 회귀

사용자가 이 PC에서 연결된 기기의 자체 빌드 실패를 직접 수정·검증하도록 승인했다.
`c79b2ab7b1ddfe5abe2b85cc9ed8916a395f3fd7`을 AGNOS `19.8-carrot-bt1`,
커널 `4.9.103`, aarch64/tizi에서 시작했다. 디스크·메모리·구형 cgroup 검사는
통과했으나 SDK capture가 `usr/lib/aarch64-linux-gnu/utempter/utempter`를
set-id 파일로 거부해 도구 컴파일 전 종료됐다.

기기 파일은 67,512 bytes, mode `2755`이며 SHA256은
`8e56d3a7c2226b21829ee69b8168d10e8bd5876c3418eb199fe191714fc6eaf2`다.
독립 factory SDK receipt의 같은 파일은 bytes/SHA256이 일치하고 mode만 `0755`다.
읽기 전용 system SDK receipt는 파일을 복사하거나 권한을 부여하지 않으므로
일반 접근 권한 `0777`을 비교한다. payload/hash/크기/경로/namespace 검증은 유지하고
추출되는 일반 input Tree와 실행 도구의 set-id 거부는 변경하지 않는다.

집중 회귀는 setuid/setgid 비트만 다른 live SDK의 일치, 실제 파일 권한 불변,
일반 input Tree의 계속된 거부, 일반 접근 권한·내용 변경의 계속된 실패를 확인한다.
새 seed의 ARM 생성·pin 발행과 해당 HEAD의 실기 빌드·시작은 후속 증거가 필요하다.

## 2026-11-05 기존 설치의 미추적 파일로 시작이 막힌 회귀

사용자는 `dev / 0f462ccc`를 Git pull한 뒤 재부팅했으나
`native source driver requires the exact clean repository root`에서 중단됐다고 보고했다.
기존 생성 파일·패키지2207개가 남았다는 사용자 보고다. 이 설치 경로를 빠뜨렸으므로
#598/#599를 다시 열었다. 이전 CI 성공은 이 조건의 기기 부팅 성공을 뜻하지 않는다.

재현 source `80ae2ac9922721069eb2be912b175cb9504062c8`의
[Actions42005313089](https://github.com/bin9208/openpilot-rust/actions/runs/42005313089)에서
ARM/X64 모두 같은2207개 조건으로 기존 오류가 발생했다. 다른34개 entry 사례는 성공했다.

수정은 설치 폴더의 미추적 파일을 삭제하거나 ignore 목록에 추가하지 않는다. 기존
updater 저장소 아래의 재사용 가능한 Git 체크아웃에 커밋된 파일만 전달하고, 원래 설치
폴더와 패키지·로그는 보존한다. 같은 커밋은 체크아웃을 재사용하고 이후 Git 변경에서도
바뀌지 않은 소스의 수정 시각을 유지한다. 고정 seed와 native worker는 계속 엄격한
소스 판정을 사용하므로 이전 seed를 재발행하거나 새 runtime release를 요구하지 않는다.
원래 추적 소스가 빌드 중 바뀌는 경우도 계속 중단한다. 새 서비스·수동 정리 단계는 없다.

로컬은 AST·Rustfmt·YAML 구문과 diff만 확인한다. 수동 `installed_checkout_only` 모드는
이 시작 경로의 Python/Git 회귀만 hosted ARM/X64에서 실행하며 runtime 재빌드는 하지 않는다.
push 자동 Fast checks와 PR 필수 검사 정책은 변경하지 않는다. 이후의 같은 커밋 재사용과
다음 Git 업데이트, 실제 native source store의 shallow 수용도 해당 경계에서 확인한다.
정확 SHA의 최신 결과는 #598에 기록한다. 실제 기기의 빌드·시작 수용 전에는 완료로
판단하지 않는다.

2026-11-03 사용자 지시. 기준 원본은 로컬 Git에 보존된
`ajouatom/openpilot:carrot-wip`의 `e8e152dbcfd9574a1791ed12026d1880e87689b3`이다.
원본 `launch_chffrplus.sh`는 기존 Git/overlay 업데이트를 처리하고 필요한 의존성을 준비한
뒤 `openpilot/system/manager/build.py`의 SCons 변경분 빌드를 거쳐 manager를 시작한다.
모델 준비 Python과 기존 native library를 재사용한다.

## 정상 설치 기준

- 사용자는 기존과 같이 브랜치를 설치·업데이트하고 재시작한다. 별도 서비스나 사전
  준비 명령을 요구하지 않는다. 필요한 고정 입력은 브랜치의 로컬 입력으로 제공한다.
- 현재 커밋 전체 runtime release의 존재를 정상 시작 조건으로 삼지 않는다.
- 기존 Cargo 의존성 계산으로 변경분을 빌드한다. 문서·HEAD 변경·재부팅만으로
  캐시를 비우지 않으며, 공통 schema 변경은 실제 소비 대상에 전파한다.
- 실패 시 정상 실행과 로그를 보존한다. 실제 source/입력/출력 및 ABI 조건은 유지하되
  빌드 그룹마다 동일 SDK와 전체 도구 트리를 반복해서 읽지 않는다.
- 최초·무변경·문서 변경·작은 소스 변경의 총시간과 준비/빌드/검증 비용을 각각 기록한다.
  사용자의 기존 최초 설치 기준은 보통 10분 이내, 길어도 20분이다. CI 시간을 콤마 시간으로
  환산하지 않는다. 실제 기기 수용은 #528에서 사용자가 확인한다.

## 이번 수정 묶음

기존 설치 연결 `f334c82e69ebd80a3edfad054a7a7f2faa0b03f5`와 검증된 JSON buffering·
feature 간 Cargo 캐시 공유 `d047b81d36c5ddc46c1b7646435accc75793694c`를 별도 작업 브랜치에
합쳤다. 다른 작업 트리·실행 중 Actions·PR611/dev/dev-md는 변경하지 않았다.

ARM 정상 launcher에서 기존 `ensure_runtime.sh`의 커밋별 전체 runtime 다운로드로
우회하지 않고 기존 source entry를 선택한다. 입력 누락은 설치 후보의 미완료로 남기며
대형 runtime 다운로드로 감추지 않는다. 로컬 입력과 기본 정책이 아직 완성되지 않은
이 브랜치를 설치 가능한 후보로 안내하지 않는다.

13개 Cargo 그룹이 같은 compiler SDK/toolchain/provider 입력을 빌드 전후에 각각
검증하던 부분을 전체 직렬 빌드의 시작·끝 경계에서 공유 검증하도록 합친다. 입력 pin의
충돌은 첫 Cargo 실행 전에 거부하고, 빌드 중 외부 입력 변경은 성공 결과를 반환하기 전에
거부한다. 단일 그룹 CLI는 원래 전후 검증을 유지한다. 저장된 보고서의 recipe·입력
identity와 출력 roster·hash는 유지한다. 시간 절감을 아직 실측 결과로 주장하지 않는다.

기존 source entry 결과에 입력 해석·seed 준비와 전체 entry 시간을 추가하고,
기존 source runtime 결과에 admission/current tools/runtime inputs/Cargo·seal/final
checks의 경과를 나눈다. 기존 선택 계측은 전체 plan 전후 검증도 기록한다.

원본 `build.py`처럼 컴파일하는 동안 사용 가능한 CPU affinity를 연다. 호출자의 제한된
마스크를 Cargo 자식이 그대로 물려받지 않으며, 성공·실패 모두 원래 마스크로 복구한다.
실제 빌드 마스크를 결과에 기록하고 manager·주행 프로세스의 배치는 변경하지 않는다.
모델 컴파일러의 별도 `model-compiler-build` target 저장소도 기존 `cargo-build`로 합친다.
내용·ABI 캐시 key와 Cargo의 기존 dependency/feature fingerprint는 유지한다.

기존 native 세대가 있다는 이유로 수동 Git 설치·브랜치 변경의 새 checkout을 무시하지
않도록 한다. 기존 부팅 잠금 안에서 깨끗한 새 source를 확인하고 updater가 이미 쓰는
source-entry CLI와 pending 전환을 재사용한다. 현재 source·dirty checkout은 다시
빌드하지 않고, 실패나 성공 exit만으로 검증된 pending 결과를 대신할 수 없다. 기존
정상 세대와 로그는 유지하며 실제 선택 source를 새 checkout으로 위장하지 않는다.

## 전체 검증과 입력 준비의 분리

기존 `runtime-native-environment.yml`의 작업 브랜치 push 트리거를 제거한다. 실행 중인
검사는 취소하지 않는다. 기존 workflow의 수동 `inputs`는 실제 source-bound 입력 준비와
동일 production 입력/인덱스 재사용 검증을 실행하고, `full`은 원래 모델·99개 전체
빌드/봉인/반복 검증까지 실행한다. 전체 명령·pin·deadline·실패 거부는 남긴다.

입력 파일 발행을 위해 전체99개 runtime을 먼저 네 번 빌드·봉인하도록 결합한 조건을
분리한다. 입력 발행은 같은 source의 성공한 실제 job·PR/base/attempt·원본 ZIP digest와
34개 파일/SDK/seed/provider·production 입력 준비/인덱스 각 경우를 계속 검증한다.
`inputs` 결과는 전체 runtime 검증 성공을 주장할 수 없고 실행 runtime은 발행하지 않는다.
원래 `full` 결과를 검증하는 경계도 보존한다. 최종 완전 후보의 전체 검증은 #601에 남긴다.

Actions의 다음 실행 제목에는 `AGNOS 빌드 입력 준비` 또는 `AGNOS 최종 전체 검증`을
표시한다. 이미 시작된 #137의 기존 제목은 바꿀 수 없으며, 그 실행은 입력 전용
`validation=inputs`다. 입력 준비도 걸린 시간을 따로 남기고, 기기의 Git 다운로드·입력
복사/준비·컴파일 시간을 생략해 최초 설치 시간을 줄인 것처럼 표현하지 않는다.

최초·변경 후 시간 측정은 기존 factory source-entry workflow를 재사용한다. 아직 공개
입력 pin이 없는 개발 단계에서는 성공한 실제 ARM producer의 입력 전용 artifact를
전체 ZIP digest와 모든 source/profile/seed/SDK/provider·로컬 입력 파일 SHA로 검증한 뒤
독립 CI Git checkout에 넣는다. 바꾸는 경로는 입력 인덱스와 `third_party/native_inputs/`
뿐이며 runtime 코드와 Cargo cache를 가져오지 않는다. 입력 준비용 공개 발행/PR 통합을
시간 측정의 선행 조건으로 만들지 않으며 어떤 release나 공개 Git 브랜치도 변경하지 않는다.
원래 source와 실제 입력 포함 commit object, producer source/run/attempt, artifact digest를
함께 남긴다. 이 측정용 checkout을 사용자에게 전달한 최종 후보로 표현하지 않는다.

## 이슈 정리

#598의 직접 실행 단위는 #599(설치·시작), #600(변경분·시간), #601(최종 통합·전달)로
정리한다. #675는 #600의 우선 작업이며, CI 환경 사건은 #605, 병목/회귀 사건은 #649
아래로 옮긴다. 각 문제의 원문·SHA·Actions·미해결 조건을 보존하고 미검증 결함은
완료 처리하지 않는다. 개별 사건마다 전체 후보 성공을 복제 요구하지 않는다.

GitHub 실제 관계를 57개 점검해 56개를 재배치했다. #598의 직접 하위는 #599/#600/#601
세 개이며 기존 이슈 상태는 유지했다. 원문과 변경 전/후 API 결과는 이 작업의 비공개
분석 기록에 보존했다.

추가 감사에서 같은 시작 과정의 세대 인계 중 실패한 checkout 빌드를 반복할 수 있는
경우를 막았다. 시도한 source는 그 시작의 launcher 인계에만 전달하고 새로운
`source_entry --launch`에서는 지워서 재시작 후 재시도를 허용한다. 디스크 상태나
사용자 설정으로 저장하지 않는다. 기존 fixture도 실패한 CLI의 중복 실행 거부를 확인한다.

factory 검사 설정은 실제 sudo helper가 요구하는 uid별 `openpilot-native-build-1000`
자식 cgroup을 사용한다. 공통 controller root 자체를 worker 부모로 넘기던 미실행
경로의 설정 오류를 고쳤다. 또한 최초/반복/문서/작은 수정이 동일한 독립 작업 checkout
경로를 쓰도록 정리했다. 원래 읽기 전용 source는 보존하고 Git 준비 시간은 빌드 시간과
따로 남긴다. 서로 다른 Cargo/model compiler 경로를 비교해 변경분 재사용을 잘못
측정하지 않는다.

최초 측정은 실제 `--launch --verify-package`로 빈 캐시에서 시작한다. 반복 빌드 요청과
무변경 시작을 구분하며, 무변경 시작은 새 source build request를 만들지 않아야 한다.
종전 검사처럼 시작 검증을 위해 세 번째 source 빌드를 강제하지 않는다. 문서/작은 소스
변경은 같은 작업 경로에서 정상 builder를 호출한다. `--verify-package`는 manager·장치
동작 전에 반환하므로 이 CI의 시간을 실제 기기 부팅·실기 수용으로 표현하지 않는다.

## 검증과 남은 작업

로컬은 서식·AST/Bash 구문·diff만 확인한다. 실제 기능 검증은 기존
`Runtime source build foundation`의 ARM·X64 집중 검사로 실행한다. 실제 Cargo fixture에서
공유 provider의 pin 충돌과 Cargo build-script에 의한 입력 변조 거부, 이전 실행 보존을
확인한다. 이 fixture의 시간을 전체 runtime이나 콤마 최초 빌드 시간으로 표현하지 않는다.

공통 입력 중복 검증 제거·ARM source-entry 선택 source
`2a08b7fce77e8fe09dc0750529da6d8d7dbe0eab`의 [집중 검사41585308958](https://github.com/bin9208/openpilot-rust/actions/runs/41585308958)는
[X64 job125749295761](https://github.com/bin9208/openpilot-rust/actions/runs/41585308958/job/125749295761)와
[ARM native job125749296160](https://github.com/bin9208/openpilot-rust/actions/runs/41585308958/job/125749296160)가 모두 성공했다.
실제 Cargo 입력 변경/충돌 거부·기존 실행 보존과 ARM shell 경로를 확인했다. 이후의
CPU/새 checkout/캐시 경로/검증 단계 분리 변경은 다음 집중 검사로 별도 확인한다.

후속 source `d02fc7cdd3f458e0233d10fc9d002163bc2f22de`의
[집중41588655110](https://github.com/bin9208/openpilot-rust/actions/runs/41588655110)은 ARM/X64 모두
성공했다. 같은 시작 과정의 실패 중복 방지·새 시작 재시도·고정 경로 측정 source
`607959ca97b5eef4b1b409138a9cf1f74ef82e56`의
[집중41591463895](https://github.com/bin9208/openpilot-rust/actions/runs/41591463895)도
[ARM125768925930](https://github.com/bin9208/openpilot-rust/actions/runs/41591463895/job/125768925930)과
[X64125768925733](https://github.com/bin9208/openpilot-rust/actions/runs/41591463895/job/125768925733)이
성공했다. 실제 CLI 실패/미검증 성공 거부, CPU 마스크 복구, 독립 Git 변경 source와
캐시 재사용 fixture를 포함한다. 이후 실행 제목/작업 절차 문서 변경은 구문·diff만 확인하며
그 때문에 이 기능 검사나 전체 검증을 재시작하지 않는다.

실제 입력만 준비하는 [Genuine #137/41589186195](https://github.com/bin9208/openpilot-rust/actions/runs/41589186195)는
`validation=inputs`로 한 번 요청했다. 모델 생성·전체99개 빌드/봉인/반복은 실행하지 않는
입력 producer다. 이 실행에서 얻을 실제 입력으로 기존 factory source-entry의 최초·변경 후
시간을 측정하며, 현재 측정 완료나 배포 후보 성공을 주장하지 않는다.

전체 입력 공개를 위해 모든 과거 CI 사건을 재실행하거나, 수정마다 전체 Genuine을 새로
시작하지 않는다. 최종 고정 후보의 실제 정상 entry 전체 빌드·변경 후 빌드 시간과 사용자의
콤마 결과는 별도로 필요하다. 현재 선행 입력/Git pin과 기본 정책이 미완료이므로, 시간
수치 없이 10~20분 목표를 충족했다고 보고하거나 #598을 닫지 않는다.

## 2026-11-03 추가 지시와 빌드 내부 시간 구분

사용자는 원본의 Git 수신 → 필요한 변경분 빌드 → 즉시 실행을 다시 요구했다.
이번 설명에서 제시한 기기 경험은 보통 5~15분이며 큰 변경에서는 20~30분이다.
개발용 전체 기능·원본 비교·메모리 검사는 기기 시작의 선행 조건으로 두지 않는다.
정상 실행·녹화·업로드와 주행 수용은 사용자가 확인한다. 이 지시를 기존 실행 파일·
모델·라이브러리의 호환성 확인이나 실패 시 이전 실행 보존을 제거하라는 뜻으로 해석하지 않는다.

현재 production source entry에는 `cargo test`/Clippy/원본 회귀 전체 실행이 없다.
긴 CI 전체 시간을 기기 시작 시간처럼 설명하지 않는다. 기기 빌드에 남은 비용은
도구·입력 준비, 실제 Cargo, 소스/출력 복사, 봉인과 ABI 확인으로 나눠 확인한다.

기존 `cargo_and_seal` 합계만으로는 이 비용을 구분할 수 없으므로 source-build worker에서
기존 `native-build-phase` 진단을 켠다. 기존 Cargo 구간 계측에 소스 보존·작업 경로 준비·
입력 확인·전체 빌드·봉인과 봉인 내부의 복사·모델 검사·manifest·ABI·최종 확인 시간을
추가한다. 진단은 기존 worker `build.log`에만 남기며 저장된 보고서·캐시 key·세대 identity에
넣지 않는다. factory source-entry의 기존 artifact에도 이 파일을 포함한다.

`generation_prepare_and_build`에는 하위 plan/Cargo 진단이,
`generation_seal`에는 하위 `seal_*` 진단이 포함된다. 같은 계층의 구간끼리만 합산한다.
작은 logger 변경 후에는 실제 `fresh`/출력 SHA와 각 Cargo 구간을 최초 실행과 비교하고,
컴파일이 재사용되어도 입력·복사·봉인 비용이 남는지 별도로 확인한다.

Rust 전환의 효과는 빌드 시간과 분리한다. 원본이 이미 네이티브 라이브러리로 처리하는
추론·수치 연산을 Rust로 감싼 사실만으로 개선을 주장하지 않는다. 같은 콤마·모델·기능·
설정에서 프로세스/전체 코어 CPU, 메모리, 주기 지연·프레임 누락·온도를 비교한 근거가
아직 없으므로 현재 전체 Rust 전환의 실기 성능 이득은 미확정이다.

이 추가 계측은 최초/작은 변경 측정을 위한 변경이며 별도 준비 서비스·전체 검증·
자동 배포를 추가하지 않는다. 로컬은 Rust 서식·YAML 구문·diff만 확인하고 기존
`Runtime source build foundation`의 정확 source ARM/X64 집중 검사 결과를 기록한다.

factory 측정 workflow 파일은 작업 브랜치에만 있고 GitHub workflow 목록에 등록되지 않아
`native-source-entry.yml` 조회가 404였다. 기본 브랜치나 보호 규칙을 바꾸지 않고 기존
`Runtime source build foundation`의 명시적 수동 요청에서 같은 factory workflow를 재사용한다.
인증한 producer run·index SHA·작업별 용량/메모리 추정치를 전달한 측정 요청에서는
기존 작은 fixture matrix를 함께 실행하지 않는다. 일반 push/수동 집중 검사는 원래
matrix를 유지한다. 이 연결은 원래 측정 경로를 실행 가능하게 하는 CI 변경이며 기기에
새 준비 단계나 서비스를 추가하지 않는다.

## 2026-11-03 사용자 시험 후보로 범위 축소

사용자는 런타임 prebuilt 설치에서 자체 빌드·업데이트 유지보수를 개선하는 작업에
일주일 이상 소요됐으며, 전체 언어 전환은 점진적으로 진행할 계획이었다고 재확인했다.
전체 회귀·성능 검증을 시험 후보 전달의 선행 조건으로 계속 쌓지 않는다. 기본 빌드·
실행 파일/라이브러리 호환성·실패 시 기존 실행 보존을 확인하고 기기 수용은 사용자가 한다.

입력 전용 CI도 별도 encoder release 빌드·codec 원본 비교를 실행하고, bootstrap 도구를
최초/재사용/문서/작은 변경 네 번 검증하고 있었다. 입력 전용에서는 검증된 encoder prefix와
현재 도구의 최초 준비만 수행하도록 줄였다. 원래 검사와 네 가지 경우는 명시적 `full`에
그대로 남긴다. bootstrap summary는 반복 검사를 실행하지 않았음을 표시하며, 생략한 검사를
통과했다고 주장하지 않는다. 로컬은 AST·diff만 확인했고 이 후속 CI 조정 때문에 새 검사나
producer를 요청하지 않았다. 진행 중인 기존 source의 실행은 보존했다.

계측 코드 source `4736f914e317360fac45604d3f54c32309f64d51`의
[집중41593307923](https://github.com/bin9208/openpilot-rust/actions/runs/41593307923)은 ARM/X64 모두
성공했다. 측정 workflow 연결 source `44c2c4726c27447e680bdd7b025568a303d7f5db`의
[집중41593758428](https://github.com/bin9208/openpilot-rust/actions/runs/41593758428)은 진행 중이다.
최초 계측 source `ec96b2149`의 실패는 CI가 고정한 artifact 목록에 `build.log` 추가를
반영하지 않았기 때문이며, `4736f914e`는 그 한 항목만 명시적으로 허용해 수정했다.

현재 브랜치에는 실제 `rust/native-bootstrap-inputs.json`, `rust/native-machine.json`,
`third_party/native_inputs/` 묶음이 없다. 이것이 직접 설치 가능한 후보를 막고 있으며
전체 검증 성공을 추가로 기다리는 문제와 구분한다. 실제 파일을 제공하기 전에는
설치 가능·최초/작은 변경 시간 측정 완료·기기 성능 개선을 주장하지 않는다.

## 2026-11-03 입력 묶음 실패 수정과 실제 후보 준비

기존 입력 producer `d02fc7cdd3f458e0233d10fc9d002163bc2f22de`의
[41589186195](https://github.com/bin9208/openpilot-rust/actions/runs/41589186195)는 원본 입력 준비를
마친 뒤 `stage_native_input_payload.py`에서 실패했다. 해시 검증한 증거 ZIP의 SHA256은
`a574a3a33fc8edb016dc97678b146fdf504e40c1a98699e71c5839cf1119e49f`이다.
실제 Source-LFS receipt는 raw cache가 아니라 펼쳐진 generation의 SVG/ONNX 경로를
반환했다. 묶음 생성기가 이를 raw asset으로만 해석해 거부한 것이 직접 원인이다.

현재 source·generation identity·멤버 경로가 맞는 Source-LFS 결과만 원래
`source-lfs-assets/sha256/<SHA>/blob`으로 연결하고, raw 파일의 실제 SHA/byte를 확인해
보존하도록 고쳤다. 기존 패키징 fixture도 실제와 같은 펼쳐진 receipt를 사용하고
다른 source/경로를 거부한다. 이 경계 검사는 기존 producer의 깊은 준비 전에 실행한다.

입력 전용은 현재 도구 최초 준비와 직접/committed-index 최초 입력 소비만 수행한다.
재사용·문서 변경 반복은 `full`에 보존하며 보고서는 실제 실행한 경우만 기록한다.
발행/측정 consumer도 입력 전용의 두 기본 경우와 전체 여섯 경우를 구분하고, 입력 전용
결과를 전체 검증 성공으로 인정하지 않는다. 별도 전체 runtime 검증을 요청하지 않는다.

기본 `rust/native-machine.json`을 추가했다. 최초 설치만 현재 Git index의 seed를 고정하고
cgroup v1/v2 경로를 선택하며, 이미 설치한 정책은 보존한다. 작업별 성장량은 보수적
추정이고 실제 기기 측정값이 아니다. 메모리 한도 3 GiB와 정상 실행 여유 128 MiB를
유지하면서 Cargo의 동시 compiler/linker를 2개로 제한한다. 기존 bootstrap의 최초
커널 peak는 3,336,716,288바이트였으므로, 새 기본 한도에서의 실제 성공을 확인해야 한다.
부족한 메모리/공간을 오류 무시로 숨기거나 사용자 로그를 삭제하지 않는다.

로컬에서는 AST·JSON/YAML 구문·Rust 서식·diff만 확인한다. 실제 입력 준비를 다시
요청할 때는 위 수정 source의 `validation=inputs` 한 번을 사용하고, 결과 파일을 후보에
반영한 뒤 최초/작은 변경 시간을 기존 factory source-entry에서 확인한다.

source `a49b7de7cc5ca2bc6f3f466d6c28a673d5dea262`의 실제 입력 준비는
[41657041599](https://github.com/bin9208/openpilot-rust/actions/runs/41657041599)에서 진행한다.
비싼 준비 전에 실제와 같은 LFS receipt를 사용한 기존 패키징 경계 검사를 통과했다.
입력 묶음 자체의 최종 성공이나 기기 빌드 성공을 의미하지 않는다.

factory의 시간 측정도 최초 빌드와 작은 logger 변경 빌드 두 번으로 줄였다. 무변경 시작은
새 빌드 요청이 없는지만 확인한다. 강제로 같은 source를 다시 빌드하거나 문서 변경으로
또 세대를 만드는 구간은 이 기본 측정에서 제외하고, 결과에도 문서 회귀 미실행을 표시한다.
새 기본 설정·실패 보존·캐시 경계의 기존 짧은 SourceEntry 검사는 image를 할당하기 전에
실행한다. 이 기본 측정 때문에 별도 전체 AGNOS 회귀를 요청하지 않는다.

이전 입력 실행 `41589186195`의 보존된 실제 기록을 집계하면 bootstrap 도구 최초는
1,122.044초였고, 재사용/문서/작은 변경 반복은 각각 937.625/936.125/942.558초였다.
이번 입력 경로에서 제외한 세 반복의 worker 합계는 2,816.308초(약 47분)다.
입력 소비의 재사용/문서 반복 네 경우도 합계 933.107초(약 16분)였다. 별도 encoder
runtime 회귀 단계는 891.784초였다. 이는 이미 실행한 GitHub native ARM의 PRoot AGNOS
입력 준비 기록이며, 제거 구간의 합계가 새 실행의 실제 단축 시간이나 콤마 실측을 뜻하지 않는다.
기기 최초/작은 수정의 실제 시간은 factory 측정과 사용자 기기 결과를 따로 남긴다.

같은 기록에서 `build.reports.bootstrap-tools.elapsed_ms`는 최초 157.756초,
재사용 17.450초, 문서 16.207초, 작은 변경 21.183초였다. 이 구간도 Cargo 전후 확인을
포함하므로 순수 compiler 시간은 아니다. 특히 재사용의 worker 937.625초 중 이 빌드
구간은 17.450초이고 13개 산출물 모두 `fresh=true`다. 긴 대기의 대부분을 Rust 재컴파일로
설명할 수 없다. 새 측정은 PRoot를 거치지 않는 factory native ARM에서 준비/빌드/봉인을
분리하며, 이 기록만으로 실제 기기에서 어느 확인을 없애도 된다고 결론 내리지 않는다.

## 2026-11-04 입력 준비의 메모리 중단 수정

`a49b7de7c`의 입력 실행 `41657041599`는 원본 입력 준비 후 bootstrap 최초 도구 빌드에서
3 GiB 제한 이벤트 10회를 기록하고 중단했다. 묶음 생성에는 도달하지 않았다. 직전 관측은
anonymous 1,014,185,984바이트, file cache 2,009,337,856바이트였고 실제 rustc 자식은 두 개였다.
기존 보고서는 `max`/`oom`/`oom_kill`을 합산하므로 이 결과만으로 실제 OOM 여부를 확정할 수 없다.

[커널 v2 문서](https://www.kernel.org/doc/html/latest/admin-guide/cgroup-v2.html#memory-interface-files)에
따라 회수 압력인 `max`와 할당 실패/OOM을 분리한다. 3 GiB hard limit·swap 금지·시스템 여유·
취소와 이전 실행/로그 보존은 유지한다. Linux 4.9 v1에는 누적 OOM 카운터가 없으므로
[원래 커널의 OOM eventfd](https://github.com/torvalds/linux/blob/v4.9/mm/memcontrol.c)를 worker 시작 전에
구독하고 알림을 누적한다. 이 변경으로 v1의 실제 기기 실행을 검증했다고 주장하지 않는다.

기존 bounded kernel 검사에 256 MiB 제한에서 512 MiB의 깨끗한 파일 캐시를 회수하며 완료하는
경우를 추가한다. 기존 실제 anonymous OOM·중단·프로세스 정리·정상 실행 보존도 유지한다.
이 집중 결과를 먼저 확인하며, 실패한 입력 실행 전체를 원인 수정 없이 다시 요청하지 않는다.

수정 source `004f42d6e65c86aa18b8faea4c755495534a33db`의
[커널 집중41670338618](https://github.com/bin9208/openpilot-rust/actions/runs/41670338618)은 성공했다.
cache-reclaim은 1.018초, 제한 회수 이벤트 1,331회, OOM 0, peak 256 MiB로 완료했다.
실제 anonymous OOM은 0.348초에 OOM 이벤트 5를 기록하고 실패 처리했으며 기존 정상 실행과
로그를 보존했다. 이는 X64 실제 host kernel 검사다. 같은 source로
[입력41670609725](https://github.com/bin9208/openpilot-rust/actions/runs/41670609725)를 `inputs`로
한 번 요청했고, ARM 입력 묶음 및 최초/작은 수정 runtime 시간은 아직 미완료다.

## 2026-11-04 실제 Git 입력과 빌드 시간 실행

[입력41670609725](https://github.com/bin9208/openpilot-rust/actions/runs/41670609725)는 성공했다.
bootstrap은 최초 한 경우, production 입력은 `initial`/`git-index-initial` 두 경우를 실행했다.
bootstrap 최초는 1,201.605초, private cgroup peak는 3 GiB였다. production 입력의 두 worker는
각각 596.401/490.356초, OOM 0으로 완료했다. 이 값은 PRoot AGNOS 입력 준비이며 기기 빌드 시간이 아니다.

실제로 소비한 13개 자산 범주의 Git 입력 537개, 1,266,096,767바이트를 후보에 넣는다.
`rust/native-bootstrap-inputs.json`과 `third_party/native_inputs/`를 포함하며 index SHA256은
`592ae732f05f28056cab6847aef0d4a72e5f07035e2504265557c99b56563a19`다.
producer source는 `004f42d6e65c86aa18b8faea4c755495534a33db`, payload artifact는
`13254948801`, 원본 ZIP SHA256은 `ddaa94d07df3f8e67552240862440c0b963c2e1475ce3f77c4ba7df0fd9cb1d1`이다.
로컬에서는 전송된 ZIP과 작성한 파일의 바이트/해시 및 diff만 확인하며 실행 파일을 실행하지 않았다.

같은 runtime 코드와 실제 입력의 최초·작은 수정 빌드는 기존
[factory ARM41687475482](https://github.com/bin9208/openpilot-rust/actions/runs/41687475482)에서 측정한다.
이 실행은 공개 후보 파일 전송과 병행하여, 인증한 producer 입력만 독립 CI Git checkout에 넣는다.
실제 입력 포함 CI commit object는 `source-entry-authenticated-inputs/proof.json`에 남긴다.
이 CI checkout이나 생성 runtime을 사용자에게 설치한 후보로 표현하지 않는다. 사용자는 실제 입력이
커밋된 Git 후보를 받고 자신의 기기에서 빌드한다. 실행 결과·정확 source와 준비/컴파일/봉인 시간은
해당 실행의 `state/*-proof.json`/`build.log` 및 [#598](https://github.com/bin9208/openpilot-rust/issues/598)에 기록한다.
실제 기기 정상 시작·녹화·업로드와 Git 수신부터의 시간은 #528의 사용자 결과로 구분한다.

위 최초 측정의 기존 checkout은 전체 Git 이력을 받도록 되어 있었다. 이후 factory 요청은 기존
설치와 같은 `fetch-depth: 1`을 사용한다. 성공한 입력 producer도 기본 shallow checkout으로
실제 bootstrap/source 보관을 수행했다. 진행 중인 factory 실행은 보존하고, 이 CI 전송 설정 변경이나
결과 기록 때문에 같은 코드의 최초/작은 수정 빌드를 추가로 반복하지 않는다.

위 factory 요청은 29개 짧은 SourceEntry 검사를 통과한 뒤 Actions ZIP 다운로드에서
HTTP415로 멈췄다. release asset용 `Accept: application/octet-stream`을 Actions artifact
endpoint에도 보내던 전송 코드가 원인이며 image 준비·runtime 빌드는 시작하지 않았다.
Actions ZIP은 기본 JSON API 요청의 인증된 redirect를 사용하고 release asset의 binary
요청은 보존한다. 바이트 수·원본 ZIP SHA·source/profile 검사는 그대로 유지한다.
입력 producer를 반복하지 않고 같은 성공 입력으로 factory를 다시 실행한다.

입력 포함 커밋 `4b4996fe5945d462656fa4b2048f583386db6ca3`의 PR 집중 검사는 factory
checkout의 `fetch-depth: 0` 고정값과 변경한 `1`의 차이를 거부했다. 변경한 다운로드 깊이를
고정 검사 허용 항목에도 함께 반영하지 않은 누락이다. `1`만 정확히 확인한 뒤 원래 설정과
나머지 명령·pin·gate·deadline 전체 비교를 유지한다. runtime 코드와 빌드 입력은 바꾸지 않는다.
API 전송 수정 `e5bf9cb877c0182fb1301efeda914dc7556e368e`의
[factory41690428995](https://github.com/bin9208/openpilot-rust/actions/runs/41690428995)를 보존하며,
이 검사 기록 갱신 때문에 동일 코드의 최초/작은 수정 빌드를 다시 요청하지 않는다.

실제 index를 모든 Git 후보에 포함한 뒤에는 그 파일의 존재만으로 X64 개발 호스트까지
기기 경로로 보내면 안 된다. ARM 자동 선택과 기존 machine 설정/명시적 선택을 유지하고,
중복된 index 존재 조건을 제거한다. 기존 시작 fixture에서 index가 있는 ARM과 X64를
함께 확인한다. ARM의 실제 source build 동작과 고정 입력은 바꾸지 않는다.

`factory41690428995`는 인증 입력과 image 준비를 마쳤지만 runtime 빌드 전에 멈췄다.
CI가 원본 checkout 전체를 읽기 전용으로 bind한 상태에서 `git status`의 LFS clean filter가
`.git/lfs/tmp`를 쓰려 한 것이 원인이다. 추적한 소스는 읽기 전용으로 유지하고 `.git`만
기존 checkout처럼 쓰기 가능하게 bind한다. source commit/tree와 전후 clean 검사는 유지한다.
실패한 실행에는 최초/작은 수정 빌드 측정값이 없으며 같은 입력 producer를 재사용한다.

실제 입력 포함 PR의 Rust 검사는 MPC 계약의 넓은 `third_party` 범위에 새 입력 저장소까지
포함해 실패했다. 저장소 경로 `third_party/native_inputs`만 생성 소스 선택에서 제외한다.
기존 생성 입력 4,662개와 SHA256 `298082a7f6b0a246c82e261a26a98687f52830eea255f6a5a0c9f5563990ca77`는
정확히 일치하며 solver/계약/pin/출력은 바꾸지 않는다. 해당 저장 입력은 기존 독립 SHA/byte로
확인한다. 기존 Git 계약 회귀에 저장 입력 추가 후 재사용 및 인접한 실제 생성 소스 추가 거부를
포함한다. 수정 전 PR 실패는 `41689525912/job126104152913`에 보존한다.

고정 입력을 Git에 추가한 뒤 내비게이션의 5분 작업 `41693601357/job126118212443`가
검사 실행 전 checkout fetch에서 시간 제한에 걸렸다. 입력을 소비하지 않는 여섯 Naver
소스 회귀 job에서만 `third_party/native_inputs/`를 제외하는 partial/sparse checkout을
사용한다. 기존 검사 명령·runner·실패 조건·제한 시간은 유지한다. 실제 기기 및 factory
빌드에는 입력 전체를 포함한다. 이 CI 준비 수정 때문에 같은 runtime 시간 계측을 반복하지 않는다.

`41693132733`은 Git LFS 경계를 통과했지만 첫 CLI import에서 static image `/tmp`에
uid1000 쓰기 권한이 없어 멈췄다. 정상 AGNOS 부팅의 임시 mount가 없는 CI chroot 문제다.
이 작업 전용 디스크 임시 디렉터리를 mode1777로 `/tmp`에 bind하며 메모리 tmpfs나 host의
공용 `/tmp`는 사용하지 않는다. runtime 빌드는 아직 시작하지 않았고 코드/정책/고정 입력은
그대로다. 실패 evidence artifact `13260521492`의 SHA256은
`21c32485241fad5caacd3d04e0ba89b0ffb1291f3903958634bc557fe288947a`다.

기존 `4deb6ca13`의 RadarCAN Miri job `41693601007/job126118211650`도 검사 전에
Git fetch만 20분 걸려 제한에 도달했다. 각 일반 소스 검사에서 고정 입력1.27GB를 반복
전송한 것이 원인이므로 Rust CI와 messaging 비교도 Naver와 같은 sparse checkout을
사용한다. Git tree에는 입력이 계속 포함되고 실제 factory/기기 빌드는 전체 입력을 받는다.
원래 source 비교·명령·runner·제한 시간·gate는 유지한다. messaging의 기존
`cancel-in-progress:true`가 PR 갱신마다 진행 중인 비교를 취소한 사실도 확인했으며
최신 보존 지시에 맞춰 `queue:max`/`cancel-in-progress:false`로 바꾼다. 고정 대조는 이
checkout/concurrency만 정확히 허용하고 나머지 원본 workflow 전체 비교를 유지한다.

최신 PR `605164a5e`의 APK diagnostic job `41697621267/job126151311331`도
검사 실행 전에 전체 Git fetch만15분 걸려 제한에 도달했다. 같은 저장 파일을 소비하지 않는
나머지 일반 PR 작업인 APK·기존 통합 빌드/업데이트 fixture·Jetlink·문서 검사8곳에도
동일한 sparse checkout을 적용한다. 인라인/블록형 원래 checkout 옵션을 모두 보존하고
YAML 중복 키가 없는지 및 그 외 설정 전체의 동일성을 확인한다. 실제 factory/기기 입력은
유지한다. Rust·기기 코드/입력이 같은 진행 중인 `41695138037`의 최초·작은 수정 빌드는
재시작하지 않는다.

이전 phone-mapping의 Maven HTTP403은 최신 `605164a5e` 실행
`41697621267/job126151310999`에서 원래 코드 그대로 성공했다. 별도 제품 수정은 하지 않았다.

## 2026-11-04 최초 실제 전체 빌드 결과와 기존 seed 호환 수정

[factory41695138037](https://github.com/bin9208/openpilot-rust/actions/runs/41695138037),
source `a263729b9d4c79e383dd7bcc8338a7c8a2a0126c`에서 99개 runtime의 빌드와 봉인은
완료했다. generation은 `9359d3b05dfa6a69b8975b699425bceb829ede2fd6f40bc510b506a7ff75d548`이다.
그러나 정상 시작의 기존 고정 seed가 MPC 원본 소스 범위에 `third_party/native_inputs/`도
포함해 `MPC generation inputs differ`로 거부했다. 따라서 실행 성공 후보나 작은 수정 시간은
아직 확보하지 못했으며 #598을 닫지 않는다. 기기 접근·설치·재부팅은 하지 않았다.

| 첫 실행 구간 | 실제 시간 |
| --- | ---: |
| 입력 반입부터 source-entry 결과까지 | 86분 3.689초 |
| 현재 소스의 빌드 도구 준비 | 19분 27.903초 |
| runtime 입력 준비 | 3분 26.439초 |
| 전체 runtime 빌드와 봉인 | 62분 18.157초 |
| 위 구간 안의 Cargo 합계 | 58분 40.388초 |
| 위 구간 안의 봉인 | 2분 3.289초 |

하위 시간은 상위 시간에 포함된다. 실제 ARM 호스트의 AGNOS userspace 결과이며 기기
커널·화면·운전 결과가 아니다. 세 worker의 실제 OOM은 모두 0, 전체 runtime worker의
peak는 3 GiB였다. 외부 통신이 없는 빌드로 Git 전송 시간은 이 수치에 포함하지 않는다.
주된 지연은 이 실행 안의 Cargo release 컴파일·링크이며 전체 기능 검증을 호출한 시간이 아니다.
증거 artifact `13267489781`의 ZIP SHA256은
`30d774473509dba7638b7e04a30413c1e273072445cea9cb600c067e2c2d24cc`다.

실제 입력 536개는 바이트와 Git mode를 그대로 유지해 최상위 `native-inputs/`로 옮긴다.
앞서 넣었던 MPC 소스 선택 예외는 제거한다. 기존 seed와 현재 빌더 모두 원래 4,662개
생성 입력과 계약 SHA를 엄격히 확인하게 하며, 입력 저장소가 모든 기존 생성 소스 범위 밖에
있는지는 이미지 준비 전의 짧은 SourceEntry 검사에서 확인한다. 이미 성공한 producer의
인증된 v1 ZIP 내부 경로는 유지하고 Git 후보를 작성할 때만 새 위치로 변환한다.

다음 실제 측정에서 Cargo 동시 작업 수를 2에서 4로 바꾼다. 3 GiB hard cap·swap 0·실제
OOM 실패 처리와 이전 실행 보존은 유지한다. 속도 개선이나 성공은 새 결과로 판단한다.
압축된 입력 blob에는 Git delta를 사용하지 않으며, CI는 30초 간격으로 현재 worker·빌드
구간·메모리·OOM을 기록한다. 진단 출력은 기존 성공 조건이나 제한 시간을 바꾸지 않는다.

[factory41717022776](https://github.com/bin9208/openpilot-rust/actions/runs/41717022776),
source `769ee8246c6d25b64149c831db765bf61d629d85`는 이미지 전의 원래 seed namespace와
입력 검사를 통과했지만 첫 seed ABI 조회에서 `unresolved or unsupported native seed dependency`로
거부됐다. worker나 runtime Cargo는 시작하지 않았다. 기존 부팅 도구/ABI pin은 이전 실제
빌드와 같으며, 현재 오류는 거부한 loader 출력 행을 남기지 않아 상세 원인은 아직 미확인이다.
알 수 없는 행의 거부와 ABI 비교를 유지하면서 loader 출력과 해당 행을 각각 최대8KiB로
오류에 남긴다. 이 진단과 거부 유지 회귀는 기존 seed 검사에 추가하고 실제 factory의 이미지
준비 전에도 실행한다. Git 수신11분5초와 전체 CI16분46초는 runtime 빌드 시간으로 쓰지 않는다.
실패 artifact `13271387545`의 ZIP SHA256은
`4f5326715ad62bd2d783a39838b2e0d407f21ecb3cd7db021b4cfc0c484f64dd`다.
동일 source의 [집중41716594416](https://github.com/bin9208/openpilot-rust/actions/runs/41716594416)는
X64 성공, ARM은 이미지·기기 없는 기존 회귀를 진행 중이다. 이 결과를 실제 seed ABI 성공으로
대신하지 않는다.

[factory41720296078](https://github.com/bin9208/openpilot-rust/actions/runs/41720296078),
source `b91fdbe5466d5b241f4dfed4cc362f66d877d67f`에서 같은 거부가 재현됐다. 실제 loader
출력 첫 행은 이름 없이 ` (0x0000ff59a4a40000)`이고 뒤에는 기존 vDSO와 liblzma·libgcc·libc·
interpreter 경로가 있었다. GNU 문서는 메인 실행 파일의 `link_map.l_name`이 빈 문자열임을
설명한다([GNU 동적 linker 문서](https://www.sourceware.org/glibc/manual/2.41/html_node/Dynamic-Linker-Introspection.html)).
이 빈 메인 map 형식에 해당하는 것으로 판단해, 정확히 한 개의 맨 앞 64-bit 주소 행만
허용한다. 알 수 없는 이름·누락 라이브러리·중간/중복 주소 행·깨진 형식은 계속 거부한다.
모든 이름 있는 라이브러리 경로와 파일 hash/size/mode, loader 및 도구 pin 비교는 그대로다.
실제 출력 형태와 기존 형태가 동일한 ABI pin을 만드는 회귀를 추가했다. 실제 native 시작
성공은 후속 factory 결과로 판단한다. 실패 artifact `13271792094`의 ZIP SHA256은
`acbd1584e3497e8f114bd0db8c69245d83a2bfb07537ef4da60456322032563b`다.

## 2026-11-04 기본 후보 실제 성공과 디버그 seed 병목 — #676

source `3092dd5cd01478a694e1726781e924290b5b258e`의
[factory41721939609](https://github.com/bin9208/openpilot-rust/actions/runs/41721939609)는 성공했다.
99개 전체 빌드·봉인, 실제 고정 seed의 정상 launcher/package 확인, 새 빌드 요청 없는
무변경 시작, 같은 경로의 작은 logger 수정과 실제 sealed logger 실행까지 완료했다.
원래 generation은 `5bae5976cbc880bb1a7d41a07b5fb71a20202d5f851333bad69918fe933dc77f`,
작은 수정의 CI 파생 source는 `e2e5dec6140b6e7d525aac385ec5f60d05695de1`이다.

| 실제 CLI 구간 | ARM CI 시간 |
| --- | ---: |
| 최초 source 빌드와 package 확인 | 66분 14.090초 |
| 무변경 package 확인·시작, 새 빌드 요청 0 | 7분 17.641초 |
| 작은 logger 수정의 source 빌드 | 21분 48.985초 |
| 작은 수정 중 current_tools 준비 | 15분 36.665초 |
| 작은 수정 중 runtime 입력 준비 | 1분 49.243초 |
| 작은 수정 중 Cargo+봉인 | 4분 5.300초 |
| 위 구간 중 실제 Cargo 합계 | 32.308초 |
| 위 구간 중 봉인 | 2분 20.458초 |

하위 구간은 상위 시간에 포함된다. 최초 source-entry 내부 시간은58분55.883초이고,
패키지 확인/시작을 포함한 CLI 총시간과 다르다. 작은 수정은07/08/09 세 그룹의 logger·
bootlog·manager만 재빌드했고 나머지96개는 fresh였다. 실제 ELF 변경은 logger/bootlog이며
manager는 재링크 후 동일한 바이트일 수 있다. bootstrap13개 도구도 모두 fresh였고 Cargo는
0.26초였다. 모든 worker 실제 OOM0, peak3GiB, 무변경과 작은 수정에서 원본 HTTP 전송0이다.
이 수치는 기기 시간이 아니다. 실제 manager·모델·화면·녹화·업로드·주행은 실행하지 않았다.

증거 artifact `13278364440`의 ZIP SHA256은
`e012d5b623ab89f43894e172920a6513a5e3923bf9d5f515729ec6dae5534b18`이다.
최종 `source-entry-full/result.json`의 cleanup_verified는true이고 전역 controller/binfmt를
바꾸지 않았다. 중간 execution-summary의 cleanup 전 false를 최종 결과로 해석하지 않는다.

실제 남은 병목은 재컴파일 대신 bootstrap 준비/검사에 집중됐다. 생성 코드에서 설치 seed가
`project-target/debug/openpilot-runtime-inputs`인 것을 확인했다. [#676](https://github.com/bin9208/openpilot-rust/issues/676)에서
별도 runtime 전체 묶음을 만들지 않고 기존 bootstrap 경계로 release seed만 내보낸다.
나머지 모델·SDK·Rust/Cargo 원본 입력은 재사용한다. 향후 producer도 release 컴파일 증거의
opt_level3/debug_assertions=false/test=false와 실제 executable을 확인해 같은 혼입을 막는다.
현재 기본 후보3092는 통과했지만 개선된 seed의 시간과 최종 PR/병합 후 결과는 아직 남았다.

[release seed41739898126](https://github.com/bin9208/openpilot-rust/actions/runs/41739898126)는
source `532a028e75baadd11bdca549595d6c756d8e9a35`의 실제 ARM/AGNOS userspace에서 성공했다.
기존 production bootstrap 경계와3GiB/swap0 안에서 현재13개 도구만 release로 만들었으며
전체99개 runtime·manager·하드웨어는 실행하지 않았다. 최종 cleanup_verified=true다.
앞선41739682269는 이미지 준비 전 소유 symlink 거부 회귀에서 O_NOFOLLOW의
`OSError(ELOOP)`를 예상 예외에 넣지 않아 실패했다. 링크는 실제로 거부됐고532에서
테스트의 OS 예외만 반영했다. ABI나 링크 거부 조건을 완화한 수정은 아니다.

artifact13281757975의 ZIP1,806,458바이트와 SHA256
`1d95b73856a55cae64b3679fef375d450722180cb872149500880c721201627c`를 확인했다.
새 seed descriptor는 `ac2d5ff89e51390724f4af0fa05a7e841e85fc0ea2839ace4a5f7c5b4b91edb6`,
helper는3,686,368바이트/SHA256
`2c69b756ea7b4b8980a69194f6aa359414d432a65b1654c197d4d1cebca71a06`다.
이는 앞선3092 실제 빌드의 current-tools release helper와 같은 바이트다. 원래 AGNOS ABI,
프로토콜1/2/1과 라이선스를 유지하고 이전 독립 boot_seed 설정용 파일도 남긴다.
바뀐 기본 index는 `bc2c62693b57842d99a3f42469239296002f3b7b61f95f1000204f3409929d61`다.
모델·SDK·원본 archive 및 나머지16개 descriptor pin은 그대로이며 release는 발행하지 않는다.
최초/무변경/작은 수정의 고정 측정 source는 `27e735027ea24375c2f5db9c6012f06dd5e42d26`,
실행은 [factory41743411210](https://github.com/bin9208/openpilot-rust/actions/runs/41743411210)다.
이 실제 Git index와 metadata를 직접 사용하며 이전 producer로 새 seed를 덮어쓰지 않는다.
확정 시간과 실제 Cargo/준비/봉인 비용, 성공·실패 및 artifact pin은
[#676](https://github.com/bin9208/openpilot-rust/issues/676)에 기록한다.
입력은540파일/1,270,598,981바이트다. 설치·시험 절차는
[현재 시험 안내](native-build-test-guide-598.md)의 첫 절을 따른다.
최종 코드/입력을 PR에 한 번 반영해 정상 필수 검사를 시간 측정과 병행하며,
기본 확인 결과 전에는 PR을 ready나 기기 시험 완료로 표시하지 않는다.


## 2026-11-04 병행 PR의 고정 비교·공간 admission 후속 — #677

최종 코드 head `4299d59e150565aaf01c378af73438369d73ceb6`를 반영해 시간 측정과
정상 PR 검사를 병행했다. [foundation41745907809](https://github.com/bin9208/openpilot-rust/actions/runs/41745907809)의
ARM job126298706231은 고정 원본 비교18개 중17개 성공/1개 실패였다. 실제 index로
factory를 선택하도록 바꾼 workflow에 대해 두 번째 job-condition 기대값만 과거
`inputs.input_producer_run == ''`로 남아 있었다. 이를 `inputs.index_sha256 == ''`로
맞추며 원래 명령·의존성·matrix·도구/소스 pin 비교는 유지한다.

별도 [Rust41745925354 / encoder126298770162](https://github.com/bin9208/openpilot-rust/actions/runs/41745925354/job/126298770162)는
원본 encoder host 빌더의 고정25GiB floor가 남아 거부했다. 실제 free10,515,775,488바이트,
작업 growth536,870,912바이트이며 해당 원본 비교는 실행 전이다.
[#677](https://github.com/bin9208/openpilot-rust/issues/677)에서 이 한 빌더의 floor를64MiB
여유분으로 바꾸고512MiB 증가량과 부족 공간 거부를 유지한다. C++ source 목록·컴파일
argv·3분 timeout·receipt 및 codec/V4L/실제 runtime 비교는 그대로다.
이 helper는 고정 ARM provider 소스가 아니며 기기 runtime 경로에서도 호출하지 않는다.
로컬 AST와 수정 전 파일 전체의 정확한 두 문자열 diff만 확인한다. 기능 결과는 새 Actions로
판단한다. 이 CI 후속은 source27의 기기 코드/입력에 영향을 주지 않으므로 진행 중인
최초/무변경/작은 수정 측정을 다시 시작하지 않는다.

## 2026-11-04 정상 dev 병합과 post 정책 fixture 후속 — #679

PR611의 source `a95a9cff6ef6a8caf6d44065a82ca4f8095e39cf`는 필수 네 context와
실제 ARM/X64 입력·캐시 검사를 통과했다. 변경 없는 lagd cadence의 첫 실패는
[#678](https://github.com/bin9208/openpilot-rust/issues/678)에 증거를 보존했으며,
같은 SHA의 해당 job만 한 번 재실행한 attempt2도 원래 수치·valid·deadline 그대로
성공했다. 이를 원인 해결로 표현하거나 #678을 닫지 않는다.

정상 dev 병합 SHA는 `bb310b26c7c39a4d1136d15f5274ef8633f94cc0`이며 보호 규칙을
우회하지 않았다. 고정 시험 후보27과의 차이는 두 문서 및 host encoder 공간 여유/
고정 workflow 기대값뿐이다. 확정 실측·artifact pin은 #676에 있다. 기기 수용은 #528이다.

[post cache41763870410](https://github.com/bin9208/openpilot-rust/actions/runs/41763870410)의
정책 검사6개 중1개가 실패했다. 네 job을 의도한 expected 집합에 workspace에서 분리한
`rust.yml/recovery-runtime` 한 pair가 빠져 있었다. 실제 영수증은 publisher의 추가 proof
job에도 명시돼 있다. [#679](https://github.com/bin9208/openpilot-rust/issues/679)에서 이
한 pair만 추가하고 시작/정리1회·always 정리·검증 성공 뒤 업로드를 그대로 비교한다.
같은6개 정책 검사를 PR/source foundation의 기존 짧은 전처리 단계에도 추가한다.
원래 명령·runner·matrix·필수 context·ARM/AGNOS 기준이나 기기 runtime/입력은 바꾸지 않는다.
이 후속의 실제 통과와 정상 supporting PR/병합 후 결과는 Actions에서 확인한다.

## 2026-11-05 기존 설치의 빌드 전 공간 정리 — #682

[#682](https://github.com/bin9208/openpilot-rust/issues/682)는 기존 Python 설치와 반복
prebuilt 시험 뒤 남은 패키지·주행 로그 때문에 소스 빌드의 공간 admission에 도달하지
못하는 경우를 다룬다. 사용자가 보고한 여유18.6GiB와 요구24.5GiB는 부족6GiB 정도다.
요구량은 현재 작업의 최대 증가량24GiB+정상 실행 여유0.5GiB이며 이를 낮추지 않는다.
보고된 패키지12개28.8GiB, 주행 로그20.6GiB는 재현 배경이고 정리 조건의 개수·크기로
하드코딩하지 않는다. 아래 정책은 앞선 기록의 로그 자동 삭제 없음보다 최신 지시다.

- `source_entry.py`가 최초 빌드/업데이트의 소스 복사·입력 준비 전에 정리한다.
  입력 복사와 도구 준비가 공간을 소비할 수 있으므로 실제 native admission 직전에도
  같은 정책을 확인한다. 필요한 여유를 확보하면 즉시 멈춘다.
- 원래 installer의 `rust/.runtime-packages`에서 오래된 정규 패키지만 먼저 정리한다.
  installer/source 요청 lock을 지키며 active, 실제 실행 중인 패키지 전체, 최신 이전
  패키지 하나를 남긴다. 설치 journal/rollback 또는 사용 프로세스 조사 실패가 있으면
  패키지 정리를 보류한다. 활성/복구 패키지의 실행 파일·라이브러리는 삭제하지 않는다.
- 그래도 부족할 때만 Params와 같은 data 아래 `media/0/realdata`의 오래된 세그먼트를
  지운다. 최신 세그먼트, `.lock`이 있는 세그먼트, 모든 `user.preserve=1` 세그먼트와
  앞2개는 남긴다. boot/crash, 알 수 없는 디렉터리, symlink는 정리 대상이 아니다.
  과거 업로드 전 로그도 공간 부족 시 정리될 수 있다. Git·Params·별도 영상·입력 묶음과
  변경분 컴파일 캐시는 유지한다. 저장된 IsOnroad가1인 동안 삭제하지 않는다.
- 기존 실행본이 있어 빌드가 필요하지 않은 시작은 공간 부족만으로 주행 로그를 지우지
  않는다. 정리 후에도 부족하면 available/required/short GiB와 보호 파일 보존을 알리고
  실제 빌드를 시작하지 않는다. 기기 접속·직접 정리·재부팅은 하지 않는다.

합성 사례는18.6/24.5 비율의 패키지 우선 정리, live/active/복구 패키지 보존,
Python 설치의 로그 정리·잠금/보존/최신 보호, symlink·다른 캐시 보존, installer
journal/lock/조사 실패, 공간 부족 보고, 충분한 공간·무변경 시작 보존과 실제 main 진입
연결을 다룬다. 로컬은 Python AST/diff만 확인하고 기능 검사는 기존 짧은 ARM/X64
수동 모드에서 수행한다. 정확 SHA/Actions 결과는 #682와 PR #681에 기록한다.
실제 콤마의 확보 용량·빌드 완료·정상 시작은 아직 사용자 확인 전이다.

## 2026-11-05 기기 가용량에 맞춘 작업 메모리 한도 — #683

사용자가 디스크 검사 통과 뒤 current-tools bootstrap에서 다음 중단을 보고했다.
`available_bytes=3219333120`, `requested_bytes=3221225472`,
`normal_reserve_bytes=134217728`, `required_bytes=3355443200`이다.
실제 컴파일/OOM 전의 host admission 거부이며 요구량이136110080bytes 높다.

`build_memory.py`는 기존 machine.json과 그 pin을 수정하지 않고 이번 작업의 private
hard cap만 configured maximum 이하로 정한다. host MemAvailable과 보이는 v1/v2
ancestor headroom 중 작은 값에서 정상 여유와 같은 크기의 시작 변동 여유를 뺀다.
한도를 낮추는 경우64MiB 단위로 내린다. 보고 수치에서는2752MiB(2.6875GiB)의
hard cap,128MiB 정상 여유와 약190MiB 추가 headroom이 된다. 충분한 메모리에서는
기존 cap을 유지하며, 여유 부족·잘못된 MemAvailable·ancestor counter는 계속 거부한다.

기본 JSON만 바꾸면 이미 설치된 정책이 계속3GiB를 요구하므로, source driver가
`memory-admission.json`과 별도 pinned `admitted-config.json`을 기록하고 고정 seed의
최초 current-tools부터 이를 전달한다. 이후 입력/실제 빌드도 같은 축소 정책을 사용한다.
원래 config pin도 전 과정에서 재확인한다. native 메모리·swap·OOM·owner/offroad
검사는 변경하지 않으며, global cache/swap/기기 설정이나 compiler 동시 수는 바꾸지 않는다.
새 seed나99개 runtime 전체 빌드를 이 Python 설정 수정의 전달 선행 조건으로 추가하지 않는다.

집중 사례는 실제 보고 수치의 기존 거부/축소 한도, 충분한 메모리, v1/v2 ancestor와
inactive-file 회수분, 잘못된/부족한 메모리, 실제 source driver의 최초 seed config
전달과 원본 pin 변경 거부를 확인한다. 이 seed 경계 사례는 컴파일러 대신 경계에서
멈추며 실제2.69GiB bootstrap/전체 컴파일 성공을 주장하지 않는다. 정확 SHA의 ARM/X64
결과는 [#683](https://github.com/bin9208/openpilot-rust/issues/683)과 PR #681에 기록한다.
실제 기기의 후속 빌드·정상 시작은 계속 사용자 확인 전이다.

## 2026-11-05 실제 AGNOS4.9의 v2 메모리 인터페이스 — #684

사용자가 제공한 마지막 strace는 private build cgroup 생성과 memory.max 설정 성공 뒤
memory.oom.group 조회가ENOENT로 실패하고 helper가exit1하는 것을 확인했다. 같은 기기의
기존kernelVersion은4.9.103이다. 해당파일과memory.peak를 최신v2와 동일하게 필수로 가정한
실제 시작 호환성 결함이다. 현대host kernel에서 AGNOS userspace를 실행한 CI는 이를 잡지
못했다. 메모리 한도2752MiB 조정은 적용됐고, 이번 중단은 설정 파일이 없어서가 아니다.

v2는 feature가 있을 때 group OOM1을 계속 설정·검증하고, 없는 kernel은 cumulative
memory.events OOM 감시와 기존 supervisor/private-group 전체 정리로 실패를 처리한다.
memory.max hard cap·swap0·host/ancestor 여유와 owner/offroad 검사는 유지한다. 필수
current/events/limit 또는 존재하던 control이 사라지는 경우는 계속 실패한다. optional
interface도 symlink나권한오류를 없는기능으로 취급하지 않는다.

memory.peak가 없으면 kernel_peak_bytes=null로 기록한다. observed_peak_bytes는 실제
current 표본의 최대값이며 관측 사이의 peak를 측정했다고 주장하지 않는다. 최신kernel은
실제kernelpeak를 계속 사용한다.3개 합성Rust사례로 legacy/modern/redirected interfaces와
limits/swap/events/실제통계구분을 확인하고, 기존 v1 OOM 누적 사례도 유지한다. 이 합성
파일은 kernel cgroup으로 실행하지 않는다. native ARM의 기존 export_seed job에서 해당
lib 사례를 실행하고 최초 고정 seed용 release helper를 빌드한다. 실제99개 전체runtime을
다시 빌드하는 작업이 아니다.

[#684](https://github.com/bin9208/openpilot-rust/issues/684)에 정확 소스/Actions/새 seed
SHA·descriptor/index와 설치 후보를 기록한다. 소스만 고치고 기존seed를 계속 받는 상태는
완료가 아니다. 기기 직접 접속·제어·재부팅은 하지 않고, 실제빌드·정상시작은사용자확인전이다.

새 seed 발행은 기존 `native-input-publication.yml`의 명시적 `seed_export=true` 모드에서
수행한다. 원래 전체 입력 발행의 source/PR/SDK/provider gate와 기본 모드는 유지한다.
이 제한된 모드는 성공한 동일 source ARM release export, 새 kernel-interface 사례와
ARM/X64 시작 집중 검사, artifact digest/source/tree/descriptor/helper/license/protocol을
확인하고 해당 source의 bootstrap prerelease4파일만 발행한다. ARM helper를 publisher에서
실행하지 않으며 source 정책·정상 source driver에 포함할 Git overlay만 만든다. 원본
machine의 boot seed용 구 파일은 보존한다. 기존 불변 asset 검증·충돌 보존·draft 원자 발행과
공개 HTTPS 수신 검사를 재사용하며 발행도 Actions에서만 수행한다. 이 결과는 전체 runtime
release나 PR 필수 검사 완료를 뜻하지 않는다.

### #684 새 seed의 확정 결과와 Git 입력

소스af3dd546e718bb0410f9bf3c0bcaca1b49ee233a의
[ARM export42030505128](https://github.com/bin9208/openpilot-rust/actions/runs/42030505128)는
구형/현대 인터페이스Rust4사례와 release bootstrap 생성을 성공했다. 실제 도구 빌드는
217.203초, private hard cap/peak는2752MiB, 실제OOM0이었다. 이는host kernel7.0에서
AGNOS userspace를 실행한 결과이며 물리kernel4.9 부팅 검증은 아니다.

[Actions 발행42034413133](https://github.com/bin9208/openpilot-rust/actions/runs/42034413133)는
artifact/source/파일 검증과 공개HTTPS4파일 수신을 성공했다. 새 helper는3686608bytes,
SHA256`b2fbbcdfd3a0be12de6efdeeb326a1828239fc1e90dc847cab0797213115350a`이다.
새 descriptor는`0d43c39915c19ec80f9ba227add262fec5594fecb0c3189065673d33ed13fd99`,
입력 index는`815c187c2d9b2e53f54c65e67ecedb92b389390de8f9b41b834d33ff2a97bcc7`다.
Git에는 새manifest/helper/license와index를 포함하고, 기존 설치machine이 고정한 구
boot_seed의metadata/seed파일은 보존한다. SDK·provider·모델 입력은 변경하지 않는다.

발행 중 source 영수증의commit/tree형식과 draft조회 누락을 실제 데이터에 맞춰 고쳤고,
기존 불변 파일/소유권·정확source검사는 유지했다. 검증된소스의동일Git태그를 준비한 뒤
Actions가release를생성하므로workflow수정용추가토큰권한은주지않았다. GitHub의tagendpoint에
보이지않는draft는인증된release목록에서찾고중복은거부한다. 해당새mode의작은사례4개도성공했다.
최종Git입력을포함한정확후보의ARM/X64집중검사에서는그입력의실제seed파일도다시읽어
새descriptor/byte/ELF와원본pin의연결을확인한다. 실제기기후속빌드/정상시작은사용자확인전이다.
