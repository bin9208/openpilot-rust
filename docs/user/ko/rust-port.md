# 실험용 Rust 포팅

이 독립 저장소에서는 오픈파일럿 런타임과 개발 도구를 Rust로 전환하고 있습니다.
저장소 전체의 Python 제거는 아직 진행 중이며, 검증 결과와 남은 작업은
[#296 검증 기록](../../naver/rust-repository-python-retirement-296-evidence.md)에 정리합니다.

Rust 1.94.0이 설치된 Linux 개발 호스트에서 `rust/` 디렉터리로 이동해 실행합니다.

```sh
cargo run --release --locked --bin cpu-sample -- 1000
```

선택 인자는 측정 간격이며 1~60000밀리초, 기본값은 1000입니다. TSV로 출력합니다.
100%는 논리 코어 하나 기준이므로 멀티스레드 프로세스는 100%를 넘을 수 있습니다.
processor 열은 마지막으로 관찰된 메인 스레드의 코어이며, 프로세스 affinity나 모든
스레드의 배치가 아닙니다. 읽을 수 없거나 종료된 프로세스와 유효하지 않은 카운터
차이는 제외합니다. 짧은 측정 간격에서는 시간 카운터의 해상도 한계가 큽니다.

이 도구는 한 번 측정하는 진단 도구이며 상주 서비스가 아닙니다. 차량 명령을 발행하지
않습니다. 호스트 테스트와 일반 aarch64 빌드 성공은 AGNOS 호환성, CPU 절감, 온도 개선,
차량 안전 검증을 뜻하지 않습니다. 일반 CI 빌드를 차량에 설치하지 마세요. 대상 기기
검증은 아직 남아 있습니다.

teleoprtc 릴리스 스크립트의 버전 계산도 Rust 도구를 사용합니다. 저장소 루트에서
`cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features teleop-version-native --bin teleop-version`으로 준비합니다.
다른 위치에 설치했다면 `TELEOP_VERSION_BIN`으로 실행 파일을 지정합니다.
`project.version`은 문자열이어야 하며, 기존 브랜치 검사·버전 정렬·커밋·태그 동작은 유지됩니다.

CI의 Params 경로와 JSON11 receipt 경로를 읽는 도구는 `ci-receipt-metadata`입니다.
`cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --features ci-receipt-metadata-native --bin ci-receipt-metadata`으로 빌드합니다.
`module ENV FILE`, `first-binding ENV DIRECTORY`, `prefix FILE`을 지원합니다.
receipt는 최대 4MiB이며, JSON11의 `prefix`는 문자열이어야 합니다. CI는 각 호스트용
실행 파일을 먼저 빌드하고 `CI_RECEIPT_METADATA`로 경로를 전달합니다.

실험용 `offline-cached-code`는 x86-64 Linux에서 원본 CPU 프로그램의 전체 실행 코드를
묶고 검사하는 도구입니다. 빌드 명령은
`cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-model-runtime --features cached-program-isa --bin offline-cached-code`입니다.
명시한 요청 파일의 경로와 SHA256으로 원본·현재 소스 및 발행 도구의 검증 자료를
연결합니다. 모델 소스를 컴파일하거나 해당 실행 코드를 실행하지 않으며, 출력 경로는
새 파일이어야 합니다. 이 후보의 실제 수치 일치 검증은 아직 남아 있습니다.

캐시 실행 코드가 포함된 번들은 `model-run --trusted-bundle BUNDLE SEQUENCE.json` 또는
`openpilot-dmonitoringmodeld --trusted-catalog CATALOG`에
`--cached-admission PATH --cached-admission-sha256 SHA256`을 함께 지정해야 합니다.
이 검증 자료는 번들과 별개로 신뢰할 수 있는 원본·현재 증거에서 받아야 합니다.
일반 로드는 캐시 번들을 거부하며, 이 옵션은 차량 설치나 정상 런타임 선택을 바꾸지 않습니다.

개발용 로그 업로드 검사는 `check-uploader-reference`, `check-uploader-logging`,
`check-uploader-siblings`, `check-manager-log-upload`, `check-runtime-log-upload`을
사용합니다. 기존 Python 제어기 10개는 제거했으며, 유지 중인 CI가 필요한 Rust 기능과
실행 파일을 먼저 빌드합니다. 이 전환의 최종 소스 부재 검증은 #296에서 추적합니다.
소스가 없는 상태의 호스트 녹화·업로드·종료 검증은 통과했으며, 전체 전환과 실기 확인은
아직 남아 있습니다.

개발용 로그 보존 검사는 `check-deleter`의 `reference`, `daemon`, `observer`
범위를 사용합니다. 빌드 명령은
`cargo build --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --no-default-features --features deleter-check-native --bin check-deleter`입니다.
유지 중인 CI가 고정된 원본 검증 자료와 네이티브 실행 파일을 전달하며, daemon 검사는
CI의 비밀번호 없는 sudo로 임시 마운트 공간을 만듭니다. Python 개발 보조 도구 4개를
제거한 상태의 기능·소스 부재·호출 정책 검증이 통과했습니다. 전체 전환은 #296에서
계속 진행합니다.

Linux용 `check-native-logging` 후보는 Rust 로거와 원본 C++ 생산자의 패킷 바이트,
콘솔 출력, 전송 실패 동작을 비교합니다. `openpilot-dev-tools`에서
`--features native-logging-check-native --bin check-native-logging`으로 빌드합니다.
네이티브 기능과 소스 부재 검증을 통과할 때까지 기존 Python 비교 도구는 유지합니다.
