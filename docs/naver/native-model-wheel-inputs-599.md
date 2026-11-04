# Python 모델 준비 wheel과 factory compiler — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
재사용 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
[고정 ARM Python](native-python-inputs-599.md)에 모델 준비 의존성을 연결하는 단계다.
기존 Python 생성 코드와 C/C++ 라이브러리를 유지한다.

## 입력과 역할

현재 읽기 감사에서 offline_pipeline은 NumPy와 zstandard를 필요로 한다.
transformations/transformations.py는 현재 저장소의 Python 구현이다.
필요하지 않은 Cython/CFFI/전체 개발 환경을 이 단계에 추가하지 않는다.
실제 전체 import와 동작 여부는 아래 CI가 확인해야 한다.

rust/native-model-wheels.json은 CPython312/AArch64 GNU 두 wheel을 고정한다.

| package | bytes | SHA-256 |
| --- | --- | --- |
| NumPy2.5.3 | 15687872 | 76c2c1e6bfa5c84adc6434dfbf013aa92096a7985221762c8f11fedfd20fff58 |
| zstandard0.25.0 | 5063012 | 6dffecc361d079bb48d7caef5d673c88c8988d3d33fb74ab95b7ee6da42652ea |

합계20750884 bytes다. filename/HTTPS URL/size/SHA는 공식 PyPI metadata에서 확인했다.
실제 다운로드·내용 검증·import 결과는 대기다.
출처: [NumPy metadata](https://pypi.org/pypi/numpy/2.5.3/json),
[zstandard metadata](https://pypi.org/pypi/zstandard/0.25.0/json).
zstandard의 CPython C extension backend를 명시적으로 선택한다.
추가 cffi extra는 선택하지 않는다.
출처: [원본 import 정책](https://github.com/indygreg/python-zstandard/blob/0.25.0/zstandard/__init__.py).

rust/native-model-compilers.json은 원본 factory image SHA
375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44와 아래 파일을 결속한다.

- CPU: /usr/lib/aarch64-linux-gnu/libLLVM.so.20.1,138943216 bytes,
  SHA daee21a3b1abf220864bc2da60817c4800f121099a6b5cd42694975abbeab9d7.
- QCOM: /usr/lib/aarch64-linux-gnu/libllvm-qcom.so,36961040 bytes,
  SHA fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093.

CPU LLVM20.1은 캐시된 SDK 파일과 원본 image의 같은 파일을 각각 읽어 SHA가 같음을
확인했다. 이 사실은 라이브러리 실행/ABI 성공 증거가 아니다. 기존 호스트 oracle의
LLVM18 명령은 유지하고 native 준비에는 실제 factory20.1을 사용한다.
새 LLVM/QCOM 다운로드나 factory 파일 변경은 하지 않는다.

## 준비·재사용·보존

기존 Rust asset cache를 별도 설치 없이 호출하는 --prepare-assets mode를 추가했다.
이 mode는 archive feature가 없는 consumer에서도 사용할 수 있다. 기존 mode는 유지한다.
모델 helper는 아래 명령으로 소스의 고정 plan을 읽는다.

```sh
python3 rust/tools/native_model_inputs.py --root CLEAN_SOURCE \
  --preparer OPENPILOT_RUNTIME_INPUTS --cache ASSET_CACHE --store PACKAGE_STORE \
  --environment-store ENVIRONMENT_STORE --output REPORT_JSON
```

세 저장소는 source 밖의 겹치지 않는 절대 경로이며 부모가 있어야 한다.
원래 source/plan은 깨끗한 Git identity로 확인하고 준비 후 재검사한다.
source SHA는 보고서에 기록하며 wheel content key에는 넣지 않는다.
package version/filename/asset이 key를 정하고 resource estimate만 변경해도 같은 prefix다.

Rust helper는 HTTPS body·저장량을 실제 계수하고 잘못된 hit를 덮어쓰지 않는다.
Python helper는 최대32MiB 압축 bytes를 고정 buffer로 읽어 길이/SHA를 확인한 뒤 같은
buffer를 ZIP parser에 넘긴다. 앞선 cache 확인 후 다시 연 파일의 변경을 신뢰하지 않는다.
ZIP 경로/종류/mode·확장량, METADATA의 이름/버전/의존성, WHEEL의 native ABI/platform,
전체 실제 파일과 RECORD의 SHA/size·완전한 목록, AArch64 ELF header, notice를 확인한다.
pip/build/install hook을 실행하거나 wheel을 준비 중 import하지 않는다.

package별 immutable prefix와 두 package를 묶는 prefix를 분리한다.
묶는 prefix는 검증된 package 파일을 hardlink하며 두 store는 같은 filesystem이어야 한다.
한 wheel이 바뀌면 해당 package만 새로 복사하고 다른 package prefix와 inode를 재사용한다.
문서/estimate 변경과 반복 준비에서는 HTTP body/저장/복사/link0을 검사한다.
ZIP metadata/파일 hash 읽기와 prefix 전체 검증 I/O는 남아 있다.
raw wheel cache는 독립 기대 Tree 계산에 필요하며 제거하면 다시 받아야 한다.

기대 Tree는 인증한 ZIP에서 계산한다. 현재 prefix를 snapshot해 스스로 신뢰하지 않는다.
위조 receipt/손상/추가 파일/모드 변경/누락/이탈/링크/특수 파일/다른 ABI는 거부한다.
비차단 lock·별도 pending·fsync·원자 NOREPLACE 발행을 사용한다. 실패·공간 부족·동시 실행은
기존 prefix를 유지한다. 남아 있는 다른 pending 디렉터리는 삭제하지 않는다.
runtime active pointer와 기존 로그는 이 helper가 변경하지 않는다.

각 wheel은10000 members/32MiB 압축, 파일128MiB, metadata1MiB, Tree receipt16MiB로 제한한다.
해제 예상 상한은 NumPy128MiB/zstandard64MiB이며 metadata 여유8MiB를 사용한다.
두 raw cache까지 비어 있을 때 작업 예상은230466084 bytes이며 묶는 prefix는 payload를
추가 복사하지 않는다. 재사용되지 않은 package만 해당 예상량을 admission에 사용한다.
이는 실측 peak RAM/디스크 증가량이 아니다. 고정25/35GiB floor를 사용하지 않는다.

## CI와 검사 방법

foundation은 raw asset CLI의 캐시 hit/손상 보존·잘못된 mode를 실제 실행하고,
일곱 owned ZIP fixture로 반복/estimate 변경, 한 package 변경의 다른 inode 재사용,
metadata/RECORD/경로/mode/ABI 오류, 위조 receipt/손상 hit/추가 파일, lock 충돌/재시도,
공간 부족·남은 pending의 이전 세대 보존을 확인한다. 가짜 ELF를 실행하거나 이 fixture를
실제 NumPy/모델 성공으로 표현하지 않는다. 기능 검사는 Actions에서만 실행한다.

genuine host는 실제 wheel 두 개를 준비·재사용한다. NumPy wheel의 OpenBLAS/libgfortran은
기존 native-numerics-provider.json의 SHA/크기와 같아야 한다. 라이브러리를 재작성하지 않는다.
source-built ARM helper와 고정 Python은 별도 prefix를 준비해 host 기대 Tree와 대조한다.
원본 source의 openpilot-model-runtime에서 offline-qcom-compile와 model-run을 실제 native
빌드한다. Python은 -I -B -S와 명시한 package/source 경로로 실행한다.

실제 NumPy SVD/역행렬·zstandard roundtrip, original offline_pipeline import, CPU:LLVM tensor,
기존 A630 compiler의 정상 kernel 생성/잘못된 source 거부를 확인한다.
GPU device 초기화는 기존 host_only_devices 정책으로 금지한다. 자신의 maps와 kernel hash를
기록하고 새 wheel/native compiler 및 CPU/QCOM 라이브러리의 ELF closure를 감사한다.
실행 후 전체 prefix를 재검사해 전송·복사/link0과 입력 보존을 확인한다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-native-environment.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

정확 SHA의 native-model-wheels JSON/Tree, native-model-compiler-build.jsonl,
native-model-environment JSON/maps/ABI/kernel 증거를 확인한다. 전체 wheel/prefix/ONNX를
evidence artifact로 중복 업로드하지 않는다. 새 기능·import·컴파일·ABI 결과는 대기다.
로컬은 구문/서식/diff·JSON/원격 metadata/캐시·image 파일 읽기 감사만 수행했다.
새 helper·unit test·Cargo·wheel 압축 해제/import·LLVM/QCOM/model은 로컬에서 실행하지 않았다.

이 단계는 의존성 준비·미소 기능 probe다. 실제 ONNX 전체4종 pipeline 생성/원본 회귀와
생성 입력별 cache, eGPU dataset/DBC·전체 Layout/99개 native build/seal·기본 업데이트/시작/
녹화/업로드·복구, 실측 자원/성능, 정상 PR/병합 후 검사·최종 한영 문서·사용자 comma3X 수용은
계속 필요하다. bin-actions의 QEMU/PRoot ARM user space를 물리 ARM CPU/AGNOS kernel 부팅/
기기 수용으로 표현하지 않는다. 현재 compiler9개 역할/runtime10개 Tree를 완성했다고 하지 않는다.
