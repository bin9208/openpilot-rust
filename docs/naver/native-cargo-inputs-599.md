# 고정 Cargo 의존성 소스 준비 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
기존 driver의 `--locked --offline` 앞에 필요했던 registry 준비를 명시한다.
초기 source helper/bootstrap과 전체 runtime 시작 연결은 아직 별도 작업이다.

## 원본 입력과 범위

`rust/native-cargo-inputs.json`은 현재 `Cargo.lock`의 registry package458개 전부를
고정한다. path package112개와 기존 `rust/vendor` patch는 원래 Git source에 남긴다.
다른 registry/Git source가 새로 생기면 지원하는 것으로 추정하지 않고 실패한다.

원래 lock의 SHA-256, 각 package name/version/checksum, 공식 static.crates.io의
`.crate` URL·원문 크기와 실제 해제 파일 수·payload 크기를 기록한다. 원본 raw archive를
lock checksum과 대조하며 읽기 감사한 합계는 압축66165198 bytes, payload484254905 bytes,
파일26641개다. 이것은 소스 입력의 측정값이며 컴파일 결과나 runtime 메모리 측정이 아니다.
기존 로컬 cache396개는 읽기만 했고, 빠진62개는 소유 scratch에 원문만 보관했다.
압축 해제·의존성 설치·코드 실행·Cargo 실행은 로컬에서 하지 않았다.

[Cargo directory source](https://doc.rust-lang.org/cargo/reference/source-replacement.html)는
원래 registry source와 같은 내용이어야 한다. `.cargo-checksum.json` 자체는 악의적인
동시 변조의 신뢰 근거가 아니므로 이 준비기는 lock의 원본 압축 checksum에서 기대 tree를
매번 다시 유도한다. package payload와 이 파일/Tree receipt를 함께 고쳐도 인증하지 않는다.

## 준비 명령과 내용별 저장

source-built helper가 준비된 개발/검증 환경에서 실행한다. 아래 명령을 기기 설치 안내나
bootstrap 완료로 사용하지 않는다. cache/store/report는 소유한 소스 밖의 경로다.

```sh
openpilot-runtime-inputs --prepare-cargo --plan rust/native-cargo-inputs.json \
  --lock rust/Cargo.lock --cache "$NATIVE_ASSET_CACHE" --store "$NATIVE_CARGO_STORE" \
  > "$CARGO_PREPARATION_REPORT"
```

- 전체 registry tuple set과 현재 lock bytes를 확인한다. 누락·외부 package·중복·다른 lock은
  다운로드 전에 실패한다. raw 자산은 기존 SHA/정확 크기/HTTPS cache와 lock을 재사용한다.
- 검증한 바로 그 bounded raw bytes에서 gzip/tar를 읽는다. crate의 원래 root만 허용하며
  탈출·절대 경로·link·set-id·특수 파일·상충/중복·사전 작성 checksum 파일을 거부한다.
  원래 모든 소스·license를 보존하고 실행0555/data0444로 정리한다. 원문64MiB/package,
  payload512MiB/package·선언된 파일 수/해제 크기·기존 member/receipt 한도를 적용한다.
- crate checksum별 별도 object를 fsync/원자적으로 게시한다. 새 lock set은 변경되지 않은
  immutable 파일을 hard-link하며 가변 Cargo target/registry extraction과 공유하지 않는다.
  같은 입력 재준비는 전송0/copy0/link0이다. crate 하나 변경 시 원문·payload 준비는 그
  객체만 추가하고 다른 원문은 전송0, 기존 입력 묶음은 그대로 보존해야 한다.
- source provenance는 vendor/lock 내용만 따르며 project commit·URL·공간 정책을 compilation
  identity로 바꾸지 않는다. 준비 공간은 원문 크기·새 object의 해제량·목록 추정·선언 reserve로
  계산한다. 자원 부족·동시 owner·잘못된 자산은 기존 정상 runtime/로그를 변경하지 않는다.

`copied_bytes`는 새 payload/checksum/provenance를 쓴 양, `linked_bytes`는 새 snapshot에
연결한 기존 data의 논리 byte 수다. 후자는 물리 data 복사량이 아니다. `elapsed_ms`는 준비
경과 시간이며 이 값들을 전체 빌드 aggregate peak RAM/디스크로 표현하지 않는다.
`assets.*.transferred_bytes`는 자산 CLI의 HTTP body다. Actions cache restore/save 전송은
별도이며 이 값으로 CI 전체 네트워크나 실제 기기의 전송0을 주장하지 않는다.

## 실제 빌드의 필수 입력 연결

report의 root/tree_receipt/tree_sha256을 신뢰한 공통 input set의 `trees`/SHA-pinned `files`에
넣고, `OPENPILOT_CARGO_VENDOR`를 report.vendor로 지정한다. driver는 canonical vendor와
그 부모의 전체 Tree pin, provenance의 schema/format/현재 Cargo.lock SHA를 확인한다.
그 후 source replacement를 명시한 Cargo config 인자로 전달하며 원래 `--locked --offline`
조건과 격리된 mutable Cargo home을 유지한다. 빌드 후 전체 입력 검사도 유지한다.

registry source를 새로 수정하는 대신 원래 프로젝트의 path/patch dependency를 사용한다.
Cargo.lock 변경은 새 입력 묶음을 요구한다. 현재의 set 전체 pin은 lock 변경 시 build cache를
나눌 수 있으며, 이 준비만으로 lock 변경의 최소 재빌드 범위나 성능 개선을 주장하지 않는다.
문서·작은 프로젝트 source 변경은 동일 vendor 입력을 계속 사용할 수 있어야 한다.

## Actions 검증과 남은 조건

foundation은 압축 fixture4개로 재사용/한 crate 변경·같은 inode 재사용/이전 입력 보존,
source와 두 receipt를 함께 변조한 거부, 잘못된 archive/lock/한도, 공간·동시 owner·cache 손상을
검사한다. tracked plan과 실제 lock의 완전성 검사도 있다. 실제 Cargo 회귀는 empty Cargo
home에서 준비된 registry dependency를 빌드해42를 실행하고, 반복·docs-only freshness,
없는 Tree pin/다른 lock/같은 크기 source 변조의 거부와 이전42 실행을 확인한다.

genuine workflow는 host에서 전체 원본을 처음/반복 준비한다. AGNOS user space에는 그
검증된 source seed와 raw cache를 별도로 연결한다. 원래 Rustup·compiler·Cargo 명령/pin을
유지하면서 소유 Cargo config로 directory source와 `net.offline=true`를 지정한다.
실제 source-built ARM inputs CLI가 seed를 검증/재사용하고 같은 tree SHA 및 전송/copy/link0을
확인한다. 이것은 ARM에서 처음부터 새 source object를 쓴 측정으로 표현하지 않는다.
이 입력을 사용하는 실제 전체 cereal schema 예제의 원본 Python wire 비교도 유지한다.

모든 기능·Cargo/Clippy·ARM 실행은 새 정확 source의 Actions 결과 전까지 미검증이다.
`prepare-cargo`는 direct helper의 기본 feature이며 no-default-features인 runtime consumer에
새 압축/TOML 의존성을 추가하지 않는다. 초기 source helper, 전체 OS native SDK/provider/
Python 모델 환경, full99개 build/seal/기본 시작·업데이트/복구·녹화·기존 업로드, 정확 source
PR/병합 후 검사와 사용자 comma3X 수용은 남는다. #598을 준비기만으로 닫지 않는다.
