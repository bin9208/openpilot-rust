# 모델 준비용 ARM Python 입력 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
내용별 재사용 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
기존 Python 모델 준비 코드를 유지하며 별도 immutable interpreter prefix를 준비한다.
현재 pyproject.toml의 요구는 Python >=3.12.3,<3.13이다.
캐시된 원본 이미지의 python3 링크는 python3.12를 가리키고 SDK header는3.12.3이다.
이는 파일 읽기 감사이며 실제 factory interpreter 버전은 Actions 실행으로 별도 기록한다.
모델 준비 입력의 재현성과 독립적인 내용별 업데이트를 위해 이 단계는 별도 prefix의
정확 버전을 고정한다. factory Python과 OS는 유지한다.

## 고정 입력과 범위

[공식 20261026 release](https://github.com/astral-sh/python-build-standalone/releases/tag/20261026)의
CPython3.12.16 aarch64-unknown-linux-gnu install_only_stripped archive를 선택했다.
공식 문서는 이 형태가 설치 트리만 포함하며 debug symbol을 제거한 배포라고 설명한다.
파일은 python/ 아래에 배치되며 이 준비기는 모든 파일과 notice를 유지한다.
설치 스크립트나 prefix의 Python을 준비 중에 실행하지 않는다.
출처: [배포 형식](https://gregoryszorc.com/docs/python-build-standalone/main/distributions.html).

rust/native-python-toolchain.json의 asset은 다음과 같다.

- 파일: cpython-3.12.16+20261026-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz
- URL: https://github.com/astral-sh/python-build-standalone/releases/download/20261026/cpython-3.12.16%2B20261026-aarch64-unknown-linux-gnu-install_only_stripped.tar.gz
- bytes:29234064
- SHA-256:bb4b6392670c5d3f93876996fcaa19858a4a413a0c7720144c18b0d6f1f0db2d
- [공식 asset metadata](https://api.github.com/repos/astral-sh/python-build-standalone/releases/assets/678574577)

위 크기/digest를 공식 API에서 확인했다. payload 검증·AGNOS 실행 결과는 아직 대기다.
공식 문서는 대부분 GNU Linux target의 최소 glibc를2.17이라고 설명하지만, 이를 실제
AGNOS ABI 성공 증거로 사용하지 않는다. 모든 ELF의 실제 symbol/loader/library closure와
준비된 Python 실행을 별도로 검사한다.
출처: [실행 요구](https://gregoryszorc.com/docs/python-build-standalone/main/running.html).

이 단계는 interpreter이며 NumPy/pycapnp/zstandard 등 모델 wheel, CPU LLVM,
QCOM compiler를 포함한 전체 모델 준비 환경을 완성한 상태가 아니다.
후속 [wheel·factory compiler 준비](native-model-wheel-inputs-599.md)는 실제 두 wheel과
factory LLVM20.1/QCOM의 import·미소 기능 검사를 추가한다. 새 결과는 대기다.
새 Python은 모델 준비 입력으로 결속할 대상이다. 현재 Rust compiler9개 역할이나
runtime10개 Tree에 Python을 실행 runtime으로 끼워 넣지 않는다.
모델 준비·생성물 cache·실제 모델/원본 회귀·전체 native 빌드 연결은 다음 필수 작업이다.

## 내용별 준비와 실패 보존

runtime-build의 prepare-python feature는 기존 tar/flate2 의존성을 사용하며 기본 feature에
포함된다. 별도 mode는 아래와 같다. archive feature 전체 없이 이 feature만 빌드하는
CI도 추가했다. 기존 no-default consumer 검사와 모든 원래 명령은 유지한다.

```sh
openpilot-runtime-inputs --prepare-python --plan rust/native-python-toolchain.json \
  --cache ASSET_CACHE --store TOOLCHAIN_STORE
```

원본 archive는 SHA로 캐시하고 재사용할 때에도 전체 size/hash를 검사한다.
압축 해제기가 실제 관찰한 압축 byte stream의 전체 길이/SHA도 같은 읽기에서 검사한다.
cache 인증 후 파일을 다시 여는 사이의 변경으로 기대 Tree를 바꿀 수 없다.
기대 Tree는 인증한 archive를 읽어서 계산한다. 현재 prefix를 snapshot해 스스로 신뢰하지
않으며 receipt만 믿지 않는다. 압축 해제/파일 hash와 실제 prefix 검증의 반복 I/O는 남아
있다. 검증에 원래 archive cache가 필요하며 이를 제거하면 다시 받아야 한다.

배포 버전/target/release/asset이 content key를 정하고 현재 project SHA는 포함하지 않는다.
작업 증가량 estimate만 변경하면 같은 prefix를 재사용한다. 실제 파일·모드·alias·추가 파일과
기대 receipt hash를 검사하고 반복 HTTP body/저장/복사0을 보고한다.
archive 안의 원래 파일 전체를 유지하고 실행 파일555/일반 파일444로 정규화한다.
Python executable/header/os/encodings와 notice 존재를 요구하며 AArch64 ELF header도 검사한다.

필수 파일 누락, 중복/이탈 경로, dangling/cycle/외부 링크, special file/mode,
다른 크기/hash·위조 receipt·추가 파일·다른 ELF architecture는 오류다.
현재 작은 구현은 regular file/directory/contained file symlink만 지원하고 hardlink나
device node를 허용하지 않는다. 인증한 vendor archive도 이 구조 검사를 통과해야 한다.
잘못된 새 입력이나 손상된 hit를 덮어써 수리하지 않는다. lock 충돌은 오류를 반환한다.
별도 pending directory에서 Tree/ELF를 검사하고 fsync·원자 NOREPLACE로 발행한다.
중단/실패 시 기존 prefix를 바꾸지 않으며 전체 runtime의 active pointer도 변경하지 않는다.

작업별 admission은 최대 압축 해제256MiB와 plan metadata 여유4MiB를 사용한다.
archive가 새로 저장되는 경우29234064 bytes를 더해 최대301863824 bytes를 예상한다.
기존 archive를 쓰는 새 prefix는272629760 bytes, 같은 prefix 재사용은 추가 예상0이다.
이는 보수적 준비 bound이며 실측 최대 디스크 증가량/메모리 peak가 아니다.
개별 파일128MiB, archive member50000개, 경로/link4096 bytes, Tree receipt16MiB,
전체 해제 stream256MiB로 제한한다.
고정25/35GiB floor를 사용하지 않는다.

## CI와 검사 방법

foundation x64/ARM의 다섯 owned gzip fixture 검사는 같은 prefix/inode 재사용, estimate
변경의 동일 key, 바뀐 asset의 별도 세대·이전 데이터 보존, 잘못된 archive/plan/hash,
license/필수 파일 누락·이탈·중복·mode·ELF·link cycle, 위조 receipt/추가 파일/손상된 hit,
lock 충돌과 해제 후 재사용을 확인한다. GNU long-name도 실제 tar parser 경계에서 검사한다.
헤더 모양의 가짜 ELF를 실행하지 않으며 실제 interpreter 성공 증거로 사용하지 않는다.
추가 parser 단위 검사는 앞선 cache 검사에 의존하지 않는 stream 자체의 SHA/길이 검사를
확인한다. 이 검사를 실제 interpreter/모델 실행 성공으로 표현하지 않는다.

genuine host helper는 실제 공식 archive 준비/반복, 독립 Tree hash와 AArch64 header를
확인한다. raw archive cache만 ARM guest에 연결하고 source-built ARM helper로 별도
prefix를 준비한다. host와 같은 기대 Tree/content key 및 반복 전송/복사0을 확인한다.

그 뒤 준비된 Python을 -I -B -S로 실행한다. 실제 version/architecture/prefix/SOABI,
ctypes getpid, bz2/lzma/zlib roundtrip, math/SHA256, in-memory sqlite3 및 SSL import를
확인하고 자신의 maps를 기록한다. 원래 factory Python 버전도 별도 기록한다.
import하지 않은 extension을 포함해 prefix의 모든 실제 ELF를 기존 strict auditor로 검사한다.
실행 후 prefix 전체를 다시 검증해 bytecode 등 추가 파일이 없고 전송/복사0인지 확인한다.
모델/GPU 실행이나 전체 Python environment 성공은 이 probe가 입증하지 않는다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-native-environment.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

정확 SHA의 native-python initial/reused/after-health JSON, summary/Tree receipt,
factory-version/health/maps/ABI JSON과 명령 기록을 확인한다. Python payload 전체를 별도
evidence artifact로 중복 업로드하지 않는다. 실제 기능/Clippy/ABI/실행 결과는 아직 대기다.
로컬은 Rustfmt·Python AST/JSON/TOML/YAML/diff와 원격 metadata 읽기만 수행했다.
새 interpreter/helper/테스트/Cargo/압축 해제/설치는 로컬에서 실행하지 않았다.
bin-actions x64 위 QEMU/PRoot ARM user space를 물리 ARM CPU/AGNOS kernel 부팅이나
comma3X 수용으로 표현하지 않는다. 전체 #598의 정상 PR·병합 후 검사·default 시작/기록/
업로드·복구·실측 자원/성능·최종 한영 문서와 사용자 수용 조건은 계속 남아 있다.
