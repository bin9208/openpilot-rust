# 정상 설치 빌드의 비용과 feature 캐시 분할 — #675

추적: [#675](https://github.com/bin9208/openpilot-rust/issues/675), [#598](https://github.com/bin9208/openpilot-rust/issues/598), [#649](https://github.com/bin9208/openpilot-rust/issues/649).

## 실사용 기준과 측정 범위

2026-11-03 사용자는 기존 콤마 최초 설치·빌드가 보통10분 이내이고 길어도20분이라고 확인했다. 이 기준을 확인하지 않은 채 Rust 전환의 초기 수시간 빌드를 당연한 비용으로 받아들이지 않는다. 실제 기기 최초/무변경/작은변경 시간은 미측정이며, CI 시간을 기기 시간으로 환산하거나 목표를 달성했다고 주장하지 않는다.

이전 source `f5d1972284340438fc6f7c6ab8e8e46831424ff1`의 [실제Genuine41508678434/job125494499198](https://github.com/bin9208/openpilot-rust/actions/runs/41508678434/job/125494499198)에서 초기 generation은 약2시간56분이다. 동일 초기 구간 Cargo Finished13회 합계는2355초(39분15초)이며, 이는 Cargo가 보고한 경과시간으로 컴파일러 CPU시간이 아니다. 전체 초기 생성에는 외부 입력 검증·준비·봉인 등이 추가된다.

같은 artifact13196795450의157650바이트 `native-complete-generation-evidence/prepared.json` SHA256은 `a495cebab35467f0697a02e3265b7280c8292ac6dab97026c889616abc8965e1`이다.13개 build group의 실제 cache key/target directory는11개다. 이 사실만으로 공통 dependency의 중복 컴파일 횟수나 절감 시간을 단정하지 않는다.

## 최소 수정

기존 native cache identity는 package/bin 선택을 지우지만 feature 선택은 포함했다. 같은 ABI·환경·입력에서도 feature 선택만 다르면 별도 target directory를 만들어 공통 dependency를 재사용하지 못할 수 있다.

feature 선택은 Cargo의 기존 package/source/feature fingerprint가 처리하게 한다. cache identity에서 선택 목록만 지우고 실제 `--features`와 Cargo metadata·바이너리 동작은 유지한다. compiler/SDK/native tool/입력·환경 변화에 대한 기존 cache 분리와 모든 변경 거부 검사는 유지한다.11개 target을 검토 없이 하나로 합치거나 새 캐시 서비스·빌드 엔진을 추가하지 않는다.

기존 두 실제 Cargo 그룹의 공통 dependency 회귀에 서로 다른 feature 선택을 추가한다. 공통 build-script1회와 rlib hash/mtime 유지, feature별 실제 실행, 공통 source 변경 뒤 두 consumer 재빌드 및 원래 compiler/provider 변화 검사를 확인한다. 기존 feature별 metadata/실제 실행 비교에도 같은 cache/target 사용을 확인한다.

## 검증 단계

로컬은 Rust 서식·diff만 확인한다. 기능 검사는 기존 Runtime source build foundation의 해당 crate/ARM·X64 검사로 확인하고, 이 수정마다 전체99개Genuine을 새로 돌리지 않는다. source를 수정 묶음으로 고정한 뒤 PR 필수 검사, 최종 설치 후보의 전체 검증 순서로 진행한다. 현재 실행 중인 d33c8ba의 전체Genuine과 다른 작업·캐시·기기 상태는 보존한다.

이 수정은 feature 선택에 의한 추가 분할만 다룬다. 그룹별 외부 입력 검증·도구 선행 빌드·모델 준비의 남은 비용과 실제 정상 설치 빌드 시간은 별도 확인이 필요하다. 최신 SHA와 실제 검사 결과는 #675 작업 기록에 남긴다.
