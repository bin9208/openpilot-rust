# 실제 runtime의 문서 변경 재사용 — #608 (Actions 검증 대기)

상위 #598/#600. dependency-free fixture의 Cargo fresh만으로 실제 runtime의 문서 변경
재사용을 입증할 수 없었다. `logging/build.rs`와 `loggerd/build.rs`가 Git HEAD/index 및
모든 추적 파일을 감시하고 전체 소스 SHA를 바이너리에 넣으므로, 문서 변경도 공통 logging
및 관련 실행 파일의 재빌드를 유발할 수 있었다. 아래 실제 비용과 재사용 결과는 아직
Actions로 확인하기 전이다.

## 변경 경계

기본 release 경로는 기존 exact-source 감시와 stamp를 유지한다. 명시적으로
`OPENPILOT_NATIVE_INCREMENTAL=1`을 recipe 환경에 넣은 native 경로만 Git 메타데이터와
전체 저장소 파일의 감시를 생략한다. 실제 crate의 src/build.rs/Cargo.toml, loggerd의 version.h
및 Cargo가 추적하는 실제 의존성/feature/입력은 그대로 감시한다. 다른 flag 값은 거부한다.
native generation의 모든 group은 이 모드를 명시적으로 고정해야 하며, 값은 기존 recipe와
cache identity에 포함되므로 기본 모드와 같은 target cache로 혼용하지 않는다.

재사용된 artifact의 내장 source commit/tree는 **실제로 컴파일했던 원래 값**이다. 문서만
바뀐 커밋으로 그 값을 덮어쓰지 않는다. native logging에는 `source_commit_kind=compiled
artifact`, loggerd의 기록에는 같은 의미의 항목을 추가해 compilation provenance임을
표시한다. parent logging context가 이를 덮어쓰지 못하게 한다. 선택한 runtime의 실제
commit은 별도 source generation descriptor/build.json/manager Git metadata에 남으며,
기존 source·compiler·provider·결과 hash와 ABI 검증을 통과해야만 해당 source로 선택된다.
작업 중인 다른 SHA의 runtime을 현재 source 결과로 바꾸는 예외는 추가하지 않는다.

## 실제 crate 검증 방법

`native-source-reuse.yml`은 bin-actions의 X64 및 별도 ARM QEMU/PRoot에서 다음을 수행한다.
로컬에서는 구문·서식·diff/읽기만 수행했다.

1. 원래 logging/loggerd 전체 회귀와 strict Clippy를 유지한다.
2. 해당 Actions source의 job 소유 clone을 만들고 실제 runtime build driver로
   `openpilot-loggerd`와 `openpilot-logmessaged`를 빌드한다. 기본 경로와 다른 native recipe
   identity, 실제 toolchain/native linker 및 각 출력 hash를 기록한다.
3. 같은 입력 반복과 문서만 추가한 source commit에서 두 실행 파일 모두 fresh=true이고
   출력 hash가 같아야 한다. 실제 컴파일 SHA가 원래 값으로 남는지도 확인한다.
4. loggerd의 `--help` 문자열만 바꾼 격리된 source commit을 빌드한다. loggerd의 실제
   help 출력/hash가 바뀌고 logmessaged는 fresh/같은 hash를 유지해야 한다.
5. 기본 모드로 전환해 별도 cache에서 빌드하고 다시 문서를 바꾼다. 기존 exact-source
   stamp 동작에 따라 두 출력이 재빌드되고 새 컴파일 SHA가 남아야 한다.

이 시험은 실제 logger crate의 증분 빌드와 provenance를 다룬다. 전체 AGNOS generation의
설치·부팅·모델·기록·기존 업로드·기기 수용 및 aggregate peak resource 측정은 별도다.
실행 파일은 격리된 CI clone에서만 바꾸며, 원래 source·다른 cache·사용자 로그를 reset/
삭제하지 않는다. 정확 SHA의 결과를 확인하기 전까지 #608과 #598을 닫지 않는다.
