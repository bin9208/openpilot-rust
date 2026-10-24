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

Carrot Navi CI는 `openpilot-dev-tools`에서
`--no-default-features --features carrot-navi-compare-native --bin check-carrot-navi-comparison`으로
`check-carrot-navi-comparison`을 빌드합니다. 캡처 디렉터리 하나를 인자로 전달하면
기존 정책·소켓·수명주기·디스커버리를 정확히 비교하고 정규화된 행과
`comparison.json`을 기록합니다. 독립 원본 비교기는 해시로 고정한 테스트 자료로
보존합니다. 기존 런타임·QA 트래픽은 CI에서 유지하며, 전체 도구군 제거와 실기
확인은 #296의 미완료 검증입니다.

Linux용 `check-native-logging` 후보는 Rust 로거와 원본 C++ 생산자의 패킷 바이트,
콘솔 출력, 전송 실패 동작을 비교합니다. `openpilot-dev-tools`에서
`--features native-logging-check-native --bin check-native-logging`으로 빌드합니다.
네이티브 기능과 소스 부재 검증을 통과할 때까지 기존 Python 비교 도구는 유지합니다.

GitHub Actions용 새 `check-native-logging retained` 후보는 고정된 비공개 캡처
자료와 새 Rust 생산자의 결과를 비교합니다. 기존 `--root`, `--binary`, `--output`에
`--context`, `--source-commit`, `--binary-sha256`을 전달하며 새 출력 디렉터리가
필요합니다. 이 모드는 원본 생산자를 실행하지 않으며 `--builder`와 `--reference`를
받지 않습니다. 새 테스트 21개와 실제 실행·종료 검증은 대기 중입니다. 저장된 자료의
읽기 검증이나 소스 리뷰만으로 전체 로깅 검증과 Python 제거를 완료한 것은 아닙니다.

Linux용 `check-loggerd-native` 후보는 원본과 Rust 로거의 전체 기록 메시지,
미디어, 오류 처리, 세그먼트 전환 시간을 비교합니다. `openpilot-dev-tools`에서
`--features loggerd-check-native --bin check-loggerd-native`로 빌드합니다.
원본·네이티브 실행 파일과 새 출력 디렉터리가 필요합니다.
기능·종료 검증은 대기 중이며 기존 CI 호출은 유지합니다.

`openpilot-model-runtime`의 `publish-model-catalog`는 전용 임시 디렉터리에
이미 생성된 번들을 게시합니다. 기존 세대의 내용을 확인한 뒤 재사용하고 카탈로그
포인터를 원자적으로 교체합니다. 호스팅 CI에서 집중 테스트 8개와 원본·네이티브
파일시스템 비교 10개를 통과했습니다. 컴파일러·모델 실행과 생성기 호출 전환은
별도의 검증 대상입니다.

유지 중인 빠른 검사는 네이티브 `check-changed-files`로 Python 3.12 소스 문법과
Git 공백 오류를 검사합니다. `openpilot-dev-tools`에서
`--no-default-features --features changed-files-native --bin check-changed-files`로
빌드합니다. 저장소에서 실행하며 `--base SHA` 또는 `BASE_SHA`를 지정할 수 있습니다.
기준값이 없거나 빈 값 또는 모두 0이면 `origin/dev`를 기준으로 이슈 브랜치 전체를
검사합니다. 공백 검사를 먼저 수행한 뒤 변경된 일반 `.py` 파일의 현재 작업 트리
바이트를 파싱하며 심볼릭 링크와 삭제된 파일은 건너뜁니다. 취소 시 소유한 Git
프로세스 그룹을 정리하고, 미지원 소스 기능은 명시적 실패로 처리합니다. 소스 제거는
승인된 인코딩·수명주기 증거와 별도 삭제 검증을 요구합니다. 이 CI 전환은 기기 검증이나
저장소 전체의 Python 제거 완료를 뜻하지 않습니다.

`openpilot-dev-tools`의 `--features developer-ssh-native --bin developer-auth-json`
후보는 `google|apple|github --json` 브라우저 로그인 상태를 처리합니다. 실행하면
브라우저를 열고 성공 시 기존 인증 설정에 토큰을 저장합니다. 실제 인증을 수행하지
않는 합성 회귀 검증은 Actions에서 진행하며, 원본 `auth.py`의 일반/JWT 출력 모드와
기존 호출부는 아직 유지합니다. 이 후보 추가만으로 인증 도구 전체 전환을 인정하지 않습니다.
