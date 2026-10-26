# 실험용 Rust 포팅

이 독립 저장소에서는 오픈파일럿 기기 런타임의 Rust 전환을 시작했습니다.
첫 workspace에는 스칼라 필터와 Linux CPU 계측 도구가 있습니다.
아직 차량 프로세스를 대체하거나 추가 AI 모델을 활성화하지 않습니다.

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

### 콤마 자체 빌드 준비 작업 (#598)

모델 준비는 기존 Python 경로를 유지하며, 성능 개선과 전체 런타임 재다운로드 없는
자체 빌드를 우선 개발합니다. 초기 개발 도구 `openpilot-runtime-build`는 준비된
네이티브 Rust 1.94.0과 오프라인 Cargo 의존성으로 지정 대상을 빌드합니다.
현재는 자동 설치·시작에 연결되지 않았으며 완성된 장치 자체 빌드 기능이 아닙니다.
진행과 수용 조건은 [#598](https://github.com/bin9208/openpilot-rust/issues/598)에서 확인합니다.

개발용 `openpilot-runtime-assets --plan JSON --cache DIRECTORY`는 계획에 명시된
HTTPS 자산을 SHA-256·바이트 수로 확인해 내용별 캐시에 저장합니다. 같은 자산을
다시 준비하면 HTTP 전송량은 0이며, provider 하나를 변경하면 그 자산만 받습니다.
보고서의 `transferred_bytes`는 HTTP 본문 바이트 수입니다. 이 도구는 압축 해제나
장치 설치를 수행하지 않습니다. 빌드 도구는 깨끗한 Git checkout을 요구하고 실제
커밋·tree·recipe·compiler·linker·실행 파일 해시를 보고서에 남깁니다.
전체 toolchain/ABI 봉인과 자동 시작 경로 연결은 아직 개발 중입니다.

압축을 푼 toolchain/provider는 `openpilot-runtime-inputs --root INPUT_TREE
--output RECEIPT_JSON`으로 실제 파일 hash·크기·권한·내부 파일 symlink 목록을
기록할 수 있습니다. 이 기록을 독립적으로 확인한 자산 출처에 연결한 뒤, recipe의
`inputs`에 receipt SHA-256을 고정하고 `input_trees`에 tree 경로와 receipt 경로를
지정합니다. 빌드 전후 실제 tree의 변조·추가·누락을 거부합니다. receipt를 새로 만든
사실만으로 입력을 신뢰하지 않으며, 기록이 없는 tree는 검증된 것으로 계산하지 않습니다.
검사별 예상 결과와 최종 기기 수용 절차는 [검증 안내](../../naver/native-build-test-guide-598.md)를 참고하세요.
