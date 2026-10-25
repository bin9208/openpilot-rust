# #523/#296 모델 compiler 결함 #482 조사

2026-10-24 KST. 기준은 `/home/fma1/.codex/worktrees/python-rust-523/openpilot-rust`,
`codex/feat-523-python-rust`, `d0f53d15a96ef474ff48810b156437a6a6923c58`이다.
상태는 **기존 수정 포함 확인 / historical half operand 불일치 미해결**이다.
새 compiler patch나 회귀테스트는 작성하지 않았다. 원인이 입증되지 않은 산술 변경은 하지 않는다.

## 범위와 근거

- [#482 최신 본문·전체 댓글](https://github.com/bin9208/openpilot-rust/issues/482)을 GitHub API로 조회했다.
  본문 `updatedAt=2026-10-23T12:04:40Z`, 최신 댓글은
  [InstCombine 관찰 기록](https://github.com/bin9208/openpilot-rust/issues/482#issuecomment-6349965576)이다.
- `rust-repository-python-retirement-296-handoff.md`의 Road#482 및
  `Lane 3: model startup and compiler behavior`, 관련 evidence 문서와 실제 job 로그를 대조했다.
- `guarded_add.rs`, `compiler_backend_cpu/llvm.rs`, `bin/offline-llvm-compile.rs`,
  `bin/offline-llvm-guarded-add.rs`, 기존 `guarded_add`/`guarded_add_cli`/`compiler_llvm`
  tests 및 원본 `tinygrad_repo/tinygrad/runtime/support/compiler_cpu.py`를 읽었다.
- 부모의 focused acceptance `40079353750`, auth/터미널/#529 musl 및 다른 작업자의
  `developer_ssh/login/native.rs` callback tests는 이번 조사에서 변경·실행하지 않았다.
  `catalog_publish.rs`, 공유 Cargo/workflows/runtime 모델 수치 gates도 변경하지 않았다.

## 이미 포함된 수정: 중복 적용 불필요

`06956b79d1d3ebacfa6e8a09616e9cdc1c358ed3`은 일반 guarded-add 후보를 추가했다.
그 초기 후보는 `select(c, +0, y)`와의 덧셈을 나눈 뒤 zero arm에서 `x`를 직접 전달했다.
`f755005e7726e9a90a07846e9eec65cc5b272abe`은 그 arm을 **flag-free `fadd float x, 0.0`**으로
바꿨다. 직접 전달은 `-0 + +0`의 부호 변화나 sNaN quieting을 수행하지 못한다.
이 수정은 출력 정규화나 특정 output index 처리가 아니다.

`git merge-base --is-ancestor f755005 HEAD`가 성공했고, 아래 다섯 파일의
`git diff f755005 HEAD -- ...`는 비어 있다. 현재 파일 SHA256도 실제 실패 artifact의
세 compiler source pin과 일치한다.

| 파일 (`rust/crates/model-runtime/` 기준) | 현재 SHA256 |
| --- | --- |
| `src/guarded_add.rs` | `a5ea7e5a9b79f5c722ea6f0d25851d5ebf2279d36a152b69f031dcf5168bf831` |
| `src/compiler_backend_cpu/llvm.rs` | `204d164dc96b48365ef8656b46e0bbeb7814530ef184191e7356b95bd1894e27` |
| `src/bin/offline-llvm-compile.rs` | `7bd9f6a9867b43dee0c120d05e97364fa0b843141615590d382b63c762b6661c` |
| `tests/guarded_add.rs` | `e9830975d19216764bf1aeb1976c817dceb9769fb194b860f8003277dff81620` |
| `tests/guarded_add_cli.rs` | `5a62add092b4a4d455166e41db6c6d39618297d06df1dbaa17ce4d861223cc90` |

## oldControls R2 실제 결과: 성공, 모델 수용과 구별

인계 문서의 `ControlsR2 now RUNNING`은 과거 스냅샷이다.
[run39817340019의 실제 job119959173147](https://github.com/bin9208/openpilot-rust/actions/runs/39817340019/job/119959173147)은
`zero-controls`, **success**다. workflow SHA는 `a42d1aefb659d99b543cf31465900603bf9352d4`,
실제 검사 source는 `f755005e7726e9a90a07846e9eec65cc5b272abe`다.
run의 `head_sha`를 실제 source checkout SHA로 혼동하지 않는다.

`gh run view --log`는 이 조회에서 비어 있었으므로
`gh api repos/bin9208/openpilot-rust/actions/jobs/119959173147/logs`로 실제 로그를 읽었다.
source units **13 passed / 1 ignored**, CLI **4 passed**, 명시적으로 실행한 ignored
`zero_arm_keeps_addition_bits_and_records_original_codegen` **1 passed**가 기록되어 있다.

artifact `12442503001`: 528,167 bytes,
SHA256 `7036169038c30aa8ddee7aa34affe76744e811fb18ccb30b5d92a845cf96c549`.
이번 로컬 inert 감사에서 ZIP 7개 CRC, 내부 tar 52개 파일 전체 SHA256,
tar seal 및 before/after identity receipt의 일치를 확인했다. retained ELF는 실행·로드하지 않았다.
과거 runner의 라이브 provider를 지금 로컬에서 재해시한 것으로 주장하지 않는다.

- 분리된 fresh old/current target과 서로 다른 helper hash가 실제 기록되어 있다.
  old `df63dfab1f797cfd30c921ab90a95d7f94ca89d3886395ccde77006c948fb2f9`,
  current `017b0d398820763931918b96564380574044289e0d8d54d2d4e60dfaf8523587`.
- 자연 target은 `znver3`, LLVM18 provider SHA256은
  `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`.
- 저장된 256행 모두 candidate의 세 출력 word가 strict addition과 일치한다.
  old helper의 negative-zero 차이 4건이 negative control로 검출됐다.
- `complete.json`은 `actual_model_operand_capture=false`,
  `historical_or_current_full_model_acceptance=false`를 명시한다.
  이 성공을 historical model 원본 비교 성공으로 확대하지 않는다. 성공한 동일 controls를 재실행할 이유가 없다.

## 남은 실제 실패: historical half operand 5건

[run39833558563/job120015837327](https://github.com/bin9208/openpilot-rust/actions/runs/39833558563/job/120015837327)은
**failure**다. 실제 source `1d5570e158bb7a799c4a816f78bc7f63f201d2a4`,
workflow `0138e0176d25755d82a6a9f9425abce3965d888e`.
실제 예외는 `strict byte gates remain required; diagnostic rows saved before rejection`이다.

artifact `12449897200`: 26,734,808 bytes,
SHA256 `fae8695cefb272f44c5bf5c33f608ad2e2d69d6d4504c1c8ee883cef51d51317`.
이번 inert 감사에서 전체 265 ZIP CRC, 262 content seals, source/held before-after receipt의
일치 및 120개 저장 poststate의 해시를 확인했다. 각 case의 candidate/fresh-original
8개 buffer가 서로 같고, output allocation9 외의 7개 buffer가 frozen 값과 같음도 확인했다.
이것은 저장 바이트 비교이며 kernel·compiler·모델의 로컬 실행이 아니다.

모든 차이는 output allocation9의 float99 한 원소에 있다. 아래 출력은 **little-endian raw bytes**다.
half 열은 정수 bit pattern 표기다. baseline은 원래 allocation16의 byte9670 half `0x8000`을 사용한다.

| case | 입력 half bits | frozen original float99 | fresh original / candidate float99 | old helper |
| --- | --- | --- | --- | --- |
| baseline | `0x8000` | `00000080` | `00000000` | frozen과 일치 |
| negative-zero | `0x8000` | `00000080` | `00000000` | frozen과 일치 |
| negative-subnormal | `0x8001` | `00000080` | `00000000` | frozen과 일치 |
| positive-snan-payload-1 | `0x7c01` | `0020807f` | `0020c07f` | 같은 quiet-bit 불일치 |
| negative-snan-payload-1 | `0xfc01` | `002080ff` | `0020c0ff` | 같은 quiet-bit 불일치 |

따라서 R4의 original `+0` / native `-0` 3원소만 고찰해서는 이 이슈를 해결할 수 없다.
old helper로 되돌려도 두 sNaN case는 실패한다. 원본 `+0`를 항상 유지하거나
출력 부호를 정규화하는 방법도 위 증거를 보존하지 못한다.

`attribution-complete.json`의 `strict_frozen_original_parity=false`,
`fresh_context_original_parity=true`, `prior_run_stays_failed=true`를 그대로 유지한다.
fresh original/candidate의 전체 object SHA256은 모두
`b1833aa2ff3b84f90d6126091bddd870542c17743aa8ebc5c8870d3d70a8ad7a`이고,
최적화 IR 해시는 서로 다르다. 해당 run의 자연 target은 EPYC7763/`znver3`이며 직전 설정과 같았다.

## 입증된 범위와 원인 미확정 경계

`attribution-source.py`의 실제 `replay()`는 `original`, `old`, `candidate` **모두 같은
Rust `offline-llvm-compile` worker**를 호출한다. `original`이라는 label은 unchanged
original IR을 뜻한다. 원본 Python `CPULLVMCompiler.compile_to_obj`를 별도로 호출한
독립 compiler 경로 증거는 이 세 항목에 없다. 그 성공은 source IR 간의 fresh-context
대응 증거이며 historical compiler context 전체 복원의 증거가 아니다.

원본 IR 166,049 bytes의 SHA256은
`e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`이다.
artifact의 `input/historical/source.ll`과 `input/operand-replay/original.ll`이 정확히 같다.
원본 provider와 Rust worker의 target-machine 기본값·O2 및 pass 옵션 설정을 소스로 대조했으나,
그 대조만으로 historical object의 생성 경로 차이를 입증하지 못했다.

[선택된 InstCombine 관찰39839598730](https://github.com/bin9208/openpilot-rust/actions/runs/39839598730)은
`function(instcombine)`에서 old copy arm이 negative-zero addend select로,
candidate가 positive-zero select와 flag-free addition으로 바뀐 사실을 보여준다.
이는 전체 pass pipeline의 유일 원인이나 historical bit parity를 입증하지 않는다.
`nsz`가 존재하는 IR에서는 서로 다른 zero 처리가 허용된다는 관찰과,
외부 exact raw-byte 계약의 실패를 구별한다. 현재 추가 production patch를 정당화할 원인은 미확정이다.

## 다음 단일 focused CI 명령 — UNRUN

다음 경계는 **동일한 sealed original kernel92 IR을 실제 원본 Python compiler와 fresh Rust
worker로 한 번씩 컴파일하여 대응을 기록하는 것**이다. 모델 실행·전체 60-case 반복·256 controls
재실행은 이 단계에 필요하지 않다. 아래는 명령 제안이며 이번 조사에서는 실행하지 않았다.
새 workflow나 wrapper 파일이 이미 존재한다고 주장하지 않는다.

부모가 `bin-actions`에서 준비해야 할 입력은 다음과 같다. 현재 checkout/provider/interpreter,
새로 빌드한 worker와 그 source의 before/after identity/hash를 유지하고 작업별 증가량을 승인한다.
`ORIGINAL_KERNEL92_LL`은 위 SHA의 artifact에서 검증한 original IR,
`NATURAL_CONFIG_JSON`은 **이번 job의 실제 CPU/features**를 관찰한 Config JSON,
`CURRENT_LLVM_WORKER`는 이번 source에서 fresh target으로 빌드한 worker다.
`LLVM_PATH`는 위 LLVM18 SHA의 held provider, `ORACLE_PYTHON`은 기존 원본 oracle 환경의
interpreter, `FOCUSED_OUTPUT`은 이번 작업 소유의 빈 디렉터리다.
원본 compiler 및 tinygrad import 소스도 현재 checkout으로 봉인한다.

```bash
timeout --signal=KILL 90 "$ORACLE_PYTHON" -I -B - <<'PY'
import contextlib, ctypes, hashlib, json, os, pathlib, subprocess, sys
assert os.environ['GITHUB_ACTIONS'] == 'true'
assert os.environ['RUNNER_NAME'] == 'bin-actions'
root = pathlib.Path(os.environ['GITHUB_WORKSPACE']).resolve()
out = pathlib.Path(os.environ['FOCUSED_OUTPUT']).resolve()
assert out.is_dir() and not any(out.iterdir())
source = pathlib.Path(os.environ['ORIGINAL_KERNEL92_LL']).read_bytes()
assert hashlib.sha256(source).hexdigest() == 'e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0'
config = json.loads(pathlib.Path(os.environ['NATURAL_CONFIG_JSON']).read_bytes())
assert pathlib.Path(config['library']).resolve() == pathlib.Path(os.environ['LLVM_PATH']).resolve()
assert hashlib.sha256(pathlib.Path(config['library']).read_bytes()).hexdigest() == config['library_sha256'] == '5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0'
assert all(config[k] is True for k in ('loop_unrolling', 'loop_vectorization', 'slp_vectorization', 'verify_each'))
os.environ['DEBUG'] = '7'
sys.path.insert(0, str(root / 'tinygrad_repo'))
with (out / 'original-provider.log').open('x') as log, contextlib.redirect_stdout(log):
    from tinygrad.runtime.autogen import llvm
    from tinygrad.runtime.support.compiler_cpu import CPULLVMCompiler
    compiler = CPULLVMCompiler(['x86_64', 'native'])
    for key, getter in [('triple', llvm.LLVMGetTargetMachineTriple), ('cpu', llvm.LLVMGetTargetMachineCPU), ('features', llvm.LLVMGetTargetMachineFeatureString)]:
        ptr = getter(compiler.target_machine)
        actual = ctypes.string_at(ptr).decode()
        llvm.LLVMDisposeMessage(ptr)
        assert actual == config[key], (key, actual, config[key])
    assert compiler.passes.decode() == config['passes']
    original = compiler.compile_to_obj(source.decode())
with (out / 'original.o').open('xb') as handle:
    handle.write(original)
subprocess.run([os.environ['CURRENT_LLVM_WORKER'], '--config', os.environ['NATURAL_CONFIG_JSON'], '--object', str(out / 'native.o'), '--receipt', str(out / 'native.json'), '--optimized-ir', str(out / 'native.optimized.ll')], input=source, check=True, timeout=65)
native = (out / 'native.o').read_bytes()
receipt = {'original_object_sha256': hashlib.sha256(original).hexdigest(), 'native_object_sha256': hashlib.sha256(native).hexdigest(), 'object_equal': original == native, 'model_or_historical_acceptance': False}
with (out / 'compiler-pair.json').open('x') as handle:
    json.dump(receipt, handle, indent=2)
assert original == native, receipt
PY
```

이 명령은 native worker를 역사 target으로 강제하지 않는다. 현재 자연 target이 과거와 달라도
현재 원본/native pair끼리만 비교하고, 과거 object SHA와 일치해야 한다고 요구하지 않는다.
로그의 최적화 IR은 진단 출력이며 LLVM module 원문 파일이라고 간주하지 않는다.
원본/native object가 다르면 그 두 경로의 실제 옵션·호출·최적화 IR에서 첫 차이를 조사한다.
같으면 compiler 진입 경로 차이라는 가설이 좁혀질 뿐이며 historical 5건은 계속 실패다.
이 경우 필요한 후속 증거는 historical original의 실제 target/features/pass 옵션,
source→object 생성·cache 유래와 전체 pass/codegen 경계다. 기록이 없으면 미확인으로 남기며
target 강제, index mask, tolerance/golden 변경이나 반대 방향 zero 패치로 대체하지 않는다.

## 이번 검증 및 변경 파일

실행한 것은 Git/API/소스 조회, 파일 해시·ZIP CRC·봉인된 저장 바이트 비교와 문서 diff 검토다.
제안 명령의 Python 본문은 `ast.parse` 문법 검사만 통과했다. 새 문서의
`git diff --check --no-index /dev/null ...`에서 공백 오류가 없음을 확인했다.
Rust source는 무변경이므로 동일한 포맷 검사를 반복하지 않았다.
기능 테스트·빌드·LLVM compile·원본 compiler 호출·모델 실행은 로컬 **UNRUN**이다.
새 push, Actions dispatch/rerun, 기기 접근 및 원격 이슈 변경도 하지 않았다.
기존 성공 근거를 현재 전체 model/runtime 전환 완료로 해석하지 않는다. #482는 열린 상태로 유지한다.

이번 작성 파일은 `docs/naver/python-rust-523-model-482.md` 한 파일뿐이다. 조사 단계에서는 미커밋 상태였고 부모가 후속 검증 배치에 기록한다.
조사 중 외부 작업으로 HEAD가 `17ff019d230d7b84ae6bca1bc2e8a0c75db07b14`로 이동했으며,
그 시점에도 위 다섯 파일의 `f755005` 대비 diff는 비어 있었다.
compiler 및 기존 tests는 무변경이며 다른 작업자의 변경은 보존했다.

## 부록: frozen original compiler 유래 조사 — 2026-10-24 KST

**입증된 결론:** baseline에서 원본 kernel92를 새로 컴파일한 것이 아니다.
저장된 PKL의 `Ops.PROGRAM`에 연결된 `Ops.BINARY` 바이트를 추출해 사용했다.
PKL 자체는 더 오래된 job에서 새로 생성됐지만, 그때 kernel92의 `compile_cached()`가
cache hit였는지 fresh compile이었는지는 확인한 기록만으로 입증되지 않는다.

- [baseline run39729562958/job119661244322](https://github.com/bin9208/openpilot-rust/actions/runs/39729562958/job/119661244322),
  source `defb9011df3fca924dca41a703b48ce5ff0c51e0`,
  [실제 workflow21ed5629](https://github.com/bin9208/openpilot-rust/blob/21ed562971a92e4fea74ea777c189fad36e7bc2b/.github/workflows/model-road-guarded-copy-296-once.yml).
  [artifact12404776700](https://github.com/bin9208/openpilot-rust/actions/runs/39729562958/artifacts/12404776700)은
  4,391,525 bytes / SHA256 `ba28ae07b5becc3afe4346db3e86437f9a93aaaf02031c19f83e070ad9c7d93b`.
  metadata·원래 로그·workflow를 먼저 읽고, 8MiB 입력 상한 아래 ZIP hash와 133 CRC를 확인했다.
- artifact의 `watch.py:66–68`은 `extract_kernel92(pkl)` 결과를 그대로 저장한다.
  `road_program_binding.py:87–117`과 `first-kernel/program-binding.json`은
  PKL SHA256 `487ef8698775a593726c79ef0651940480f755f5ded57f672d0ee3a4bd8a624f`,
  metadata SHA256 `8d86fabaa6e1d89f2f69028a1cf158be512cace51183f13c719d0434d9056026`,
  IR SHA256 `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`을 연결한다.
  binary leaf 위치는 metadata 내 `6798231`, 길이 14,442 bytes, code SHA256은
  `324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767`이다.
  이는 fresh compiler의 ELF `.o`와 구별해야 하는 저장 runtime image다.
- 이 baseline의 **재생/guarded variant compile 호스트**는 `actual-cpu.json`상 AMD EPYC9V45,
  `guarded-copy-result.json`의 LLVM 자연 CPU는 `generic`, pass는 `default<O2>`다.
  전체 feature string은 같은 JSON의 `actual_host_features`에 있으며 `+avx512f`,
  `+avx512dq`, `+avx512ifma`, `+f16c` 등을 포함한다. `road_guarded_copy.py:101–128`은
  `CPULLVMCompiler(['x86_64','native'])`와 실제 mapping을 기록한다. 그 provider는
  `/usr/lib/llvm-18/lib/libLLVM.so.1`, SHA256
  `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`이다.
  이 값들은 **PKL 안 원본 code가 최초 생성된 CPU/features/provider의 증거는 아니다**.

**PKL 생성까지의 연결:** tracked fixture ledger와
[원래 run39635596188/job119345061762](https://github.com/bin9208/openpilot-rust/actions/runs/39635596188/job/119345061762)을 대조했다.
[workflow8e77eed6](https://github.com/bin9208/openpilot-rust/blob/8e77eed65b9c68ada39f0415f7a67c8164c34f26/.github/workflows/driving-native-296-once.yml)은
source `e25a14386582858dae25c6daca5ced3bfb4bacab`에서 `compile_modeld.py`를 실행한다.
로그는 `2026-10-21T16:37:31.9931238Z`에 `Saved JITs ... driving_tinygrad.pkl (72.05 MB)`를 기록한다.
이 원래 run 전체는 failure이며 PKL 생성 기록을 전체 성공으로 재분류하지 않는다.
관찰 manifest는 같은 72,045,323-byte PKL SHA를 확인한다.

원래 artifact `12365522894`는 4,388,618,662 bytes여서 전체 다운로드하지 않았다.
HTTP `206`·정확한 `Content-Range`·요청별 4MiB/합계 상한을 강제한 선택 읽기로
중앙 directory와 `driving-native-296/fresh-original-observation/manifest.json`만 조사했다.
두 읽기의 실제 전송 합계는 7,959,336 bytes이며, 선택 manifest의 ZIP CRC와 SHA256
`ed174bb4610ee55108b3f2502d7ed284cb66abb3cd153777ca546bf8fe0d3fca`를 확인했다.
4.39GB ZIP의 전체 SHA/CRC를 독립 검증했다는 주장은 하지 않는다.
manifest의 imported source SHA256은 다음과 같으며 해당 Git blob과 일치한다.

- `tinygrad_repo/tinygrad/runtime/support/compiler_cpu.py`: `db106591197315ecf28f0e15de99d2841c9892e6e58290213c62c598e7014139`
- `tinygrad_repo/tinygrad/device.py`: `7c28ae3854a3f009fc6eddfb52f03b3d382a23b581d94a3c4741a12bdc79f008`
- `tinygrad_repo/tinygrad/codegen/__init__.py`: `3e137459c3c1c7a7d8bc7aa67da9a067ff7e0a9dd547d327a5f42b0c10c86357`

이는 관찰 시점의 source provenance이며 kernel별 최초 compiler 호출 receipt는 아니다.

**미확정 경계:** 당시
[codegen `do_compile`](https://github.com/bin9208/openpilot-rust/blob/e25a14386582858dae25c6daca5ced3bfb4bacab/tinygrad_repo/tinygrad/codegen/__init__.py#L201)은
`ctx.compiler.compile_cached(source.arg)`를 호출한다.
[`Compiler.compile_cached`](https://github.com/bin9208/openpilot-rust/blob/e25a14386582858dae25c6daca5ced3bfb4bacab/tinygrad_repo/tinygrad/device.py#L279)는
cache가 없을 때만 `compile()`을 호출하며,
[`LLVMCompiler.compile`](https://github.com/bin9208/openpilot-rust/blob/e25a14386582858dae25c6daca5ced3bfb4bacab/tinygrad_repo/tinygrad/runtime/support/compiler_cpu.py#L90)은
`compile_to_obj` 후 `jit_loader`를 거친다. cache key에는 processor/features/jit/opt가 있으나
LLVM library SHA는 없다. **실제 cache hit나 그것이 결함 원인이라는 뜻은 아니다.**
확인한 원래 workflow·로그·manifest에는 kernel92별 cache hit/miss, cache row/생성 object,
최초 TM CPU/features 및 최초 loaded LLVM SHA를 함께 묶은 receipt가 없다.
당시 apt `libllvm18=1:18.1.3-1ubuntu1` 설치 로그와 `LLVM_PATH` 설정만으로 이 공백을 채울 수 없다.

**model pair 결과에 따른 최소 후속:** source562a의
[40082144360](https://github.com/bin9208/openpilot-rust/actions/runs/40082144360)은 이번 조회에서 queued였다.
준비/봉인 실패면 그 경계만 보완하고 기존 pair를 완료한다. 실제 object가 다르면 같은 한 IR의
원본/native config와 최적화 IR에서 첫 compiler 경계 차이를 조사한다.
object가 같으면 성공한 pair를 반복하지 않고, 다음 CI는 같은 한 IR에서 원본 `compile_cached`
진입의 실제 key·hit/miss·반환 image SHA와 `compile_to_obj` 호출 여부 및 ELF→runtime-image
연결만 기록하는 작은 compile-only probe로 좁힌다. 생성 code/model을 실행하지 않으며,
그 새 관찰도 과거 cache hit/miss의 소급 증거로 사용하지 않는다.
기존 26MB/265 ZIP과 알려진 half 5건은 재감사·재계산하지 않았다.
로컬 compiler/model 실행·archived ELF/script 실행 및 workflow/source 변경은 하지 않았다.

## 원본 cache probe CI 통합 — 2026-10-24 KST, 실행 UNRUN

위 부록의 조건부 다음 단계를 cache 전용 CI로 구체화했다. 성공한
[pair run40084922315/job120845780499](https://github.com/bin9208/openpilot-rust/actions/runs/40084922315/job/120845780499),
source `97f7ad36dfa0919b4e5c216fb16f44ed29161663`은 현재 원본/native object가
각 17,808 bytes / SHA256 `e29630e0aade6971df153c374afda4e5f7052e80cc495f7e1647e72170f9c9b3`로
일치했다. 이는 현재 IR와 두 compiler 진입점의 대응 증거이며, frozen PKL의 최초 생성 환경이나
historical half 5건의 수용 증거가 아니다. 이번 workflow는 그 pair를 재실행하지 않는다.

변경 범위는 다음 네 파일이다. 기존 pair script와 Rust/Cargo, 다른 workflow 및 main 기록은
변경하지 않았다. 이번 작업에서는 커밋·push·Actions 실행을 하지 않는다.

- `tools/ci/model_cache_probe_482.sh`: 검토한 cache probe draft를 CI 진입점으로 승격.
- `tools/ci/model-cache-482-natural-config.json`: 성공 pair의 exact JSON bytes를 고정.
- `.github/workflows/python-rust-523-model.yml`: 원본 cache 전용 job으로 교체.
- 이 문서의 이번 끝 섹션: source/pin·동작 범위·검증 한계 기록.

고정 config의 출처는
[artifact12556379198](https://github.com/bin9208/openpilot-rust/actions/runs/40084922315/artifacts/12556379198),
ZIP 26,950,553 bytes / SHA256
`f8fae028c614f5efa33db25150d29347a52c7f9b49de6e4f6330aa069c063d8c`의
exact member `model-compiler-pair/natural-config.json`이다. 부모의 다운로드 완료 audit 뒤
로컬 ZIP 크기·전체 SHA와 이 단일 member의 CRC/1,247 bytes/SHA256
`2d769aa04fecbf2175532d5ebca995eaaece64adb1200d81bcddebabd1ff7a0d`을 확인하고
원문 bytes만 소스 파일로 추출했다. archive의 code 실행·unpickle·68 CRC 재감사는 하지 않았다.
새 job은 이 26MiB artifact를 다시 다운로드하지 않는다.

자연 TM은 `CPULLVMCompiler(['x86_64','native'])`의 실제 getters/pass에서 생성하며
config 전체를 고정 reference와 비교한다. 실제 `alderlake`/features,
`x86_64-none-unknown-elf`/`default<O2>`가 다르면 실패하고 target을 강제하지 않는다.
SDK는 현재 pair의 실제 경로 `/usr/lib/x86_64-linux-gnu/libLLVM-18.so`, apt
`libllvm18:amd64=1:18.1.3-1ubuntu1`, SHA256
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`만 허용한다.
초기 draft의 `.so.1`이나 다른 provider로 fallback하지 않는다. provisioned library/package가
없거나 pin이 다르면 설치·교체하지 않고 중단한다. Rust toolchain/worker build는 필요 없다.

새 job의 artifact 다운로드는 작은 baseline `12404776700` 한 건이다. `actions:read`로
metadata의 id/run/workflow SHA/expiry/digest/4,391,525 bytes를 확인한 뒤 ZIP을 받는다.
API와 ZIP 각각 GNU timeout30s, metadata64KiB·ZIP8MiB 파일 상한을 적용하고, 정확 길이와
ZIP SHA256 `ba28ae07b5becc3afe4346db3e86437f9a93aaaf02031c19f83e070ad9c7d93b`을
확인한 완성본만 입력으로 쓴다. 133개 member 중 중복이 없어야 하고 expanded total은
16MiB 이하다. 아래 네 member만 CRC/길이/SHA 검증 후 inert bytes로 추출한다.

- `first-kernel/kernel92-original.ll`: 166,049 bytes,
  SHA256 `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`.
- `first-kernel/original-kernel92-code.bin`: 14,442 bytes,
  SHA256 `324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767`.
- `first-kernel/program-binding.json`: 967 bytes,
  SHA256 `e9353a5e24b982c1cbe45f9e02bb479a867273115971da9104d03f7d06e78728`.
- `first-kernel/context.json`: 396 bytes,
  SHA256 `a6a1f6b07d38a54e85537df7655fced57139e77dfdb195c0c9e16b44dca9f02a`.

원본 package source와 실제 import 파일, interpreter/pyvenv, loaded LLVM/provider/input을
before/after 봉인한다. workflow도 전체 tracked source/index, venv 파일·symlink 목록,
provider 파일/package/realpath를 전후 대조한다. script는 새 proof 아래의 진짜 private SQLite
DB만 사용한다. 원본 `compile_cached`를 동일 IR로 두 번 호출하며, bound `diskcache_get/put`,
실제 `compile_to_obj`, 원본 `jit_loader`에 관측 wrapper가 위임한다. 실제 miss→compile→link→put→hit,
get2/put1/compile1/link1, key 전체와 실제 row, 첫/둘째 반환 image 일치를 요구한다.
fake cache 답변·archived cache 주입·PKL 역직렬화는 없다.

fresh ELF와 실제 `jit_loader` 반환 image, DEBUG7 IR log, event/count/hash/seals를 남긴다.
ELF/link는 bytes와 relocation 처리까지만 수행하며 생성 object/image/model은 실행하지 않는다.
GNU timeout90s의 자체 process group과 파일당16MiB/core dump0 제한을 유지한다.
frozen image와의 전체 byte 비교는 `fresh_image_equals_frozen` 및 첫 차이의 **진단값**이다.
불일치를 정규화하지 않으며 일치 여부는 cache protocol 성공과 구별한다.
`objects_executed:false`, `frozen_image_executed:false`,
`model_or_historical_acceptance:false`를 항상 유지한다. 새 miss/hit 관측으로 과거 hit/provider나
미확인 최초 CPU를 소급 입증하지 않는다. runtime 수치 gate/CPU target/golden/tolerance는 무변경이다.

기존 source checkout 전 2GiB 작업별 admission과 source ownership guard를 유지한다.
이후 준비는 provisioned LLVM/Python toolcache 증가량0, private venv/wheel 상한768MiB와 진단256MiB를
합친 1GiB를 실제 filesystem free bytes로 사전 확인한다. private 준비의 실제 증가량도 기록하고
768MiB 초과면 probe 전에 중단한다. Python3.12.15의 기존 complete toolcache가 없으면 다운로드
대신 중단한다. pip는 pinned numpy wheel만 설치하고 cache/source build를
남기지 않는다. 기존 Rust 준비 3GiB는 제거했으며 볼륨 확장이나 고정 reserve 완화는 하지 않는다.
실패/timeout에도 proof를 항상 upload하고, upload id/digest와 source after seal·소유권 확인 후에만
기존 cleanup 경로로 이번 source와 job temp를 정리한다.

로컬 검증은 shell 본문 `bash -n`, embedded Python `ast.parse`, JSON/YAML 문법 및 diff 검토뿐이다.
실제 venv import/cache miss-hit/compile/link와 새 workflow의 다운로드·봉인·upload/cleanup은
**UNRUN**이며 다음 부모의 정확 source CI가 확인해야 한다. 자연 features/provider가 reference와
다르거나 baseline이 만료되면 실패하므로 그 경계만 조사한다. #482 및 모델/runtime 전체 수용은
열린 상태로 유지한다.


## 0e28 원본 cache 실제 관측

[40087242279/job120852565465](https://github.com/bin9208/openpilot-rust/actions/runs/40087242279/job/120852565465)은
성공했다. artifact12557553192/5,024,082B/SHA256
`f193dcf00c29d4db0df066fba797a370f0f3fd3b11ae06737c86a702f7eb88fc`,70 ZIP CRC와 source/index,
source/provider/input/imports 전후 일치를 확인했다. 원본 get2/put1/compile_to_obj1/jit_loader1,
실제 private SQLite miss→hit 및 같은16,256B 반환 image를 관측했다. image SHA256은
`f2427b64b89301d197dfd374a6ae89ca3166d3ca1d13203aca277ff9bb9a59ed`이며,
fresh object SHA는 성공 pair의 `e29630e0aade6971df153c374afda4e5f7052e80cc495f7e1647e72170f9c9b3`와 같다.

frozen14,442B와 byte14부터 다르다. 이 결과는 현재 자연 target의 실제 cache protocol 진단이며
과거 최초 compile/cache CPU/provider를 소급 확정하지 않는다. 실제 metadata Python은3.12.15였고
모델/kernel/object/frozen image를 실행하지 않았다. `model_or_historical_acceptance:false`와
historical5건 실패를 유지하며 성공 cache/pair는 변경 근거 없이 반복하지 않는다.


## 역사 object의 새 단서와 비실행 대조 후보

[39635596188/artifact12365522894](https://github.com/bin9208/openpilot-rust/actions/runs/39635596188/artifacts/12365522894)의
outer central directory28880entry 전체와 제한된 selected member를 HTTP206 Range로 조사했다.
전체4388618662B ZIP digest는 API metadata이며 전체 body를 독립 검증한 hash는 아니다.
20MB 상한 내12190859B 전송으로 central directory/개별 LFH/길이/CRC/SHA를 검증했다.

`complete-fixture/catalog/e776e00928c8166d2cb5d47874d43f2aa860161f5c0110dc556210fa42805534/`
아래 두 driving 해상도의 kernel92.o는15928B/SHA256
`bf56a30c37f0128497b626329df7239dd7a1c4b5dd9ec728d501f2c11deebea1`로 같다.
해당 object는 당시 e25a source의 native exporter가 원본 IR 함수명을 바꿔 컴파일한 역사 산출물이다.
PKL 최초 compile object라는 receipt는 아니며 그 유래를 소급 확정하지 않는다.
IR166049B/SHA e223a2c...도 같은 fixture manifest와 source/PKL pin에 연결된다.

object의 text14244/readonly section 크기는 frozen14442B layout과 맞는 단서지만 전체 byte 일치는
아직 UNRUN이다. 기존 `cached_elf::reconstruct`에 실제 역사 object를 넣고 전체 frozen BINARY와
비교하는 read-only ignored test 및 등록 Actions 후보를 준비한다. 모든 input/source/test ELF의
FD를 보유하고 전후 identity/bytes SHA를 확인한다. compiler·generated code·모델은 실행하지 않는다.
새 gate나 object 포장 우회는 만들지 않으며 현재 재구성기의 그대로인 엄격한 section/relocation 정책을 쓴다.

후보 성공도 ISA admission, 최초 compiler/provider 입증, raw 수치5건·전체 모델·device 수용은 아니다.
그 결과를 토대로 기존 cached issuer의 엄격 gate를 유지한 후속 독립 검증을 진행한다.


## e1bf 역사 object 전체 image 일치 수용

[40094270972/job120873139135](https://github.com/bin9208/openpilot-rust/actions/runs/40094270972/job/120873139135),
source `e1bfbdc22ca11f6ce4d89c1c95ed02cd4c52b5ec`가 성공했다.
기존 native 재구성기로 역사object15928B에서 얻은 image14442B가 frozen BINARY와 전 바이트 및
SHA256 `324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767`까지 같다.
text extent14244B/alignment16, first mismatch는null이다. ignored1과 strict Clippy가 성공했다.
compiler/JIT/generated-code/model 실행은0이며 ISA 및 수치 수용은false다.

artifact12560955339(4750233B), SHA256
`84342c6721b7e5d4d14b9062f9d3af97723e509c31070227b40bae8703f59fe7`의
41CRC와 source/index/전체SDK 전후4쌍 및 input/ELF 보유 봉인을 부모가 별도 감사했다.
첫 compiler provider/cache event의 역사 identity를 소급 입증한 것은 아니다.
하지만 fresh `alderlake` compile image와 달리 실제 역사object가 frozen 전체image를 만족하므로,
엄격한 cached triplet/ABI/ISA gate를 유지한 다음 검증으로 진행할 구체적 입력을 확보했다.


## 다음 strict triplet/host ISA 후보

기존 `offline-cached-code`의 ISA 모듈 본문과 기존 unit5개를 byte 그대로 shared
`cached_isa.rs`로 옮겨 readonly 검사와 동일 구현을 사용한다. ISA 판정이나 decoder whitelist를 바꾸지 않는다.
기존 cached source9경로에 추출 파일을 추가해 정확히10경로로 강화했으며 두 native catalog consumer도
같은 roster를 요구한다. old9 current receipt는 새 source에서 미수용이고 새 issuer receipt가 필요하다.
guarded-add8 경로와 기본 cached 비활성 경계는 유지한다.

새 ignored 검사는 실제 pinned program-binding967B를 읽어10pointer/1scalar/core_id0/workers2,
global0..9와 선언을 대조하고 기존 `verify_original_program`의 엄격한 SOURCE/BINARY/object/ABI를
그대로 호출한다. 그 뒤 실제 host 기능으로 전체text14244B를 decode해 ISA를 판정한다.
상수tail을 instruction으로 보거나 sourceCompile/currentCPU tuning으로 바꾸지 않는다.
현재 test/source4개·원문4개·ELF의 FD 및 bytes/identity를 전후 보유한다.
이 후보·기존ISA unit5개·old9/변조/경로대체 회귀1개·strict Clippy는 **UNRUN**이다.
전체모델/원본92program roster, 원본실행·raw수치5건·장치수용은 이 gate로 대체하지 않는다.


두 native catalog consumer도 `driver-daemon-native`로 엄격 Clippy compile을 별도 수행한다.
기존 guarded-add source 경로8개는 보존되지만 공용 `lib.rs` bytes가 달라 과거 hash를 현재 binding으로
승계할 수 없다. 새 current issuer는 변경된 전체source bytes를 다시 봉인해야 한다.
이 단계는 기존 원본 capture나 e1bf image equality를 무효화하지 않으며 새 수치·전체모델 수용은 아니다.


## d2c18c37e strict triplet 성공·ISA 미수용

[40097057962/job120881407614](https://github.com/bin9208/openpilot-rust/actions/runs/40097057962/job/120881407614)는
source `d2c18c37eb502f3d8e5a6d0314c709804df92f38`에서 strict source/object/BINARY triplet을
수용했지만 cached program0의 `AVX512VL` extension에서 거부해 전체 실패했다. 기존ISA unit5와
source9→10 closure거부1, model-runtime Clippy 및 두 native catalog consumer의 feature compile은 성공했다.
반환 feature목록은 API가 검사한 subset이므로 그것만으로 host의 AVX512 부재를 확정하지 않는다.

artifact12561770243(5,420,496B, SHA256
`bb3ef2e11dbcae8472b89f022cf4be838fa066cb3871cc6df09bd547b6d7e326`)의48CRC와
source/index/준비SDK content·roster 전후4쌍을 부모가 확인했다. held source/입력/실제 ELF도 불변이다.
실제 test ELF SHA는 `31dfc62fa9db40b90a0973968f23769a593b39aa638528ad868366c3d1426666`다.
generated code/model/원본 compiler 실행은0이며 수치5건/전체program roster 수용은 false다.

다음 진단은 거부 instruction의 IP/Code/encoded bytes/CPUID 목록과 현재 자연 CPU/OS feature를
기록한다. decoder allowlist/host admission/수치 기준을 바꾸거나 CPU·ISA·target을 강제하지 않는다.

### 실제 PKL 크기의 별도 issuer 제약 보완 후보

원본 driving PKL72,045,323B는 기존 issuer의32MiB materialized read에서 거부된다.
원본 모델에만 기존 catalog의 모델 합계128MiB 정책에 맞춘 cap을 두고,64KiB `read_at` SHA와
정확 길이/EOF·FD offset 불변 및 전후 FD/path identity 검사를 추가했다. source32MiB와
manifest/ISA/image/출력 상한, 원본SHA, schema와10source roster는 유지한다.
`HeldLibrary::verify`의 전체 파일 크기 Vec 복사도 동일 streaming 검증으로 바꿨다.

synthetic 큰 입력/offset/EOF/동일 크기 변조·truncate/동일 bytes 교체·symlink 거부5개를
작성했다. 다음 Actions는 cached-code 기존8+새5=13, ISA기존5, Clippy `--lib`와 실제 catalog
consumer compile을 확인한다. 로컬은 repository edition2021 rustfmt·diff만 성공했다.
실제72MB 원본의 새 issuer 통과와 ISA/수치·전체 모델 수용은 아직 미실행이다. current source/ELF
binding은 새 소스로 발급해야 하며 옛 receipt의 current 자격을 자동 상속하지 않는다.

### 9c95 실제 instruction 및 현재 CPU 확인

[40098470418/job120885554504](https://github.com/bin9208/openpilot-rust/actions/runs/40098470418/job/120885554504)는
원본triplet 성공 후 ip0x14c의 `EVEX_Vpbroadcastd_xmm_k1z_r32`, bytes `62 f2 7d 08 7c d6`,
CPUID `[AVX512VL, AVX512F]`에서 거부됐다. 같은 job의 자연 `lscpu`는 i9-13900이며 OS feature에
AVX512가 없다. 이 cached image는 현재 host에서 실행 수용할 수 없다. ISA allowlist와 CPU/target을
바꾸거나 불가 명령을 실행하지 않는다. actual generated/model execution은0이다.

artifact12562433455(5,423,678B,SHA256 `464cda9216eb0671f9cfdaa91a024275352d8d6eff193681665eb0bc4694800d`)의
51CRC와 source/index/SDK 전후4쌍을 부모가 확인했다. 기존ISA5/roster1/Clippy/두consumer compile은
성공했으나 수치5건과 전체모델 수용은 계속 false다. 원본이미지/동작 기준을 보존한 다른 모델
검증 경계와 나머지 개발도구 전환을 이어간다.

### 642d01 streaming guard의 실제 집중 수용

[40100476235/job120893195418](https://github.com/bin9208/openpilot-rust/actions/runs/40100476235/job/120893195418),
source `642d01cc2296790226d7f9058e4f19bca8dc9b1e`의 cached-code13개(기존8+streaming5), ISA5개,
현재 library/bin/test strict Clippy 및 두 native catalog consumer compile은 성공했다.
artifact12562813044(5,424,311B,SHA256 `01eb7b7d6fc9f7c60953e73c9798b5ab8499e116c3e2be18035b4051d5872a23`)의
51CRC와 source/index/전체 SDK 전후4쌍을 부모가 감사했다. 실제 test ELF SHA256은
`2b5fed3c378f22621fc87ec125cf511a033489a0330056e4bba51fa127ba2508`이다.

전체 job은 위와 같은 자연 host의 frozen ISA 거부 때문에 실패다. 새 streaming guard는 큰
synthetic 입력에서 검증됐으며, 실제72,045,323B 원본 PKL은 이 읽기 전용 역사 triplet 경계의
입력이 아니므로 새 issuer로 실제 원본을 수용했다고 기록하지 않는다. generated image/모델 실행,
FP5·전체 모델 수치 수용과 원본 compiler 최초 발행자 증명은 계속 미완료다.


## PR593의 현재 원본·native road1928 실제 byte 차이

[40161295597/job121073229367](https://github.com/bin9208/openpilot-rust/actions/runs/40161295597/job/121073229367),
source merge `40562f2d8068878a00b18e12f355799b4fe0405d`에서 현재 원본128frame 실행 후
`model-1.bin`에서 strict 비교가 실패했다. road1928 두 번째frame의2576 float 중
index100/101/1349 세 원소만 원본 bits `80000000`, native `00000000`이다.
나머지2573 원소는 전바이트 같다. frame0 및 앞서 확인한 다른 output의 성공을 전체128frame
수용으로 확대하지 않는다. 뒤의1344/driver/publication/retirement는 이 step 실패로 미실행이다.

artifact12586773257은18301B/SHA256
`7dbdc3eedec86a9ffb86f0a49653ac3e2d252c556fa5829a462f4145d354b579`,5CRC/expanded21621B를
부모가 감사했다. complete raw pair 각10304B와 original-preservation-verified true다.
별도 sidecar artifact12586298677은15043364B/SHA256
`0d13baade833e2921e3964088afb6c7ceea023f1ef5cf7cdb39a6439e0fb924f`,47CRC/expanded43321456B다.
source40562 및 final helper/provider guards0을 확인했다. 안의 bounded tar1467개/expanded36910209B에
현재 road1928 kernel92 원문166049B와 exact compiled source/4rewrite receipt가 남아 있다.
자연 CPU는 AMD EPYC9V45이며 과거 i9-13900의 ISA 거부를 이 host에 소급하지 않는다.

현재 pair는 같은 job에서 ONNX로 새로 생성한 original PKL과 current native compile의 비교다.
과거 frozen1344float99와 동일 원인이라고 아직 확정하지 않는다. signed-zero 정규화/허용오차/
기대값/CPU·target 변경 없이, 동일 원본 SOURCE/BINARY와 실제 object의 전체 재구성·ISA 및
최초 producer를 확인할 후속 경계를 검토한다. 기존 guarded-add unit 성공을 이 raw 수치 수용으로
대체하지 않으며 compiler/kernel/모델의 로컬 실행은0이다.

## 현재 whole triplet 뒤2frame producer 관측 후보 — Actions UNRUN

PR593 source `40562f2d8068878a00b18e12f355799b4fe0405d`의 road1928 frame1
model2576개 중100/101/1349 signed-zero3만 확인된 상황에서 다음5파일을 한정 구현했다.
기존128frame/full-model/driver/publication 수용 gate를 바꾸거나 과거 half/sNaN5를 해결한 것으로 세지 않는다.

- `.github/workflows/python-rust-523-model-current-triplet.yml`: 승인 branch push 전용 ubuntu24 X64 진단.
- `tools/ci/model_current_triplet_482.py`: stdlib controller와 원본 worker. 새 Python 실행 edge이며 부모 원장 등록이 필요하다.
- `rust/crates/model-runtime/src/cpu/diagnostic_482_tests.rs`: 기존 feature 아래 private ignored lib-test2개.
- `rust/crates/model-runtime/src/cpu.rs`: 그 test module의 cfg 등록만. 생산 execute/수학/API/Cargo는 불변이다.
- 이 문서의 후속 범위 기록.

A는 absent private CACHEDB를 tinygrad import 전에 정한다. 실제 compile_cached/get/put/compile_to_obj/
원본 jit_loader/elf_loader/CPUProgram 생성에 delegate wrapper를 두고 반환값을 교체하지 않는다.
기존 grouped original compiler argv의1928/1344 순서를 유지한다. capture warm-up이 실제 generated code를
실행하므로 job 전체 compile-only/실행0이라고 쓰지 않는다. 같은 job의 새 PKL에서 road1928의 전체 unique
PROGRAM과 COPY/SLICE, 모든 emitted call·global/core/scalar·allocation/view/입출력 연결을 수집한다.
실제 반환 원본명 object와 전체 BINARY 및 로드 주소 정렬이 연결되지 않으면 실패한다. native rename object를
원본 object로 대신하지 않는다. LLVM18 provider SHA는 기존 `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`이고
실제 compiler target-machine triple/CPU/features/passes와 `llvm.dll._name`을 읽는다. CPU/feature forcing은 없다.

첫 Rust selector는 exact guarded-add8/cached10+신규 test의 compiled/current source를 대조하고 기존
`verify_original_program`의 전체 object→image/ABI 검증과 `cached_isa::admit_all`의 whole-roster/전체64MiB
상한·실제 host ISA 검사를 적용한다. model128MiB는 streaming SHA로 읽고 PKL/이미지는 실행하지 않는다.
유효한 입력에서 missing/duplicate/order·view bound·image 변조·global/worker ABI 거부를 확인한다.
기존 issuer는 이미 인증된 ISA receipt를 요구하므로 자동 접속하지 않는다. 기본 cached loader도 disabled다.

A 성공 뒤 B는 기존 pipeline의2frame 입력 생성식(seed20260930, frame/big_frame, matrix, packed fields,
frame0 hidden-state)을 유지한다. 모든 original/plain-native 출력과 original/native tapped 출력을 byte로
대조한다. original CPU worker 안에서 copyout/as_memoryview/Device.synchronize를 호출하지 않는다.
native adapter는 같은 held kernels.so의 실함수/ABI/인자를 정확1회 forward하며 FFI에서 unwind하지 않는다.
실제 model view가 offset0/10304B이고 정확 source `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`의
단일 call/2worker 및 포인터·전체 view roster가 대응할 때만 worker0의100/101, worker1의1349 raw4bytes를
읽는다. 다른 alias/producer가 있거나 context가 다르면 관측을 거부한다. read-only selector는 generated
execution0이지만 B는 실제 계산이다. call producer와 최초 SSA arithmetic producer를 구분해 후자는 false다.

CI의 exact selector는 다음2개이며 각각1pass/0fail/0ignored와 실제 receipt가 필수다.

```text
cpu::diagnostic_482_tests::fresh_program_roster_triplets_and_host_isa_are_bound_without_execution
cpu::diagnostic_482_tests::bound_road1928_two_frame_producer_capture
```

둘 다 `--ignored --exact --nocapture --test-threads=1 --color=never`로 실행한다. fresh Cargo JSONL 전체를
eager parse해 exact package/manifest/src/features/profile.test/kind/unique ELF를 선택한다. archived ELF와
cached issuer 실행은0이다. 기존 maintained normal40 helper의 SHA 고정 stdlib `Holds/Children/snapshot`만
재사용하며 normal40 기능은 호출하지 않는다. 실제 job key `model-pipelines`로 기존 LFS driving admission을
사용하고 DIAG/CI296은 동일 소유 source에 명시적으로 연결한다. fresh private Rust1.94는 기존 현재 SDK
prepare/whole seal 계약을 사용한다. 새 WF의 정적 정책·Focus/source pins·Python edge 원장 등록은 부모 소유다.
특히 cached10의 cpu.rs postimage가 달라지므로 과거 receipt를 현재 것으로 바꾸지 말고 소비자별 새 binding이 필요하다.

동시 growth 계획은 source2GiB/build+registry8GiB/SDK3GiB/venv1GiB/proof512MiB/apt2GiB/종료512MiB다.
최초17GiB를 실제 각 filesystem에서 확인하고 helper도 allocated/apparent 상한과 남은 growth·종료 reserve를
감시한다. du 오류/형태 미확정은 성공으로 처리하지 않는다. file512MiB/log32MiB, original source/object/image16MiB,
request4MiB, program4096, event100000/16MiB가 후보 한도다. 별도 재시도/상한 확대는 없다. 전체 helper40분,
readonly CPU60s/AS4GiB/wall90s, 각2frame180s를 shared remaining에 묶고 job finalizer 여유4분을 남긴다.
새 VM의 compiler·memory·copy·growth fit은 아직 실측하지 않았다.

실패에서도 source/index/whole SDK/Python base·venv/held ELF·provider의 전후 봉인과 pidfd/subreaper 회수,
partial object/image/IR/compile events/실제2frame raw 출력의 bounded proof 보존을 독립 수행한다.
maps는500ms 표본이며 전체 pre-dlopen 관측으로 주장하지 않는다. PKL/weights 전체는 artifact에 중복하지 않는다.
numeric mismatch는 모든 관측이 끝나도 그대로 helper/closure FAIL로 남고 진단 완료로 대체하지 않는다.
full128/full-runtime/historical-provider/half-sNaN5/retirement flags는 false/0이다.

로컬 검증은 AST/YAML/Bash 문법·Rustfmt·source/diff뿐이다. 새 source의 실제 compile/readonly decoder/
original capture/2frame plain-tap/회수 및 모든 기능은 **Actions UNRUN**이다.
