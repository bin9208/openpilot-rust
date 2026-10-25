# #482/#584 PR1344 원본 replay의 NPY 입력 쓰기 수정 — 2026-10-26

## 실제 실패와 수용 범위

실패 source는 `0e521792e8473be142d6c8bbc7aaaa9457e32a56`, [run40207816680/job121212441629](https://github.com/bin9208/openpilot-rust/actions/runs/40207816680/job/121212441629)이다. 보존 artifact12605181275는26780256B/SHA256 `bc00acb0b8a38fb2a3d4a5c692aa7ae5417f8d339cad0cc02f0e42af27a565b4`이며 private `.analysis/scratch/2026-10-25-523-completion/ci/12605181275.zip`에서 필요한 JSON/로그만 읽었다. 원본·모델·PKL·개별 명령·decoder를 로컬에서 실행하지 않았고, 전체 ZIP 독립 재감사나 전체 기능 수용으로 표시하지 않는다.

- `current-isa-feature-tests`는11 passed/0 failed/0 ignored다. `triplet-readonly`는 전체91개 strict SOURCE/object/image·host ISA를 수용했다. 실제 자연 host는 Microsoft hypervisor의 Linux x86_64/Intel Xeon Platinum8573C/4logical CPU이며 이번 feature census에는 AVX512_FP16이 없다. 따라서 앞선 Xeon6973P-C의 FP16 실제 생성 image 재수용까지 증명한 것은 아니다.
- D1은28단계·Fixture/Original/Current/held-daemon 후검증과 d1.json 작성을 완료했다. command는 약12.356초에 exit0이지만 진단 수집용 test의 종료이며 strict_pass=false/strict_failure_sticky=true다. frame3의 전체2576raw에서 index99 한 곳 actual `00000000`(+0)/frozen `80000000`(-0)를 기록했다. raw SHA256은 `069099e1d4d8081ed9de155494cd18dd00cf2d8be3e12cfe04be08d2e6857f87`, frozen은 `9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe`다. 첫 raw 거부 뒤 recursive fields와 전체12frame/33publication 성공을 주장하지 않는다. 이 signed-zero 실패는 아래 입력 API 수정과 별도로 남는다.
- 그 뒤 D2 `original-plain`은 약2.958초에 exit1이다. stderr는 helper396행의 `target.copyin(...)` → `tinygrad/device.py`209행 → 기본 `Allocator._copyin`241행의 `NotImplementedError: need copyin`을 기록했다. 첫 prepare stage의 입력 쓰기를 마치지 못했으며 original-plain/native-plain/tapped/최종 comparison receipt는 없다. 실패 입력 이름은 stderr에 없으므로 특정 key의 실제 실패로 단정하지 않는다.
- final-seals는 pass=true/failures=[]/all_owned_children_reaped=true다. job의 source/index·SDK 봉인, 독립 회수, 증거 업로드·정리 성공은 확인했지만 원본 수치 비교 성공을 대체하지 않는다. 마지막 qualification 실패는 위 미완료를 유지하는 정상 거부다.

## pinned 소스와 입력 쓰기의 동등성

`tinygrad_repo/tinygrad/runtime/ops_npy.py`의 NpyAllocator는 `_alloc`, `_as_buffer`, `_copyout`을 구현하며 `_copyin`은 구현하지 않는다. 따라서 CPU HCQ의 `_copyin` 미구현 문제가 아니다. CPU/HCQ는 기존 copyin 구현을 유지한다. `openpilot/selfdrive/modeld/compile_modeld.py`는 tfm/big_tfm 및 packed_npy_inputs를 NPY에 만들고, `model_export/pipeline.py::restore_inputs`는 captured dtype/device/크기 계약을 확인하여 같은 device에 static buffer를 복원한다.

기존 `rust/tools/check_pipeline_reference.py::write_tensor`는 NPY에 대해 `buffer.as_memoryview(force_zero_copy=True)[:] = memoryview(values).cast("B")`를 쓰고, 다른 device에는 `buffer.copyin(memoryview(values))`를 쓴다. 이번 수정도 동일한 base buffer와 동일한 device 분기를 사용한다. 차이는 비교 도구가 ndarray를 byte view로 만들고, 이번 helper는 이미 봉인된 fixture의 raw bytes를 읽었다는 점뿐이다. ndarray 재구성·dtype 변환·정밀도 변경은 하지 않는다.

NPY `_as_buffer`는 `np.require(src, requirements='C').data`를 `flat_mv`로 감싸며, `flat_mv`는 비어 있지 않은 view를 format `B`/1차원 nbytes 길이로 cast한다. `Buffer.as_memoryview(force_zero_copy=True)`는 allocator의 `_as_buffer`가 있고 options가 None일 때만 그 view를 반환하며, 조건을 충족하지 않으면 assert로 거부한다. Buffer 수명은 기존 target/tensor가 유지한다. 입력의 bytes도 1차원 byte 자료이며 기존 `target.nbytes == len(raw)` 검사 뒤 전체 slice에 대입하므로 기존 비교 도구의 byte 쓰기와 같은 길이·형식이다. 일반적인 예외 fallback이나 allocator monkeypatch를 추가하지 않는다.

## 최소 변경과 검증 경계

소스 변경은 `tools/ci/model_pr1344_replay_482.py::original_replay`의 입력 쓰기 한 곳뿐이다.

```python
if target.device == 'NPY':
    target.as_memoryview(force_zero_copy=True)[:] = raw
else:
    target.copyin(memoryview(bytearray(raw)))
```

기존 flat input/이름·전체 길이·16MiB 읽기 제한, 원문/initial allocation provenance, prepare/prepare/policy 순서, mapped image/runtime/ISA admission, plain/tapped 전체 출력 비교, D1 strict sticky 및 최종 실패 조건은 유지한다. 원본/모델 수학·기대값·허용오차·CPU·timeout과 shared WF/Focus/ledger는 변경하지 않는다. defect 이슈 작성과 pin/원장/등록·commit/push/GHA 통합은 부모 담당이다.

로컬 Python AST 구문·소스 검토·diff 공백 확인은 PASS다. 위 분기를 기존 copyin 한 줄로 역투영하면 기존 helper 전체51701B/SHA256 `760b51ea14e9502bc2ad57d1f894d4c36a1629db7b655f50841979eeaf6ca625` 및 AST가 정확히 복원된다. 수정 helper는51842B/SHA256 `2e8627ab5b71a86efaf815a35d64e443f38f8f97ec1d8d93b593790a519405ab`다. helper import/실행·Cargo/build/test/Clippy·모델 기능 실행은0이다. 이 수정의 실제 NPY 쓰기와 D2 원본/native/tapped 비교는 **GHA UNRUN**이며, 다음 exact-source Actions에서 기존 전체 흐름으로 검증해야 한다. NPY API 오류를 넘겨도 D1 signed-zero의 strict 실패를 성공으로 승격하지 않는다. 원본 비교 성공·기기 수용·full128/half·sNaN5/retirement credit은 여전히0이다.

## 후속 signed-zero 읽기 전용 소스 조사

같은 ZIP의 request·graph·SOURCE·guarded-add receipt만 읽었다. fresh 원본 triplet의 program90/kernel92 SOURCE는166049B/SHA256 `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`다. graph의 마지막 call175는 kernel92에10pointer/1scalar/2workers를 전달하며 data0는 model view9/allocation9/offset0/10304B, data1는 view365/allocation16/offset9472/3152B다. 이 정적 연결은 실제 D1의 중간 operand 값 관측이 아니다.

SOURCE의 worker0·outer index24·vector lane3으로 model index99를 역투영하면 data1[99]의 half load `%v189` → 부호/지수 bit 처리 `%v682` → vector `%v694` lane3 → `%v2650` → `%v3274` → `%v3275` lane3 store 경로다. 이 경우 `%v63=24`, `%v182=(24<394)` 분기의 원래 마지막 식은 `fadd nsz arcp contract afn float %v2650, %v3273`이며 select의 해당 arm은 명시적+0이다. 같은 bundle의 실제 compiled sidecar에는 기존 guarded-add rewrite4개 중 lane3의 `%diagnostic_guarded_sum_3.zero = fadd float %v2650, 0.0`가 기록되어 있다. 이는 이미 존재하던 소스를 읽은 결과이며 이번에 수학/flag/rewrite를 변경하지 않았다.

data1[99]의 정적 allocation16 byte 구간은9670..9672다. 그 구간에 겹치는 마지막 선행 인자는 call147/kernel67의 출력 slot0/view365와 call148/kernel68의 입력 slot3/view365다. kernel68 SOURCE는 data0에만 store하므로 이 두 call 사이에서 view365를 다시 쓰는 경로가 아니다. kernel67 SOURCE는217245B/SHA256 `2d46595564d66783b4846a87bf02247e200ba3dec4a6ea03b02204103064e948`이고 data0의 half 결과를 store한다. 이로써 좁은 선행 producer 후보를 정적으로 식별했지만, 실제 D1의 half bits·해당 producer 입력과 원본 fresh/tapped 결과는 없다. historical half/sNaN5와 이번 실제 입력을 같은 원인으로 합치지 않는다.

따라서 index99의 실제 최초 부호 생성 지점이나 frozen-0/current+0 차이의 원인을 확정하지 않는다. 다음 existing D2에서 동일 입력의 original/native plain·tapped 결과와 writer 경계를 먼저 확인해야 하며, 그 전에는 guarded-add를 copy로 바꾸거나 zero/expected/tolerance를 조정할 근거가 없다. 이번 산술 조사는 read-only이며 새 모델 실행·instrumentation·코드 수정은0이다.
