# 당근파일럿 원본 설치·빌드 흐름으로 정리 — #598 / #675

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
