# 모델 불변 실행 입력과 소스·봉인·시작 검사 — #600 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
증분·복구 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
[실제 ONNX 생성/cache](native-model-generation-cache-600.md) 다음 실행 연결이다.

## 가변 cache에서 실행 입력 분리

`native_model_generation.py`에 source 밖의 별도 `--export-store`를 추가했다.
종류별 cache의 live current를 다시 선택하지 않고, 해당 요청에서 이미 검증한4개
pipeline receipt와 반환된 generation ID로 두 카탈로그를 구성한다.
선택한 `driving-models`, `driver-models`, `model-inputs`만 하나의 immutable prefix로
내보낸다. cache lock·과거 generation·Python/wheel/toolchain·SQL cache는 포함하지 않는다.

기대 Tree는 producer receipt와 고정해서 구성한 index/pointer/메타데이터에서 계산한다.
현재 export를 snapshot하여 스스로 신뢰하지 않는다. 원래 출력은 readonly hardlink,
index/pointer/생성 receipt는 별도 readonly 파일이다. 같은 export 재사용은 복사/link0이며
검증 I/O는 남는다. 다음 cache current 변경은 이미 발행한 export pointer를 바꾸지 않는다.
같은 filesystem을 요구하며, 동일 계정의 임의 파일 변경에 대한 서명/격리를 제공하지 않는다.
최종 실행 세대는 기존 objects store를 통해 이 데이터를 가져와 별도로 봉인한다.

prefix identity에는 실제 생성 receipt의 SHA를 사용한다. 요청 SHA로 원래 생성자나
compiler source를 다시 표시하지 않는다. 각 receipt에 최초 생성 SHA/tree와 실제 compiler
build source/byte hash/feature/profile/toolchain/lock을 함께 보존한다.
실제 compiler가 사용한 Cargo source Tree와 system SDK receipt SHA도 기록한다.

비차단 lock·자체 pending·fsync·NOREPLACE로 발행한다. 검증 실패나 변경된 입력에서는
새 export를 발행하지 않고 이전 prefix·실행 세대·로그를 지우지 않는다. 손상된 기존
export는 정상 cache로 덮어 고치지 않는다. 공간 입장은 실제 metadata 길이·Tree 크기·
directory 수·hardlink entry 수에 따른 추정이며, 대형 출력 데이터 복사는 하지 않는다.
이 값은 전체 빌드 peak disk/RAM/swap 실측이 아니다.

## source/runtime 소비 경계

`rust/native-model-source-scope.json`은 Python 생성 namespace, Rust compiler source와
workspace Cargo 파일, 모델 준비 plan을 명시한다. Python과 Rust가 같은 Git ls-tree raw
records를 선택하여 SHA-256을 계산한다. 문서/무관한 logger 변경은 제외한다. ONNX는
별도의 현재 Git blob/LFS OID·길이로 종류별 검증하여 한 모델 변경을 다른 모델 key에 넣지
않는다. scope 내부의 보수적인 plan/코드 변경은 추가 재생성을 유발할 수 있다.

compiler source namespace는 실제 compiler build record에도 기록하고, 생성 시 현재
compiler source와 다르면 거부한다. compiler와 모델의 원래 source는 content key 밖의
provenance로 보존한다. 현재 요청 source와 같다고 임의로 수정하지 않는다.

`openpilot-runtime-inputs --verify-models --root SOURCE --store MODEL_EXPORT`는 source와
export를 읽기만 한다. shared Rust verifier가4개 종류/카메라, 현재 source namespace,
현재 ONNX Git identity, 원래 compiler source/feature/toolchain/lock, output Tree 전체의
길이/SHA/mode·완전한 파일/디렉터리 목록과 catalog/descriptor를 확인한다. Python cache나
compiler 설치 경로를 다시 열지 않는다. 실행 증명 서명은 아니며, 최초 생성은 신뢰한
producer와 실제 model-run/qcom-model-run 검사에 의해 확립한다.

이 verifier는 updater가 가진 현재 scope bytes를 옛 세대에 강제하지 않는다. 각 retained
source의 schema1 scope를 읽고 필수 v1 역할/경로·중복·안전성을 확인한다. 새 scope의
추가 입력 때문에 기존 정상 source의 fallback 검증을 막지 않도록 한다.

- runtime 입력 Tree 하나에 두 모델과 provenance를 결속한다. native 계획에서 기존10개
  runtime Tree에 더한11개 입력으로 연결하며, compiler9개 역할/13그룹99개 roster는 유지한다.
- 모델 daemon을 포함한 준비는 이 완전한 source-bound model Tree가 없으면 Cargo 빌드
  전에 거부한다. runtime-only 입력이므로 무관한 Rust group의 compiler key는 바꾸지 않는다.
- Layout은 검증한 모델 파일을 정확한 rust/bin 목적지에 전부 매핑해야 한다. 누락·교환·
  추가 목적지를 빌드 전/봉인 시 거부한다. 전체 provider 조합에서는 이 prefix를 포함해야 한다.
- 봉인할 실행 디렉터리에 materialize한 뒤 source 모델 검사를 반복하고, 해당 source-built
  model-run과 qcom-model-run으로 실제 catalog/weights/program/dispatch를 검사한다.
- 봉인된 source generation의 manifest 검증에서도 같은 source/모델 검사를 적용한다.
  부팅/복구 검증이 가변 Python/model/compiler cache에 의존하지 않도록 했다.

## 검사 방법과 현재 한계

기능 검사는 Actions에서만 실행한다. 로컬에서는 AST/JSON/YAML/rustfmt/diff와 runner/
guard/queue·원래 명령/pin/timeout 보존만 확인한다.

1. foundation의 Python export4개 unittest: warm 복사/link0·inode, cache pointer 변경과
   옛 export 분리, 손상 export 거부, 실패 시 새 generation 미발행을 소유 fixture로 검사한다.
2. installer4개 Rust test: 실제 소유 Git/파일로 docs/무관한 Rust 재사용, source/compiler/
   ONNX/plan 변경, 손상/추가/링크/포인터, retained scope 호환을 검사한다.
3. runtime-build2개 Rust test: 실제 core CLI 검증과 정확한 Layout mapping, 모델 입력
   누락 시 존재하지 않는 compiler/cache를 열기 전 거부를 확인한다.
4. genuine ARM의 실제4개 ONNX 생성/반복/docs/작은 logger source 검사는 모두 export와
   shared Rust verifier도 호출한다. 같은 export root/Tree와 warm 복사/link0을 요구한다.
   실제 준비된11개 runtime Tree로 plan을 재조합하고 원래 compiler group recipes와 같음을
   검사한다. 이 plan 검사를 실제99개 빌드로 표현하지 않는다.

runner는 bin-actions/self-hosted/Linux/X64/dev-md-local이고 ARM은 x64 host의 QEMU/PRoot
AGNOS user space다. 새 검사와 실제 전체 생성·LLVM20.1 수치 회귀 결과는 아직 대기다.
전체 provider Layout 조합·99개 자체 빌드/실제 세대 봉인·정상 startup/update/녹화/기존
업로드·aggregate 자원/중단/전원/동시 실행 실측·정상 PR/병합 후 검사·사용자 comma 3X
수용은 계속 남는다. 실기 설치 안내나 대단원 완료로 사용하는 단계가 아니다.
