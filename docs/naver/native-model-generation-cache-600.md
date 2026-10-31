# 실제 ONNX 생성과 입력별 재사용 — #600 (Actions 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
재사용 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
[Python·wheel·factory compiler 준비](native-model-wheel-inputs-599.md) 다음 단계다.
기존 Python/tinygrad 모델 생성 코드와 원래 C/C++ compiler를 유지한다.

## 실제 입력과 결과

`native_model_generation.py`는 깨끗한 현재 Git에서 주행/운전자 ONNX를 읽는다.
주행 모델은 현재 LFS pointer의 OID/길이를 별도 build asset cache에 준비하고,
운전자 모델은 현재 ordinary Git blob과 실제 파일의 길이/SHA를 대조한다.
checkout의 LFS pointer를 덮어쓰거나 runtime LFS4개 allowlist를 확장하지 않는다.
ONNX initializer/attribute 안의 외부 tensor는 읽기 전에 비활성화하고 거부한다.
주 ONNX SHA만 확인한 뒤 계약 밖의 다른 파일을 생성 입력으로 사용하는 경로를 막는다.

작성 시 읽기 감사의 주행 모델은60792584 bytes,
SHA f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5,
운전자 모델은7844499 bytes,
SHA dee5a294e8afaacc9295ac5d100e00733ecac278e79264b96331e40a3ede1b04이다.
이 문서의 숫자로 미래 모델을 강제하지 않고 해당 Git의 실제 입력을 다시 검증한다.

두 모델을 각각1928×1208과1344×760 카메라로 생성한다. 기존 manager의 분리된
`driving-models`/`driver-models` 카탈로그를 유지하며 총4개 pipeline이다.
원래 NV12/warp/metadata/queue/recurrent 경로를 사용한다. Rust `model-run
--check-catalog`가 두 카메라의 QCOM graph/descriptor/entrypoint 계약을 검사한 뒤에만
각 종류의 후보 `current.json`을 바꾼다. 이 명령은 GPU kernel을 실행하지 않는다.
기존 Rust `qcom-model-run --check-bundle`도 각 bundle의 실제 weights/kernel 길이·SHA,
program metadata·인자·dispatch를 확인한다. GPU를 열거나 kernel을 실행하지 않는다.

## 재사용·provenance·보존

pipeline별 key에 ONNX bytes, 종류/해상도/NV12, Python producer 파일 목록과 내용,
tinygrad 및 관련 원본 Python package/부모 init, interpreter/wheel Tree,
실제 ARM compiler binary·Cargo feature/profile/toolchain/lock 입력, factory LLVM20.1/
QCOM pin, renderer/schedule/environment를 묶는다.
CPU LLVM이 원래 native 설정으로 사용하는 실제 processor/features도 key에 기록하고
발행 전에 재검사한다. CPU 계산 정책을 바꾸지 않으며 QEMU와 실기 CPU가 다르면 동일
준비 결과라고 가정하지 않는다. Python package 내부의 추가·삭제와
ignored import shadow를 검사한다. 문서와 무관한 Rust 소스는 생성 key에서 제외한다.
실제 실행은 `-I -B -S -X pycache_prefix=EMPTY_PRIVATE_PATH`를 요구하고 자식 준비기에도
전달한다. `-B`만으로는 과거 bytecode 읽기가 차단되지 않으므로 빈 별도 lookup 경로를
사용한다. stdlib 검색을 먼저 유지하며 실제 import된 source 파일이 입력 closure에 있고
나머지 module이 검증된 Python/wheel prefix에 있는지 실행 후에도 확인한다.
Python package 범위 안의 무관한 변경도 보수적으로 재생성할 수 있다.

요청 commit은 보고서에 따로 기록한다. cache hit에서는 최초 생성의 commit/tree와
compiler의 실제 build source를 유지한다. docs-only 커밋으로 기존 결과를 다시 표시하지
않는다. Python hash randomization은 상태를 기록하며 bit 단위 재현성을 주장하지 않는다.

원본 archive와 wheel에서 준비 환경을 다시 검증한다. 생성 출력은 신뢰한 producer가
최초 만든 파일 전체의 길이/SHA/mode와 입력·원래 source를 content-addressed receipt에
기록한다. 이후에는 receipt의 content identity, 입력 일치, 실제 모든 출력과 catalog를
대조한다. 생성물은 외부에서 받은 파일처럼 사전 digest가 있는 자산이 아니며 이 cache는
서명이 아니다. cache 전체를 임의로 다시 쓰는 동일 계정 공격자의 실행 증명을 제공하지 않는다.

같은 입력에서는 Python graph 생성/QCOM compiler 호출을 건너뛴다. 새 catalog는 검증한
bundle을 hardlink한다. 같은 current/catalog 재사용은 output hardlink도0이다. 준비 환경과
source/output 전체 hash 검증 I/O는 남는다. raw ONNX/interpreter/wheel cache 제거 후의
재다운로드까지0이라고 주장하지 않는다.

종류별 비차단 lock, 자체 staging, file/directory fsync, NOREPLACE 발행과 atomic pointer
교체를 사용한다. compiler/consumer 실패, 입력 변경, 손상·위조 receipt/추가 파일/링크,
잘못된 current, 공간 부족/동시 실행은 거부하며 이전 세대·로그·다른 pending을 지우지
않는다. 출력 상한은 pipeline당 max(64MiB, ONNX bytes×12), 최대2GiB이고, missing 출력
합계+8MiB를 입장 추정치로 사용한다. 재사용은 metadata64KiB를 확인한다. 상한은 출력
검사와 작업별 공간 입장이며 filesystem quota나 실측 peak가 아니다. 기존 standalone
생성 CLI의 고정27GiB 조건도 같은 작업량 기반 추정으로 바꿨다.

각 종류의 카탈로그는 개발 후보를 따로 발행한다. 두 종류의 원자적 runtime 활성화,
전체 Layout 조합/실제 ABI 봉인과 정상 시작 수용은 별도 후속 작업이다.
[불변 runtime export와 source/봉인/시작 검사](native-model-runtime-inputs-600.md)를
추가했으며, CLI는 별도 --export-store를 요구한다. 새 Actions 결과는 대기다.
이 cache store 전체를 가변 상태 그대로 runtime 입력으로 봉인하지 않는다.

## 검사 방법

로컬은 구문/YAML/runner/diff만 확인한다. 아래 기능 검사는 Actions에서만 실행한다.

1. `Runtime source build foundation`: 기존 core asset CLI와 wheel 검사를 유지하고
   `test_native_model_cache.py`9개, `test_native_model_generation_inputs.py`5개 unittest를
   실행한다. 소유한 작은 가짜 출력/Git tree로 재생성 생략·입력별 무효화·최초 provenance/
   inode·실패/손상/중단/공간/lock 보존을 검사한다. 모델 추론 증거로 사용하지 않는다.
2. `Genuine AGNOS native compiler feasibility`: 원래 모든 명령/pin/360분 제한을 유지하고
   그 뒤 실제 ARM Python/native compiler로4개 pipeline을 생성한다. 반복은 전송/저장0,
   생성/QCOM 호출0·같은 generation/inputs/origins와 실제 Rust consumer 검사를 요구한다.
3. 같은 job의 별도 소유 Git clone에서 docs-only commit과 실제 logger help 변경 commit을
   만든다. 각 fixture 자체 script/source를 사용해 같은4개 결과의 생성/전송/link/QCOM0과
   최초 source 보존을 확인한다. compiler는 원래 빌드 SHA를 유지하며 fixture runtime을
   빌드했다고 표현하지 않는다. logger 실제 증분 빌드는 기존 #608 검사와 별개다.
4. artifact의 `native-model-generation-{initial,reused,docs,small-logger}.json`, compiler
   build record, commands.log, 각 catalog receipt/current/index를 정확 source SHA와 대조한다.
   기존 host LLVM18의 전체 모델·recurrent·원본 회귀 명령은 그대로 유지한다.

실행 runner는 `bin-actions`의 self-hosted/Linux/X64/dev-md-local뿐이다. ARM은 고정 AGNOS
filesystem의 QEMU/PRoot user space다. 아직 새 Actions 결과는 대기이며 실제 ONNX 전체
생성/LLVM20.1 원본 수치 회귀/전체99개 runtime/aggregate RAM·swap·disk peak/정상 부팅·
녹화·업로드/사용자 comma 3X 수용이 성공했다고 주장하지 않는다. eGPU big model은 별도다.

최신 SHA·실행 링크와 상태는 연결된 #598/#599/#600 댓글에 함께 기록한다.
