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

## 검증과 남은 작업

로컬은 서식·AST/Bash 구문·diff만 확인한다. 실제 기능 검증은 기존
`Runtime source build foundation`의 ARM·X64 집중 검사로 실행한다. 실제 Cargo fixture에서
공유 provider의 pin 충돌과 Cargo build-script에 의한 입력 변조 거부, 이전 실행 보존을
확인한다. 이 fixture의 시간을 전체 runtime이나 콤마 최초 빌드 시간으로 표현하지 않는다.

전체 입력 공개를 위해 모든 과거 CI 사건을 재실행하거나, 수정마다 전체 Genuine을 새로
시작하지 않는다. 최종 고정 후보의 실제 정상 entry 전체 빌드·변경 후 빌드 시간과 사용자의
콤마 결과는 별도로 필요하다. 현재 선행 입력/Git pin과 기본 정책이 미완료이므로, 시간
수치 없이 10~20분 목표를 충족했다고 보고하거나 #598을 닫지 않는다.
