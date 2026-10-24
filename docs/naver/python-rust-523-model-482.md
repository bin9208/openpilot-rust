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
