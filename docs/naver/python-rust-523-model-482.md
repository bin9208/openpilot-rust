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


### d514의 첫 hosted 준비 실패와 파일명 정정

[40170396030/job121099875349](https://github.com/bin9208/openpilot-rust/actions/runs/40170396030/job/121099875349),
source `d514956d7b9365beabc0687b39cd80c366a290a8`은 LLVM 준비에서 실패했다.
artifact12589909513은3,631,153B/SHA256 `0debbbb38f3fceb1ea5001666bdd2f1e4e4a5c5c51b9fc36557dca08db15e72a`,
64CRC/전개13,020,924B다. 실제 apt update/install은 성공했고 libllvm18은
`1:18.1.3-1ubuntu1`이었다. 잘못 지정한 `libLLVM-18.so.1`을 찾지 못해 LLVM before manifest가
작성되지 않았다. Python 준비·현재 모델 compile·2frame 관측은 도달하지 않았으며 source/SDK
최종 봉인은 성공했다. 전체 진단 수용은 실패 상태다.

후속은 기존 성공 model workflow가 고정한 `/usr/lib/x86_64-linux-gnu/libLLVM-18.so`를 canonical
해석하고 같은 SHA `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`를 검사한다.
package version도 위 정확값으로 검사한다. provider 교체·숫자 기대값·허용오차·runtime 수용 변경은 없다.

### ffa의 현재 compile/Clippy 성공 뒤 DSO 목록 parser 실패

[40170689564/job121100726744](https://github.com/bin9208/openpilot-rust/actions/runs/40170689564/job/121100726744),
source `ffa09722288ecfa98c4cb0fb461ab3ad53b63690`은 LLVM 준비를 통과했다. 단독 감사한
artifact12590250880은7,348,756B/SHA256 `028a7f239c9b63259d027f29b9cf45b444bd438df298be41df205f9124e09762`,
114CRC/전개36,455,098B다. current bins16.603초/libtest compile5.423초/strict Clippy15.293초는
각 exit0이었다. 최초 실패는 `python-dso-2`의 `ldd` 응답 해석이다.

NumPy2.5.3 `_operand_flag_tests.cpython-312-x86_64-linux-gnu.so`의 실제 `ldd`는 exit0,
stdout `statically linked`였으나 helper는 이를 `unknown/missing DSO provider`로 거부했다.
이때 fresh original import/compile·전체 triplet A·2frame B는 미도달이다. source/index/전체 SDK/
Python base·venv/registry/held final과 자손 회수는 pass이며 전체 진단/closure는 FAIL이다.
이를 모델 수치 실패나 compile 결함 수용으로 표시하지 않는다.

기존 로컬 감사 ZIP의 동일 source-lock NumPy wheel16,717,410B/SHA
`b7e18c623bb5c95acb3b3328861272816ba199fb531921c5d6d0b675f1fde9e3`을 inert read하여 확인했다.
문제 파일은16,800B/SHA `d985fc6e648abe43a0d11d9e9c1c49c15a3020936c6105c159cc9ab2ff2e41f0`이고,
동일 wheel의 `_struct_ufunc_tests.cpython-312-x86_64-linux-gnu.so`는16,936B/SHA
`895b0c3376a227d8f2f9f35465876cd6b381daa177effa7a3070c7d31bba8688`이다. 두 파일은 실제
ELF64 little-endian/ET_DYN/x86_64/PT_INTERP0/PT_DYNAMIC1/DT_NULL 종료/DT_NEEDED0이다.
이는 선언된 DT_NEEDED edge가 없다는 뜻이며 Python runtime symbol/provider 폐쇄 증거는 아니다.

후속 source는 exact-single-line `statically linked`에만 위 두 venv exact member·크기·SHA와
실제 ELF 구조를 교차검증해 한정 영수증을 쓴다. GHA에서는 실제 bytes의 needed 삽입/동적 header
누락·중복/절단/ET_EXEC 변조5종도 거부해야 한다. 다른 unknown/누락 library/root/count 거부는
그대로다. LLVM pin·CPU·생산 수학·Rust test·cached 정책·한도·expected 변경은 없다.
로컬은 AST/YAML/Bash 문법·inert wheel/ELF metadata·diff만 확인했으며 수정 경로와 통제의
기능은 **Actions UNRUN**이다. fresh original/A/B와 기존 half·sNaN/역사·runtime 미수용도 유지한다.

### b9cc의 fresh 전체 roster 수집 뒤 bool-pointer ABI 거부

[40171726319/job121103782651](https://github.com/bin9208/openpilot-rust/actions/runs/40171726319/job/121103782651),
source `b9cc4c108c2ea1289762c6e10980d0e4f4e8192b`의 artifact12590232774는11,577,976B/SHA256
`0d7e1e876345acddf09543e7d165888b0d61131a9a3a1cc93b6499b65269e856`, 442CRC/전개60,402,842B다.
현재 bins/libtest compile/strict Clippy와 DSO 준비는 성공했다. fresh original 수집도42.633초/exit0으로
91개 unique PROGRAM, 176개 호출, 104개 compile/link event를 기록했다. 이 수집은 원본 warmup의 generated
code 실행을 포함한다. 보존한 source/object/image273개는 request의 길이/SHA와 각각 일치한다.

최초 실패는 A의 readonly selector3.501초/exit101이며 `program73: cached LLVM pointer calling convention`이다.
원문 `model482/retained/triplets/compile-73.ll`의 SHA는
`f6b328b6e7f08b90191e9d1e8de2392eec5de4d5fc5d9bfc7ebab43b889b632b`이고 첫 줄은 다음과 같다.

```llvm
define void @r_8_9_9_16_4(float* noalias align 32 %data0, i1* noalias align 32 %data1, float* noalias align 32 %data2) #0
```

기존 `cached_code::verify_original_program`의 포인터 타입 목록은 `i8*/i16*/i32*/i64*/float*/double*`이며
`i1*`를 거부한다. 91개 원문 header의 inert 대조에서 이 목록 밖 타입은 위 한 곳뿐이다. 원본 renderer도
`dtypes.bool`을 `i1`로 출력한다. helper의 metadata가 잘못 옮겨졌다는 근거는 없으며, 원문 수정/타입 치환/
부분 roster 선택으로 통과시킬 수 없다. `strict_triplet_all=false`, ISA 검사는 보류됐고 B의2frame/plain-tap/
worker-cell 비교는 미도달이다. 다른 PROGRAM의 후속 ISA 통과나 수치 원인은 아직 판정하지 않는다.

source/index/SDK/Python base·venv 및 held385개는 전후 일치하고 independent final-seals는 pass,
failures는 빈 목록이다. registry의 before는 archive에 별도 저장되지 않아 독립 byte 대조 대신 finalizer의
실제 pass만 확인했다. 자손 live0, source·SDK final/upload/소유 cleanup은 성공했고 전체 진단/closure는 FAIL이다.
LLVM provider는 같은 SHA `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`다.

이번 허용 helper/WF/doc 범위에서는 실행 경로를 바꾸지 않았다. 최소 후속은 별도 소유 범위에서 bool-pointer의
정확 ABI 지원과 양성/unknown·arity·alignment·argument-order 거부 회귀를 검토하는 것이다. 기존 object 재구성/
전체 ISA/소스 봉인 및 full128·R4·역사 half/sNaN5·runtime 기준은 유지해야 한다. 후속 지원과 A/B 기능은
**Actions UNRUN**이며 이번 실패를 compiler 수치 결함 해결이나 모델 수용으로 표시하지 않는다.


### i1 buffer pointer 지원 후보와 현재 source 재봉인

program73의 실제 LLVM은 `float* %data0, i1* %data1, float* %data2`이며 bool load 뒤 float select를 한다.
[LLVM18.1.3 DataLayout](https://raw.githubusercontent.com/llvm/llvm-project/llvmorg-18.1.3/llvm/include/llvm/IR/DataLayout.h)과 현재 tinygrad의 bool itemsize는1byte다. 현재 C wrapper와 Rust KernelFn은
같은 raw buffer 주소를 void pointer로 전달하므로 scalar bool 변환을 추가하지 않는다.
후속 production diff는 cached ABI pointer 열거의 정확 `i1*` 한 항목뿐이다. 기존 pointer/scalar·정렬·
arity/global order·source/object/image·ISA/issuer의 혼합 거부는 유지한다.

기존 test13개 본문은 그대로 두고 bool pointer의 실제3인자 형태와 독립 triplet/global binding,
다른 integer 폭·pointer-to-pointer·vector·주소공간·정렬·scalar/varargs 거부의 의미 있는 회귀2개를 추가했다.
새 current-triplet controller와 historical readonly workflow는 cached15개 전체 및 새2개 명시 selector를
확인한다. historical workflow의 streamed5/ISA5/실행 금지 경계는 그대로다. 로컬은 source/format/diff뿐이다.

CACHED_SOURCES10은 그대로이며 새 binary/current catalog/admission은 변경된 cached_code.rs를
현재 source로 독립 봉인해야 한다. 기존 certificate/receipt를 현재 것으로 다시 표기하지 않는다.
현재 source의 whole91 A·ISA/B 수치·기존 full128·historical half/sNaN5·runtime 수용은 모두 GHA 대기다.

### 13ed의 bool ABI 회귀 통과와 기존 AVX512 admission 거부

source `13ed850409ac7ba6e79055be036e4466ffd3a21e`의
[current40173413003/job121108900384](https://github.com/bin9208/openpilot-rust/actions/runs/40173413003/job/121108900384)와
[history40173413094/job121108900571](https://github.com/bin9208/openpilot-rust/actions/runs/40173413094/job/121108900571)는
모두 FAIL이지만 새 bool 회귀나 compile/Clippy 실패는 아니다. cached_code source는
`e0962acb92692f7bb320a9b53dec486361a26f63daf3f11ea0ca55e27b608a32`다.

단독 감사한 current artifact12591930120은11,655,829B/SHA
`f0c523a1d819720ce1ce31c1b89620bfdfb4c246ec40849b63df158b81f9a79e`, 444CRC/전개60,542,606B다.
GitHub-hosted Ubuntu24/EPYC9V74에서 bin2/libtest1은 현재 package/manifest/src/features/profile의 유일
Cargo artifact이며 모두 fresh=false다. strict Clippy와 cached-code15개는 exit0이다. fresh original은
50.721초/exit0, PROGRAM91/호출176/compile-link104개를 기록했다. readonly의 strict_triplet_all=true,
triplet_error=null로 전체 source/object/image 재구성·ABI가 통과했다. 첫 실패는 program0의
AVX512F: `ip=0xf3`, `EVEX_Vcvtsi2ss_xmm_xmm_rm32_er`, 원문 image bytes `62 c1 46 00 2a c0`다.
실제 compiler getter cpu는 znver4이며 LLVM pin은 기존5c3a... 그대로다. B의2frame/수치 비교는 미도달이다.

history artifact12591460785는5,425,373B/SHA
`5e08f77fa2bceabc0960b7cd53951ff09f9d3c4a427719fbc828a4545bc3ed62`, 51CRC/전개10,073,415B다.
실제 runner는 self-hosted bin-actions/XeonE3-1230V2이므로 hosted 증거로 확대하지 않는다. frozen 단일
triplet 검증은 통과했으나 AVX512VL: `ip=0x14c`, `EVEX_Vpbroadcastd_xmm_k1z_r32`, 원문 image bytes
`62 f2 7d 08 7c d6`에서 거부됐다. ISA controls5/cached-code15/strict Clippy와 두 catalog 소비자 compile은
성공했다. image/model 실행0이며 historical 첫 compiler/provider·전체 model roster 수용은 여전히 false다.

두 오류는 `cached_isa::feature_name`의 기존 미수용 extension 거부이며 단순 host-feature 누락과 구분한다.
current의 source/SDK/Python base·venv/held383개와 자손 live0, history의 source/index/SDK·held 입력은
각 실제 봉인 검사를 통과했고 양쪽 upload/소유 cleanup도 성공했다. 전체 진단/ISA gate는 FAIL 그대로다.
이번에는 production/helper/WF/ISA를 더 바꾸지 않았다. CPU 강제·type/ISA 허용 확장·기대값/수치/128R4
변경은 없으며, bool readonly·unit15 성공을 B·역사 half/sNaN5·모델/runtime 수용으로 확대하지 않는다.


### 실제 관측 F/VL 지원과 history의 기본 GitHub 러너 이전 후보

앞 절의 실제 실패를 근거로 `cached_isa.rs`는 AVX512F/VL 두 종류만 해독 허용 집합에 추가하고,
`current_host()`와 로드 시 `cached_code::host_has()` 양쪽에서 Rust의 실제 host detector를 요구한다.
[Rust1.94 detector](https://github.com/rust-lang/rust/blob/1.94.0/library/std_detect/src/detect/os/x86.rs)의
CPUID 및 OSXSAVE/XCR0 상태 확인을 사용한다. 기능 누락과 미수용 extension은 계속 거부한다.
컴파일 target/LLVM 옵션·CPU 강제·수치·기대값·기존128R4 및 historical half/sNaN5 조건은 바꾸지 않는다.

현재 program0의 `62 c1 46 00 2a c0`와 역사 image의 `62 f2 7d 08 7c d6` 원문을
실행하지 않는 단위 fixture로 연결했다. 각각의 F/VL host 기능 제거, 마지막91번째 image/ret 뒤 명령,
미수용 DQ·POPCNT 거부를 검사한다. ISA 단위는5→7이며 cached15/streamed5는 유지한다.
CACHED_SOURCES10/GUARDED_ADD_SOURCES8은 유지하고 새 source·issuer·catalog를 독립 봉인한다.
과거 receipt를 새 source로 다시 표기하지 않는다.

별도로 historical workflow를 `ubuntu-24.04`로 이전했다. 실제 GitHub-hosted/Linux/X64 및 OS/arch를
두 readonly test에서도 확인하고 RUNNER_NAME 위조나 skip을 사용하지 않는다. private Rust1.94 SDK의
기존4ELF 및 전체 SDK roster, source/index/provider 전후 검사를 유지한다. 역사 입력 artifact12560955339,
고정5member·원문 SHA/기대값, Cargo selectors, 이미지·모델 비실행 경계는 그대로다.

새 runner의 입장은 실제 FS별9GiB 동시 여유(source2/build+registry2/SDK3/apt1/proof0.5/finish0.5)를
요구한다. phase 전후 실제 allocated/apparent 크기와 종료 reserve를 확인한다. 경로별 hard quota나
실측 peak 보장은 아니다. job45분/시작 후 command 종료 목표41분/개별 Cargo900초를 적용하며,
source/SDK/provider/resource final 실패는 upload 뒤 소유 cleanup을 거부한다.

로컬은 Rustfmt2021·YAML/Bash/생성 BASH_ENV·source/diff만 검토했다. 새 ISA7/cached15/Clippy,
현재91 A/B 및 historical readonly의 실제 hosted fit은 모두 Actions UNRUN이다. 현재 수용·원본 제거
또는 inventory credit을 추가하지 않는다. 13ed의 실패와 위 부분 성공은 계속 그 source에만 귀속된다.

독립 검토의 P2에 따라 command remaining은 용량 검사와 argv 기록 **뒤**에 다시 계산한다.
이미41분 종료 목표를 넘겼으면 명령을 시작하지 않으며, 남은 시간으로 개별 bound를 제한한다.


### 111f에서 관측한 DQ/BW와 정확 지원 후보

[현재40176419883/job121117963146](https://github.com/bin9208/openpilot-rust/actions/runs/40176419883/job/121117963146)의
actual111f는 strict triplet/ABI91개 뒤 program0 `ip=0x1be`의 AVX512DQ
`EVEX_Vpinsrd_xmm_xmm_rm32_imm8`, raw `62 c3 7d 00 22 c2 01`에서 거부됐다.
natural CPU는 XEON8573C/Sapphire Rapids이며 F/VL std detector는 실제 true다.
artifact12593110708은11514371B/SHA
`5713389e3804bf4c4743e20d24c6d177366796552a69a77f0ec7e3cda13501ca`/444CRC다.

[history40176419778/job121117962877](https://github.com/bin9208/openpilot-rust/actions/runs/40176419778/job/121117962877)는
같은111f의 strict triplet 뒤 `ip=0x2e6`의 AVX512BW
`EVEX_Vpbroadcastw_xmm_k1z_xmmm16`, raw `62 e2 7d 08 79 05 74 35 00 00`에서 거부됐다.
artifact12593190092는5426138B/SHA
`a916dd8c280d64975af3f54a41015eb9cef39bcf7eb3491f1e0f9fb8f2e66c37`/84CRC다.
실제 EPYC7763에는 std F/VL이 없으므로 새 BW 지원 뒤에도 같은 CPU의 역사 image는 거부해야 한다.
ISA7/cached15/streamed5·strict Clippy/catalog 소비자 compile·표준 hosted SDK 준비와 source/index/SDK
final/upload/소유 cleanup은 성공했지만 모델 B·수치·역사 image의 host 적합성 수용은 아니다.

두 실제 확장만 닫힌 feature match와 current_host/로드 host_has의 공식 Rust detector에 연결했다.
production6줄 추가이며 나머지 production/ABI/수치/CPU target/source10·guarded8/원문/한도는 그대로다.
원문 DQ7byte/BW10byte의 readonly fixture와 각 필수 기능 부재·91번째 image/ret 뒤 검사를 추가하고,
이제 지원하는 DQ의 unknown 음성은 미수용 CD로 바꿨다. POPCNT 거부와 나머지4 test 본문은 유지한다.
ISA7/cached15/streamed5 수는 그대로다. 새 issuer/catalog/request/source는 독립 봉인하며 과거 receipt를
다시 표기하지 않는다. 로컬은 source/형식/diff뿐이고 신규 동작은 Actions UNRUN이다.

별도 PRbacda의 `same_elf_complete_fixture_survives_all_retired_python_absences`는 raw prediction 불일치로
계속 실패다. 이를 ABI91·ISA 단위 성공이나 이번 지원으로 해결됐다고 주장하지 않는다. 전체128/R4,
historical half/sNaN5와 runtime 수용·원본 제거 credit0은 계속 별도로 추적한다.


### 9b 역사 readonly 수용과 855의 SSE4A·전체 feature 관측 후보

[9b history40178086021/job121123033740](https://github.com/bin9208/openpilot-rust/actions/runs/40178086021/job/121123033740)은
기본 GitHub runner의 자연 EPYC9V45에서 SUCCESS다. std F/VL/DQ/BW가 실제 true이며 원문
image14442B/object15928B·text14244B의2410 instruction 전체 ISA가 통과했다. ISA7/cached15/streamed5,
Clippy·catalog 소비자 compile·source/index/SDK raw7쌍·resource/upload/cleanup도 성공했다.
artifact12592734144는5426269B/SHA
`fe2db37f870f449741dc79c791356da1912f1cde63f23cf50d41caa64c9c3421`/84CRC다.
readonly만이며 generated code/model/original Python 실행0과 수치/128/R4 수용false는 그대로다.

같은9b의 current40178086039는 current-bins 감시에서 du exit1로 A 이전 실패했다. 부분 stdout/stderr가
보존되지 않아 ENOENT/권한/I/O/용량 중 원인을 확정하지 못했다. #570으로 추적하고 helper에 phase/root/
cap/kind/argv/status·timeout·부분 원문을 첫 실패 파일로 보존한다. 각 stream64KiB·index16KiB를 넘으면
명시적으로 truncated를 기록한다. 기존10초·unknown FAIL/cap·512MiB reserve/finalizer를 유지하고
retry/ignore를 추가하지 않는다. 과거 원인 불명을 ENOENT 또는 용량 부족으로 다시 표기하지 않는다.

[855 current40178341964/job121123811451](https://github.com/bin9208/openpilot-rust/actions/runs/40178341964/job/121123811451)은
whole91 triplet/ABI 뒤 program21 `ip=0x628`, `Insertq_xmm_xmm_imm8_imm8`, raw
`f2 0f 78 d1 20 10`의 SSE4A에서 거부됐다. 이 실행은 du 실패가 아니며 immutable final-seals PASS/live0다.
artifact12593310108은11669476B/SHA
`626084c54c24e18feae9f568c84beb83ce82c3dd6d959eee7009702f32ccd183`/444CRC다.
해당 실제 확장만 feature match와 current_host/로드 detector 양쪽에 연결했다. 원문 readonly 양성·
feature 부재와91번째/ret 뒤 거부 회귀1개를 추가해 ISA7→8, cached15/streamed5는 유지한다.

별도 test-only A 관측은 source/object/image 전체 인증 뒤 같은 fixed iced decoder로 **전체91 text**의
실제 CPU feature 집합·프로그램별 길이/count·첫 Code/raw 예시·invalid 범위를 기록한다.
1M instruction/256 feature/invalid 상세64/JSON1MiB와 기존 CPU60초/AS4GiB/명령90초 제한을 유지한다.
census와 admission을 분리하며 기존 inspect/admit_all·host 검사와 첫 실패의 의미는 바꾸지 않는다.
미수용 feature를 관측했다는 이유로 자동 지원하거나 수치/기대값/CPU flag·source10/guarded8을 바꾸지 않는다.
새 source의 issuer/catalog/request 봉인이 필요하고 옛 receipt를 다시 표기하지 않는다.
새 동작은 source/AST/Rustfmt/diff만 확인한 Actions UNRUN이며 #482 수치/전체 모델 수용은 계속 미완료다.


## 2026-10-25 PR1344 첫 ModelV2 packet D0 연결 — Actions UNRUN

원문 PR head `bacda68cf229eae5f5f757774e943afeb287c0c8`의
[run40173790842/job121111148638](https://github.com/bin9208/openpilot-rust/actions/runs/40173790842/job/121111148638)는
merge source `e0e55f32161f3613bfd93bd23a18816abb159b04`에서 첫 `1344-dual` raw prediction byte 비교가 실패했다.
D0는 이 packet의 차이를 분류하는 자료 진단이며, 미리 signed zero3으로 판정하지 않는다.

- 고정 artifact12592585842 ZIP 176,747,548B/SHA `22c0e16340e756c4ba438319e26455942b4d1deb64d6e47ada03166dcf47ee66`에서
  `driving-four-source/retirement/checkout-model/1344-dual/modelV2-output-3.bin` 19,152B/SHA
  `5c9ad58bd983e76375378fae1e27dce6267a9529c25d5707d6413ad03704dfad`만 Cereal 입력으로 사용한다.
  API의 head SHA는 PR head이며, 같은 ZIP의 고정 `checkout-model.cli.json` 2,509B/SHA
  `f4047be0caa5a4d8924820309329ad3fc6066ebc471d6ce3ddb768bd666de967`가 merge source와 exit1/checker 동일성을 별도 증명한다.
- 비교 대상은 source `e25a14386582858dae25c6daca5ced3bfb4bacab`의 기존 독립 fixture manifest
  `afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d`에 고정된
  `observations/1344-dual/outputs/raw-pred-0.bin` 10,304B/SHA
  `9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe`다.
  기존 XZ blob SHA/크기, memory320MiB/expanded512MiB/regular roster1564를 검증하고 이 raw만 추출한다.
  archived ELF/PKL/원본 프로그램은 실행하지 않는다.
- 현재 source의 `driving_daemon_retirement` private sibling selector
  `model_482_d0::pinned_pr1344_model_v2_raw_prediction_full_bit_census`(`--ignored --exact`)가 실제 `event::Which::ModelV2`를 decode한다.
  EOF/union/raw 길이를 검증하고 2,576개 little-endian u32 전체를 비교한다. 서로 다른 모든 index의 두 bit값과
  signed zero/finite/nonfinite 및 qNaN/sNaN class를 보존한다. 부동소수 연산·허용오차·정규화·index mask는 없다.
- 전용 `python-rust-523-model-packet-d0.yml`은 Ubuntu24.04/X64 private SDK1.94, 전체 source/index/SDK 전후 봉인,
  전체 Cargo JSONL eager unique 선택과 현재 test ELF/declared DSO FD hold를 사용한다.
  DSO 범위는 `ldd`가 선언한 현재 test closure이며 실제 전체 runtime mapping 관측이라고 주장하지 않는다.
  새 stdlib controller `tools/ci/model_packet_d0_482.py`는 기존 common의 FD/subreaper 도구만 exact SHA로 재사용하며
  기존 model-current/normal helper나 생산 코드·공유 Cargo를 변경하지 않는다.
- source2GiB/SDK3GiB/apt1GiB/build+registry8GiB/input256MiB/proof512MiB/finish512MiB에 대해 실제 FS16GiB 동시 성장 admission을 둔다.
  controller1800초 + 독립 finalizer120초, build1200초/test60초, child file512MiB/log32MiB를 유지한다.
  실패에서도 source/SDK/held/child 회수를 독립 시도하고 업로드 뒤 봉인·소유성 검증을 통과한 root만 정리한다.

D0 성공은 decode/전체 분류 완료만 뜻한다. byte equality나 원래 모델 gate 성공을 뜻하지 않는다.
원본·모델·native daemon 실행0, 역사 provider 입증/first producer/full128/half·sNaN5/retirement credit는 모두 false/0으로 유지한다.
D1/D2 adapter, 같은 VM 원본·native 모델 재생 및 별도 nativefamily 장기 job은 이 변경에 포함하지 않았다.
로컬 AST/YAML/Bash/rustfmt/diff만 검토했으며 실제 build/decoder/selector/자원 적합성은 **Actions UNRUN**이다.
부모가 Focus/정책/원장 및 새 Python execution edge를 등록한 뒤 실제 Actions 결과로 판단한다.


### 31e D0 실제 실패와 target metadata 재관측 후보 — Actions UNRUN

[run40184569720/job121142516145](https://github.com/bin9208/openpilot-rust/actions/runs/40184569720/job/121142516145)은
source `31e29a35e1a253813f76526692919b42284505ed`에서 current-build 감시 중
`build/target/debug/deps/.tmpo0GBUR.temp-archive`의 실제 ENOENT로 실패했다.
artifact12596035285는 3,775,521B/SHA
`b3f4f340c18df40c18405640c58be5b9c0220184d6122f2591311e87d1436075`/74CRC다.
Rust 컴파일 오류나 용량 초과는 관측되지 않았으며, controller 오류 뒤 Cargo를 SIGTERM으로 회수했다.
packet/기존 raw fixture pin·1564 TAR roster admission은 통과했으나 test ELF 선택과 2576-word decode에는 도달하지 않았다.
source/index/SDK/held 봉인과 child 회수·업로드는 통과했고 qualify/closure 실패로 source/job cleanup은 모두 skip됐다.

수정 후보는 Cargo 전에 private target을 만들고 dev/ino/mode/uid/gid를 고정한다.
COMMON의 기존 mutable walk callback을 build 순회에만 재사용하되 실제 ENOENT가 exact target의 엄격한 하위 경로이고,
현재 literal도 ENOENT이며 target identity가 같은 경우에만 전체 metadata scan을 최대 3회 다시 관측한다.
성공에는 모든 scope의 깨끗한 완주가 필요하다. root 부재·교체, target 밖 경로, permission 및 다른 오류는 즉시 실패한다.
각 부분합과 build/registry 합산 8GiB 초과는 missing이 함께 있어도 즉시 실패하며 재관측으로 지우지 않는다.
Cargo 재시작·명령 재실행은 없고, source/proof/input/registry/SDK에 mutable 허용을 확대하지 않는다.

GHA 시작 시 실제 private metadata 양성 1개와 음성 8개를 별도로 기록한다.
음성은 target-root 경로·실제 root inode 교체·밖 경로·permission·현재 존재·3회 고갈·부분합 초과·합산 초과다.
root 교체 통제는 Cargo 전 owned target을 잠시 rename하고 같은 경로에 새 디렉터리를 만든 뒤 정확한 root-changed 오류와
원래 identity 복구를 확인한다. 이 통제는 compiler/decoder 양성이 아니다.
phase/attempt/부분합/identity/ENOENT 경로는 경로당 1024B·시도당 16경로·32KiB/record로 제한해 보존한다.
기존 2초 sampling, controller1800/build1200/test60초, file/log·성장 cap과 독립 finalizer는 유지한다.
로컬 검사는 AST/source/diff뿐이며 새 metadata 9통제·Cargo 완주·D0 decode와 자원 적합성은 **Actions UNRUN**이다.
원래 모델 byte 비교·전체 모델 수용·원본 제거 credit는 여전히 false/0이다.

이 resource observer의 개별 결함·완료 조건은 [#573](https://github.com/bin9208/openpilot-rust/issues/573)으로 추적하며, #482의 원본 모델 수치 수용과 구분한다.


## 9ae D0 실제 완료와 PR 첫 raw 차이

source `9ae67e412eeb8d9c4b3e19ee9f3b586fe2855739`의 [40185757021/job121146069197](https://github.com/bin9208/openpilot-rust/actions/runs/40185757021/job/121146069197)은 성공했다. artifact12595518605(3850388B/SHA256 `b4f8b99dfde59f477f6979487d66afa300b2e2e67142efff2441fb6133eba533`)142unique/safe CRC와 원문·receipt를 감사했다. metadata1양성/8음성·decoder거부6·Cargo360행의 고유성공EOF/testELF1·선언DSO·actual1PASS/0FAIL/0ignored를 확인했다. build79.424초, decode0.060초이며 source19482/SDK188/전후7쌍/held22/live0/회수·봉인·closure/upload/두cleanup이 성공했다.58scan은 모두clean/ENOENT0으로, 실제build에서경합후재관측복구한사례로세지않는다. #573의새policy/기능완료와31e의원래실패·cleanupskip을구분해개별종료했다.

현재decoder가고정PRpacket에서읽은10304B를고정frozen raw전체와 little-endian u32로직접대조했다. **2576중2575동일/1차이**, 정확히index99만actual `00000000`(+0)/frozen `80000000`(-0)이며 다른차이는없다. actual raw SHA256 `069099e1d4d8081ed9de155494cd18dd00cf2d8be3e12cfe04be08d2e6857f87`, frozen `9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe`다. 이는PR593의다른입력/index100·101·1349 세차이와같다고가정하지않는다. byte_equal=false·모델/원본/daemon실행0·half/sNaN5/full128/역사provider/firstproducer/retirementcredit=false/0을유지한다. 새epsilon/zero mask/expected/runtime수학변경은0이다.

## PR1344 D1/D2 same-VM 전용 후보 — source 동결, 기능 UNRUN

D0 실제 run40185757021/job121146069197(#573)의 원문 packet19152B를 Cereal ModelV2로 읽은 결과는 전체2576 중2575동일/index99 한 개 actual `00000000`(+0), frozen `80000000`(-0)였다. packet SHA `5c9ad58bd983e76375378fae1e27dce6267a9529c25d5707d6413ad03704dfad`, decoded raw10304B SHA `069099e1d4d8081ed9de155494cd18dd00cf2d8be3e12cfe04be08d2e6857f87`, frozen SHA `9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe`를 서로 구분한다. historical half/sNaN5의 float99와 연결된 실제 입력·buffer·producer 증거는 아직 없다.

새 `python-rust-523-model-pr1344-replay.yml`은 표준 Ubuntu24의 실제 `model-pipelines` job 하나다. 새 Python controller는 pinned `model_current_triplet_482.py`의 소스·SDK·provider/DSO hold, FS별 admission, owned subprocess/subreaper와 독립 finalizer를 재사용한다. 기존 helper/COMMON/생산 CPU/runtime/expected를 수정하지 않는다. controller 전체40분, 각 Cargo/compile900초, readonly90초(내부CPU60초), 각 runtime180초와 source2GiB/build+registry8GiB/SDK3GiB/venv1GiB/proof512MiB/reserve512MiB를 유지한다. 초기17GiB는 실제 filesystem별 동시성장 admission이며 실측 peak 주장이 아니다. full fixture/stager와 full5 PKL→4bundle current-host `export-current-driving-catalog` 기존 wrapper를 사용한다. 하위 prepare wrapper로 receipt를 새로 흉내 내지 않는다.

A에서는 고정 fixture tar59,663,776B SHA `3ddfb1213cccbbf0d4d0ef7854b79cef5637acdfd7b59d883b13063d6968b4fe`를 기존 native stager로 materialize한 뒤, driving PKL의1344 전체 unique91 PROGRAM을 읽는다. CPUProgram 생성 sentinel 아래 원래 SOURCE/name을 자연 CPULLVMCompiler의 실제 private `compile_cached` get/put→compile_to_obj→ELF→jit_loader에 위임한다. old PKL BINARY SHA와 새 image SHA를 분리하며, 생성코드 실행은 이 단계0이다. 기존 whole-roster SOURCE-name/object→image/ABI/whole-host ISA 검사를 private child에서 재사용한다. source19/CACHED10/guarded8을 확장·완화하지 않고 새 child의 compiled source SHA를 별도로 결합한다.

D1 selector `model_482_pr1344::same_vm_current_dual_first_packet`은 기존 `run_only_dual`의12frame/33publication/ownership/finalizer를 그대로 호출한다. 원래 raw 비교 거부와 다른 실행 오류를 먼저 구분하고, 실제 첫 ModelV2의 전체2576 raw 차이를 원문으로 남긴다. 해당 raw 거부는 후속 recursive field 비교보다 앞서므로 그 뒤 fields가 검증되었다고 쓰지 않는다. 수집용 test가 반환해도 D1 strict 실패는 controller의 최종 실패로 남는다. 다른 root 또는 첫 packet에서 입증되지 않는 raw 실패는 D2로 진행하지 않는다. 정상 통과 시 실패를 강제 재현하거나 반복하지 않는다.

D2는 같은 D1 catalog generation/bundle/held native ELF와 원래 warmup prepare(frame2)→frame0 prepare→policy(frame3) 세 stage만 쓴다. 원문 main/extra/packed/transform 및 전체6outputs를 사용하며 새 expected는 없다. warmup transform 별도 파일이 없으므로 실제 원문 frame0의 zero36B, initial/warmup의 calibration-unseen 상태, frame0의 liveCalibration 부재, 고정 current `DrivingCalibration::default` source를 함께 결합한다. 불일치/입력 누락은 HOLD/FAIL이며 추정 matrix로 대체하지 않는다. 원본은 A 후에만 전체 fresh image의 실제 CPUProgram을 생성하여 map/function/bytes/alignment를 확인하고, 정확 PROGRAM key의 real runtime cache에 연결한다. unknown runtime/old BINARY fallback은 거부한다. 초기 read-before-write allocation SHA와 current native initial bytes가 같아야 실행한다. original/plain·native/plain·original/tapped·native/tapped 각각1회, backend별 전체 출력 byte 일치가 필수다. 실제 같은 writer SOURCE `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`의10pointer/1scalar/2workers를 확인하고 index99 전후4B를 관측하며 실함수를 한 번 forward한다. SSA 최초 산술/half operand producer를 관측했다고 주장하지 않는다.

새 lib selectors는 `cpu::diagnostic_482_tests::pr1344::frozen_source_whole_triplets_before_runtime`(A) 및 `cpu::diagnostic_482_tests::pr1344::first_packet_whole_outputs_and_writer99`(같은 selector를 명시 plain/tapped 각1회)다. CargoJSONL 전체 eager unique/terminal/source/package/features/test-profile/fresh executable 선택을 사용한다. source/SDK/venv/base/registry·fixture/catalog snapshot과 held ELF/DSO/5PKL/manifest/직접 입력의 전후 검사를 구분한다. 기존2048 hold 상한은 올리지 않으며 full fixture의 모든 파일에 FD를 유지했다는 주장은 하지 않는다. maps는 시간 표본이며 whole-PID/FD/DSO 폐쇄 증거가 아니다. proof에는 triplet/실제packet/raw/config/provenance와 실패를 보존하고 PKL/weights를 업로드하지 않는다. cleanup은 독립 source/SDK/seal 성공과 실제 upload ID/digest 이후의 기존 AND 조건이다. 최종 진단green도 normal128, historical half/sNaN5, 과거provider, cached issuer 또는 모델/runtime 전체 수용을 뜻하지 않으며 관련 flags는 false/credit0이다.

별도 최신 PR9ae run40187340482/job121150826818의 부모 직접 감사는 `check_pipeline_reference.py:176`의 road1928 두 번째 frame `model-1.bin`에서 최초 실패했다. 원문10304B/2576 중 indices100/101/1349 세 개만 original LE `00000080`(-0)/native `00000000`(+0)이고 original SHA `7fe2075ebcc4048be72666c5fc101238df672d438dfb849507251293d4c43242`, native SHA `bbebf82fb3e5c20efd6011d5c538b005464e4f9a7eaccc1f2cd1338f44f2732f`다. 부모 raw artifact12597251454(18,301B/5CRC)와 guarded artifact12597046785(14,909,437B/47CRC) 감사에서 실제 CPU Intel Xeon6973P-C, LLVM CPU `graniterapids`, LLVM18 provider SHA `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0` 및 전후 provider/source 봉인이 확인되었다. complete-sidecars.tar의 road-pipeline-1928/kernel92 원문166,049B SHA `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`, compiled.ll166,727B SHA `ffc2206baa1f05f49a5c9d07b68de71da1a1437485da87d8e4a75d0692a31b52`, guarded receipt4rewrites도 부모가 정적으로 확인했다. PR의128frame1928 첫 비교에서 실패했으므로 PR1344에는 도달하지 않았다. 근거는 private `pr9ae-model1928-frame1-raw-audit.json`, `pr9ae-model-guarded-inert-audit.json`, `pr9ae-model1928-firstfailure-audit.md`다. D0 index99/과거593/다른 자연 CPU의 B 성공과 숫자 원인을 합치지 않으며 AMD 가정·CPU 강제·zero 정규화·허용오차 변경은 없다. 이 작업자는 해당 새 ZIP을 중복 다운로드하지 않았다.

로컬 확인은 소스/AST/YAML/Bash 문법/rustfmt/diff만이다. 새 Rust 컴파일·Clippy·원본/PKL·모델/CLI·수치·실제GHA fit은 모두 UNRUN이다. COMMON/base pin의 마지막 결합, Focus allow/pins와 새 Python execution-edge 원장은 부모가 소유한다.

## #578: source56f 두 진단의 실제 Clippy 실패와 private 별칭 후보

source `56f6bc237c739f60df60f1d227697302407260af`의
[current40191173025/job121162317311](https://github.com/bin9208/openpilot-rust/actions/runs/40191173025/job/121162317311)과
[PR1344 replay40191173098/job121162317587](https://github.com/bin9208/openpilot-rust/actions/runs/40191173098/job/121162317587)은
모두 새 private leaf `cpu/diagnostic_482_pr1344.rs:48`의 `events` tuple에 대한
`clippy::type_complexity` 한 건으로 실패했다. current의 bin/libtest NO-RUN과 replay의
model/dev-tools/daemon build·retirement NO-RUN은 실제 exit0이다. fatal1은 Cargo JSONL 원문에서
확인했으며 A/B/D1/D2·whole-host ISA·모델 수치 실행은 모두 미도달이다.

artifact12598765423은 7,158,930B/SHA256 `2272bdfbdb977abbc4f7ed8d4c7d34363ab49e899861e5f67309bf232e98ffc4`/91CRC,
artifact12598521616은 3,959,331B/SHA256 `a15fbdcdc72c11db7e0f2caae6ea6fb7682eabc08591642ebbb49bb9690bd158`/82CRC로
각1회 단독 다운로드·전수 감사했다. 두 실행의 source/SDK/Python venv/Python base4쌍 snapshot과
outer source/index/SDK6쌍은 동일하고 child live0/회수true·업로드 성공이다. 내부 final-seals는
Clippy 뒤 설정할 registry baseline이 없어 false이며 cleanup은 모두 skipped다. 이를 source drift나
모델/ISA 실패로 해석하거나 registry baseline 없는 상태를 성공으로 올리지 않는다.

[#578](https://github.com/bin9208/openpilot-rust/issues/578)의 최소 수정은 같은 tuple을 private
`WriterEvent`로 명명하고 field를 `Vec<WriterEvent>`로 표기하는 것뿐이다. tuple 자료형·순서,
기존 직렬화·callback ABI·계산/기대값·CACHED10/guarded8·unknown ISA 거부·시간/용량 한도는 그대로다.
helper가 현재 leaf의 SHA를 읽어 `MODEL482_NATIVE_TEST_SHA256`에 전달하고 Rust는 자기
`include_bytes!` SHA와 대조하므로 helper/WF literal 갱신은 필요 없다. Focus source pin은 부모가 갱신한다.
별칭 두 표현을 역투영하면 leaf 전체가 source56f와 byte 동일하며 문서는 append-only다.
로컬 rustfmt·source/diff만 확인했고 수정 후 Clippy 및 A/B/D1/D2 실제 수용은 **GHA UNRUN**이다.
private 전체 감사는 `model-56f-two-runs-audit.md`에 보존했다. road1928의3개 signedzero와
PR1344의 index99, historical half/sNaN5·normal128 및 전체 모델/runtime 수용은 계속 구별한다.

## #578 후속 current-model 경로 연결 — 2026-10-25

`4ea1baccd`의 private WriterEvent 별칭 수정은 PR1344 경로에서만 자동 실행됐다. current-triplet의 push paths는 부모 `diagnostic_482_tests.rs`만 포함하고 새 private leaf를 직접 포함하지 않았다. 실제 strict model Clippy가 leaf도 컴파일하므로 current-triplet trigger에 `rust/crates/model-runtime/src/cpu/diagnostic_482_pr1344.rs` 한 경로를 추가한다. helper·steps·env·원래 whole PROGRAM/2frame 수치·deadline·자원·source/SDK 봉인은 byte 불변이며 branch/source binding을 속이지 않는다. current 재검증은 새 exact source의 Actions가 필요하고 이 source-only 연결을 Clippy/모델 성공으로 세지 않는다.

## #570 원문 보존 수용·#579 후속 — 2026-10-25

정확 source `4ea1baccd7cbc288fdb3c7150e135675a69237bf`의 [40193190050/job121168391183](https://github.com/bin9208/openpilot-rust/actions/runs/40193190050/job/121168391183)에서 첫 du 실패 원문 보존을 실제 확인했습니다.

artifact12599126105=3939577B/SHA256 `ad21161e97d661416e40529b98f1f9b9c8a44921beb56102a0f15bc6df3feb9d`, 82unique/82CRC PASS입니다. current-daemon allocated scan exit1/0.02078924초/timeoutfalse이며 stderr1674B와 부분stdout42B를 완전 보존했습니다. 각 SHA는 `782f869b358a7942545b7b048a7a994da161ce11d6619ba824f5790c73a51b87`와 `568136f205f1aa2da8287c1f9577de9ad0762c767521a3370c25b23363a9db27`입니다. 원문은 정확 build/target/debug/deps 하위9개 missing-file 경로를 기록했고 retry/ignore는0입니다. 부분합1555288064B는 clean scan/cap PASS가 아닙니다.

source19487/SDK188/venv2037/base9398 snapshot과 outer6쌍은 실제 동일하며 children.live=[]/회수true/uploadSUCCESS입니다. registry baseline 미도달로 내부 final-seals=false, cleanupSKIP을 보존합니다. Clippy/A/B/D1/D2 미도달입니다. 관측 누락 문제 #570의 조건은 충족했으며 실제 mutable target 측정 경합 수정은 #579로 분리했습니다. #482/#523/#296 전체 수용은 미완료입니다. 상세 감사는 private `model-4ea-pr1344-firstfailure-audit.md`와 후속 tracked 모델 기록에 연결합니다.

## #579 mutable build metadata 후보 — 2026-10-25

#570에서 확보한 current-daemon9개 ENOENT는 exact mutable `job/build/target/debug/deps` 안의 compiler 임시파일이었다. build root만 FD-relative NOFOLLOW metadata로 측정하며 target의 고정dev/ino/mode/uid/gid와 현재 실제 ENOENT·strict child를 확인한 incomplete pass만 최대3회/전체10초 안에서 다시 관측한다. source/SDK/registry/venv/proof의 원래 du/unknown 거부·원문 보존은 유지한다. directory의 변하는시간·크기·nlink를 불변 조건으로 쓰지 않는다. Cargo 재실행0이며 root/부모 소멸·교체·권한·다른오류·현재존재·소진은 FAIL이다.

allocated(st_blocks×512)와 GNU usable apparent 양쪽, per-root hardlink중복제거/xdev/symlink대상미순회, registry prefix+build 합산8GiB의 partial 초과 sticky와 원래 모든 caps/512MiBreserve/global/command 시간을 보존한다. 각 scan은16개×1024B missing/32KiB record/2048scan으로 제한하며 증거저장 실패도 숨기지 않는다. 원래 scan은 peak 보장이 아니며 이 metadata 관측도 peak를 보장하지 않는다. [GNU du 원문](https://github.com/coreutils/coreutils/blob/v9.4/src/du.c)의 의미를 검토했고 실제 runner provider 동등성은 아래 GHA 통제가 필요하다.

첫 고정 Cargo build 직전 emptytarget0700을 만들고 원래 fresh absence 검사를 보존한다. 양성2는 정지 sparse/hardlink/symlink/dir fixture의 실제 GNU du 두 metric 일치와 exact absent child다. 음성12는 root/outside/permission/현재존재/다른errno/소진/deadline/allocated·apparent·합산cap/root부재·교체를 엄격히 거부한다. 실제 Cargo race 복구로 표시하지 않는다. 64KiB create_new receipt는 source/base/helper/du/targetidentity를 묶고 FD SHA hold에 넣는다. 두 WF는 exact2/12/3attempt10초·정지fixture두값·rawreceipt SHA+fullidentity+alias·du heldSHA와 기존 finalseals/children/수치AND를 독립 확인한다.

base81782B SHA256 `6b173bb32ec045bbef9a0d2a968555a9318ff031e1d4cb1574a5254033bf2759`, PR1344 helper50959B SHA256 `f551bb6a54b173d61229b895726aab3cf14366c7a90c2250af38c130239cef41`다. PR1344 실행 본문은 BASE_SHA 한 literal 외 byte 동일하며 기존 전체모델/ISA·cached10/guarded8·math/expected/normal128/half5는 불변이다. private 인계는 `model-579-bounded-build-scan/handoff.md`/`source-freeze.json`이다. 새 GNU metadata provider callsite는 기존 Python 조정 의존성의 pending으로 추가했고 모델·baseline3580 제거 credit0이다.

로컬 AST/YAML/Bash/diff/sourceprojection만 확인하며 controller/guard/fixture/du·모델/원본/프로젝트 실행0이다. metadata양성2/음성12, 실제 compile 경합 처리, strictClippy/A/B/D1/D2·전체 동등성은 **GHA UNRUN**이다. 이 후보 또는 source-only 독립리뷰를 실제 수용으로 올리지 않는다.

## PR9ae LLVM20 경계 수용 — 2026-10-25

[run40187340482/job121150827092](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482/job/121150827092)은 실제 merge `75d0cfd49912b631946721e58979487935372828`에서30target/108PASS/0FAIL/0ignored/0filtered로 성공했다. source/index 및LFS7쌍이 동일하고, provider pin/ldd·두upload·소유source/target cleanup까지 실제 성공했다. provider artifact12598113892=426B/SHA256 `f4d6c764696630a9cab962f1aa0d7ece0c12b0fab55d671db752f1e16996b3d6`/1CRC, private12597789608=3600018B/SHA256 `ae60a16744bc76a3ab211a262c904d22db7bc91c97672955954f90227f857dfc`/35CRC를 담당자가 각각1회 감사했다.

#355 unused input binding2개와 frozen strict actual-prefix의 두memory mode가 실제 통과했다. 이를 최신전체PR/실기 수용으로 올리지 않는다. 이 job은 #260 AMD emulator의mode3/8instruction/310kernel/473+473replay를 실행하지 않아 #260의 새수용으로 세지 않는다. 자연 CPU model/ISA census·whole SDK/모든DSO/전체자손FD의전후closure는 이artifact계약에 없으므로추가성공으로 주장하지 않는다. 기존원본prefix의251호출은역사capturemetadata이며 이번Python재실행횟수가 아니다.

자세한검토는 private `pr9ae-llvm20-260-355-actual-audit.md/json`, sourcebindings/testcounts/provider 및privateZIP 인덱스에 보존한다. 이미통과한108개를목적없이재실행하거나이번수치와모델전체/half5/1928·1344signedzero를혼합하지 않는다.

#579 후속 보강은 정지fixture의 실제 du2회의 stdout/stderr 각64KiB prefix와 argv/status/timeout/elapsed/길이/SHA/잘림을 strict output·metric 판단 전에 create_new로 보존한다. 비정상/timeout 예외는 증거를 남긴 뒤 그대로 전파하고 증거 저장 실패도 숨기지 않는다. env/secret·재시도·추가 subprocess는 없다. 최종 인계는 private `model-579-metadata-du-evidence/handoff.md`이며 앞선 source-only 동결3609/0eec는 이를 대체하기 전의 중간 기록이다. 아직 GHA_UNRUN이다.

## #579 실제 수용·#578 lint 및 #584 D1 경계 — 2026-10-25

정확 source `68a1098f3c23bb006dcab29e619148e0df1cc574`의 [current40196626649](https://github.com/bin9208/openpilot-rust/actions/runs/40196626649/job/121178731808) SUCCESS 및 [PR134440196626662](https://github.com/bin9208/openpilot-rust/actions/runs/40196626662/job/121178731403)의 후속 D1 실패를 각각 전수 감사했습니다.

- current artifact12599882842=37503896B/SHA256 `72d8d381564c720745578b200ac829403447d958b8e3c4de9041dec43bffadf9`/1040safe unique·1040CRC PASS, PR1344 artifact12600048061=22444202B/SHA256 `8a4000f8d2988edda5e22740c4d82faebce4f29d940870946e366af25010cb9c`/2155safe unique·2155CRC PASS입니다. 각1회다운로드/원본·모델·프로젝트 로컬실행0입니다.
- 양쪽에서 실제 metadata 양성2/음성12·source/helper/target/held du/receipt 결합과 raw du2개의 argv/크기/SHA/완전수집을 확인했습니다. 실제 GNU du와 metadata는 allocated28672B/apparent1052939B로 일치했습니다.
- current155/PR402개의 concurrent scan은 모두 first-attempt clean, 최대0.043785/0.119732초였습니다. 실제 ENOENT incomplete/retry는0이며 actual_race_recovered=false를 보존합니다. 실제 race 발생을 꾸미지 않고 의미 있는 strict-child/오류/소진/cap/root 통제 및 정상 compiler 경로 통과로 제한해 수용합니다. 3attempt 전체10초/원래8GiB·512MiB reserve/기한·수치·ISA는 불변입니다.
- 두 model strictClippy 모두 exit0, A whole91 SOURCE/object/image·ABI/hostISA PASS입니다. current B의2frame×6output12개는 원본/native/tapped 전체 bytes가 같았습니다. 자연 host는양쪽 AMD EPYC7763/znver3여서 옛 Intel road1928 signedzero3건 재현·원인해결로 올리지 않습니다.
- source19490/SDK188/venv2037/base9398 snapshot과 outer6쌍, 기존 held·최종회수/업로드/정리는 양쪽 모두 정상입니다. PR1344는그뒤D1의180초 command deadline으로FAIL했고 첫packet만남아D1strict12/33·D2·index99수치는미판정입니다. 별도 #584에서 추적하며 #482/full128/half·sNaN5/retirement credit0을 유지합니다.

#579의 bounded metadata 측정/통제·원문 보존 및 compiler 관측 실패 통과 조건을 충족해 완료 처리합니다. strictClippy blocker #578은 해소됐지만 D2실제관측은미도달이므로 해당후속상태도분리합니다. 상세는 private hosted-68a-model-audit/current-audit.md와pr1344-audit.md 및 tracked모델기록입니다.

## #584 D1 단계 관측 후보 — 2026-10-25, 기능 GHA UNRUN

[#584](https://github.com/bin9208/openpilot-rust/issues/584)는 source68a의 [PR1344 run40196626662/job121178731403](https://github.com/bin9208/openpilot-rust/actions/runs/40196626662/job/121178731403)에서 A 전체 SOURCE/object/image·ABI/hostISA/Clippy를 통과한 뒤 D1 command180초에 도달한 별도 경계다. 기존 artifact12600048061의 peer 원문17행과 packet19152B는 frame3의 입력 publish8개·image send·첫 ModelV2 수신까지 입증하지만, 다음 receive/비교/오류 teardown/private 사후 hash 중 어디서 기한을 맞았는지는 구분하지 못한다. 다음 두 publication과 이후 frame, D1 receipt, D2는 없다. 원래12frame/33publication을 완료한 정상 종료는 아니며 index99의 새 수치 판정도 없다. 실제 sha2는 opt0이나 별도 #571 성능 결과를 이 실패의 원인으로 승격하지 않는다.

기준 source `5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64`에서 private `tests/driving_daemon_retirement/model_482_pr1344.rs`에만 고정28단계의 전후 표본을 추가한다. 기존 exact workflow/ref/host/source-root/job-owner·request SHA/source/test include SHA·A 전체/hostISA 결합 후 source40/EXPECTED_SOURCE를 확인하고 한 Instant를 시작한다. 고정 관측점은 Fixture/Current/Original open, frozen/daemon admission, run_only_dual 진입/반환, packet open/decode/whole2576 차이 생성, input guards 및 Fixture/Original/Current/held-daemon 각 후검증, 기존 d1.json 작성 전후다. 반환 표본만 실제 Ok/Err와 UTF-8 경계의 오류 prefix1024B·전체 길이·잘림 여부를 담는다. env/credentials·새 원본/모델 호출은 추가하지 않는다.

새 파일은 동일 job의 `proof/model482/d1-stages/`에만 만든다. private job0700과 FD-relative NOFOLLOW로 연 proof/model482의 소유자·canonical 경로·고정 dev/ino/mode/uid/gid를 확인하고 새 단계 디렉터리0700을 배타적으로 생성한다. 고정 allowlist·순서 외 이름을 거부하며 최대32파일/각4096B/전체131072B, create_new에 해당하는 CREATE|EXCL·NOFOLLOW·NONBLOCK·CLOEXEC와 실제 mode0600/nlink1을 요구한다. 각 JSON은 sequence·한 Instant의 elapsed_ns·source40·compiled test SHA를 기록하고 write/sync 뒤 FD/hash/identity를 봉인한다. 마지막에는 정확28파일 roster와 모든 기존 단계 FD/hash/identity를 재확인한다. 기록·소유 검증·봉인 실패도 test 실패이며 기존 proof512MiB·180초/global 안의 비용이다. 부분·누락 파일은 unknown이고 마지막 receipt-write-after 표본도 strict PASS를 뜻하지 않는다.

기존 selector `model_482_pr1344::same_vm_current_dual_first_packet`, 요청/Options·Fixture/Original/Current/held 후검증·12frame/33publication·전체2576 raw·원래 오류 분기/strict sticky 및 controller의 D2 진입 조건은 그대로다. 수학/expected/CPU/ISA/180초·global·caps/independent finalizer·회수·upload/cleanup을 변경하지 않는다. run_only_dual 진입 표본만 남으면 내부 receive/compare/error Drop을 구분할 수 없다는 한계를 유지한다. elapsed_ns는 각 JSON 기록 전에 채취하므로 두 표본의 간격에는 앞선 관측의 write/sync 비용도 포함되며 순수 함수 실행시간으로 쓰지 않는다. 관측에서 확인되지 않은 hash 병목이나 단계 시각을 추정하지 않는다. full128/half·sNaN5/역사provider 및 모델/runtime 전체 수용·retirement credit0도 유지한다.

로컬은 source·rustfmt·정적 exact 역투영·diff만 확인한다. 새 writer/guard/test/helper/프로젝트/모델 실행·빌드·CI 조작0이며 실제 새 단계·resource fit·Clippy·D1/D2는 **GHA UNRUN**이다. 동결 근거와 preimage/유지 상수는 private `model-584-d1-stages/`에 보존한다. 부모가 WF/Focus/원장 바인딩과 실제 Actions를 담당한다.

## #584 301f 실제 21단계와 private sha2 빌드 최적화 후보 — 2026-10-25

지정된 `/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`에서 인계 원문 `imported/continuation-523-pause-20261025.md`와 Curie 체크포인트를 읽었다. 기존 다른 WIP는 보존한다. [source301f의 run40201220135/job121192613485](https://github.com/bin9208/openpilot-rust/actions/runs/40201220135/job/121192613485)의 보존 artifact12602092284(22459484B/SHA256 `c54c8bfbb6643021d47bab0b6b643a483f52f9b8e4fef173f123b91a9c550dce`)에서 단계 JSON21개만 정지 데이터로 읽었다. 다운로드·PKL/모델·packet decoder 실행은 없다. 기존 부모의 2173 member 감사와 이번 제한된 JSON 확인을 구분한다.

- source는 `301f88ccd19a2f0921cdefd75da7615d8cc50984`다. 단계2 fixture-open-after24.308101489초, 단계4 current-open-after106.605216397초, 단계6 original-open-after115.363367624초, 단계8 frozen-daemon-admission-after116.922956190초를 확인했다.
- 단계9 run-only-dual-before116.923824476초에서 단계10 반환147.106823595초까지 약30.183초다. 실제 반환은 `raw predictions differ from original model bytes`의 Err(48B, 잘림 없음)다. 단계16 whole-raw-diff-after147.111699779초와 단계18 input-guards-after147.115082154초가 뒤따른다.
- 단계19 fixture-verify-before147.115609602초에서 단계20 fixture-verify-after180.263173903초까지 약33.148초다. 마지막 단계21 original-verify-before180.263959625초 뒤 command deadline이며 original/current/held-daemon 후검증 완료와 d1.json은 없다. 단계 시간에는 앞선 기록 IO/sync도 포함된다. 차이 벡터 생성 관측만으로 mismatch index/count를 확정하지 않는다.

소스 검토에서 `Fixture::verify`는 내부 Original 검증과 fixture 재개방을 수행하고, inventory fingerprint는 전체 bounded 파일을 `sha2::Sha256`으로 읽어 dev/ino/size/time·FD/path identity와 hash를 재확인한다. `Original::verify`도 manifest/roster와 각 파일 SHA를 다시 검사한다. private retirement test의 CurrentCatalog/DataSeal도 같은 sha2 의존성을 사용한다. 이 경로와 위 관측은 SHA 검증 비용을 줄이는 좁은 후보의 근거이며, 33.148초 전체의 hash 귀속이나 opt3 효과·180초 내 완료를 측정한 증거는 아니다. 별도 #571 결과로 이번 실패 원인을 확정하지 않는다.

실제 수정은 `tools/ci/model_pr1344_replay_482.py`의 `current-retirement-test` 빌드 argv 한 곳에 `--config profile.test.package.sha2.opt-level=3`만 추가한다. 공통 args/env/Cargo.toml·다른 build/Clippy 호출에는 전파하지 않는다. 해당 test 빌드의 sha2에 적용되므로 admission과 후검증 양쪽 hash 경로에 영향을 줄 수 있다. 모델/runtime/test 본체 opt0 및 fresh Cargo JSONL의 opt0/debug assertions/overflow checks·unique ELF 선택을 유지한다. private `model_482_pr1344.rs`16073B/SHA256 `4dcc040ed795819356969801e395f1c50fb5a1e38ad9dadb924676c0e96ac376`는 변경하지 않는다.

로컬 Python AST 구문 확인과 source-only 역투영에서 추가 argv 두 값과 설명 주석을 제거하면 helper 기존 전체 bytes50959/SHA256 `f551bb6a54b173d61229b895726aab3cf14366c7a90c2250af38c130239cef41` 및 AST가 정확히 복원된다. 따라서 180초/global·900초 build 상한·12frame/33publication·전체2576raw/기대수치·CPU/ISA·모든 guard/후검증·strict sticky/D2 진입·독립 finalizer/upload/cleanup은 기존 소스 그대로다. docs는 기존 원문 뒤 append만 수행하고 diff 공백 검사를 확인했다. helper import/프로젝트 CLI/Cargo/build/test/Clippy/원본·모델 계산은 로컬 실행0이다.

**GHA UNRUN**: 실제 최적화 적용 Cargo artifact와 빌드 성공·새28단계 전체/후검증·d1.json·D1/D2/180초 resource fit는 아직 미검증이다. 새 실행 전 helper pin·Focus·원장/WF 등록과 commit/push/CI/이슈 갱신은 부모 담당이다. 실제 GHA 결과 전 mismatch index/count·수치 원인 해결·full128/half·sNaN5/전체 모델 수용을 주장하지 않으며 retirement credit0을 유지한다. #584/#482 및 #523 전체 완료는 아직 열려 있다.

## #587 해시 profile과 보유 실행 파일 재빌드 — 2026-10-26

`ad9f19222`의 [40204820140/job121203423355](https://github.com/bin9208/openpilot-rust/actions/runs/40204820140/job/121203423355)은 최초 보유한 `stage-driving-daemon-fixture`가 바뀌었다고 거부했다. 실제 runtime 수치나 D1 시간 개선까지 도달한 성공으로 세지 않는다. 이 회귀는 [#587](https://github.com/bin9208/openpilot-rust/issues/587)에 분리했다.

retirement test 한 명령에만 적용한 sha2 설정을 관련 model/dev-tools/daemon build·test·Clippy의 dev/test package profile에 일관되게 전달한다. 모델·테스트 본체 opt0, CPU/ISA·수치·시간 및 보유 파일 후검증은 유지한다. 최초 파일을 새 값으로 재봉인하여 변경을 받아들이지 않는다. 로컬 AST/diff만 확인했고 정확 소스의 실제 Actions는 대기다.

## #482/#584 00c899 readonly AVX512_FP16 거부와 한정 지원 후보 — 2026-10-26

지정 completion worktree에서 [source `00c899291f3c82e9a6e6449ed22ceb7fa4d691aa` / run40205795864/job121206329807](https://github.com/bin9208/openpilot-rust/actions/runs/40205795864/job/121206329807)의 보존 ZIP `.analysis/scratch/2026-10-25-523-completion/ci/12603663262.zip`을 정지 메타데이터로 읽었다. ZIP9947253B/SHA256 `383b50278a444059e2ff6f9f2162808c413b4662d6d21cd63c98eae0abeb7c82`, readonly JSON51790B/SHA256 `83553ee93a3fe4e66192f559ac87271e0e76c9fab54a7c1a33a1973e1d1c4e0c`, stderr505B/SHA256 `46bf4210e1fa38c6e66870be294c9a499cb3de1a215a3ebbb9fbc5ad8178be1f`다. 재다운로드·원본/모델/PKL/개별 코드/decoder 실행은0이며 이번 확인을 ZIP 전체 독립 재감사로 표기하지 않는다.

실제 실패는 `triplet-readonly`의 `cached program 18: unadmitted cached instruction extension: AVX512_FP16; ip=0x277a; code=EVEX_Vmovw_xmm_r32m16; encoded_bytes=[62, d5, 7d, 08, 6e, 84, 52, 06, fe, ff, ff]; cpuid_features=[AVX512_FP16]`다. readonly는 strict_triplet_all=true/complete_roster91이나 host_isa_all=false이며 generated_code_executed/cached_issuer_invoked/numeric_acceptance=false다. feature census의 complete/valid는 ISA 수용이 아니다. 앞선 #587의 stage ELF 변경 거부와 이번 ISA 거부를 분리한다. 기존 정책에 FP16 매핑과 detector 항목이 모두 없어, 다른 guard 오류를 확장 허용으로 숨기는 수정이 아니다.

실제 hosted-cpu.json은 x86_64/GenuineIntel/Intel Xeon6973P-C(family6/model173/stepping1), Microsoft hypervisor, online CPU0..3과 flags의 avx512_fp16·avx512f/dq/bw/vl·xsave·fma·f16c를 기록한다. 기존 readonly host_features에 avx512fp16이 없는 것은 탐지 항목 누락이며 hardware 미지원 판정이 아니다. lscpu 플래그만으로 새 수용을 선언하지 않는다. 다음 실행 host가 달라질 수 있으므로 동일 CPU 강제 없이 그 실행 host의 표준 detector와 전체 roster admission을 다시 확인해야 한다.

Rust1.94의 rust-src는 로컬에 없어 공식 고정 tag의 [feature 선언](https://github.com/rust-lang/rust/blob/1.94.0/library/std_detect/src/detect/arch/x86.rs#L202)과 [OS/CPUID detector](https://github.com/rust-lang/rust/blob/1.94.0/library/std_detect/src/detect/os/x86.rs#L165-L270)를 직접 읽었다. 전자는 `is_x86_feature_detected!("avx512fp16")`를 제공한다. 후자는 CPUID leaf1 ECX의 XSAVE bit26/OSXSAVE bit27을 확인한 뒤 XGETBV(0)의 SSE/AVX bits1·2(mask0x06), AVX512 bits5·6·7(mask0xe0)을 모두 요구하고 FMA/F16C 조건 안에서 CPUID leaf7 subleaf0 EDX bit23을 읽는다. 합친 OS mask는 XCR0 & 0xe6 == 0xe6이다. 읽은 공식 raw source는 arch14503B/SHA256 `a3e874f324167c5a0aa4854666d4d57a6f9d8b2f3730145413a9dfeb7c486e0b`, os16071B/SHA256 `86116b52d471341868eebdf0185aef358716e9423cbe5b71e2a6974fd7bc4bc8`다. 새 unsafe CPUID/XGETBV 구현·컴파일 feature 강제는 추가하지 않는다.

수정은 `rust/crates/model-runtime/src/cached_isa.rs`의 AVX512_FP16→avx512fp16 매핑1개와 기존 current_host의 표준 detector literal1개뿐이다. 기존 unknown-extension 거부와 각 image/전체 roster의 host 요구 검사는 그대로다. 같은 파일의 cfg(test)에 실제11바이트+synthetic ret decode/정확 code·feature·범위/양성 및 FP16 host 항목 제거 음성, roster91의 마지막 image·ret 뒤 도달 불가 명령 검사와 roster 누락 음성, 현재 host 항목과 OS-aware 표준 탐지 일치 검사를 추가했다. 기존 unknown-extension test는 FP16까지 주장하는 host에서도 AVX512CD/POPCNT를 계속 거부하도록 입력만 보강했다. 이 mutation은 host snapshot/roster를 바꾸는 검사이며 실제 CPUID/XCR0 레지스터 변경이나 개별 명령 실행 검사가 아니다. 나머지 기존8개 test 의미와 production 수학/threshold/CPU정책·시간/guard·봉인·원본 기대값은 유지한다.

부모의 GHA 실행 대상은 `openpilot-model-runtime --no-default-features --features cached-program-isa --lib`이며 기존 cfg인 Linux x86_64가 필요하다. 새 exact selectors는 다음3개다.

- `cached_isa::instruction_isa::tests::observed_avx512_fp16_image_requires_its_own_host_feature`
- `cached_isa::instruction_isa::tests::avx512_fp16_at_roster_end_and_after_return_still_requires_the_host`
- `cached_isa::instruction_isa::tests::current_host_avx512_fp16_matches_os_aware_rust_detection`

같은 tests prefix 전체11개(기존8+신규3, ignored0)와 보강한 `unknown_extension_fails_even_on_a_claimed_capable_host`를 함께 검증한다. 실제 원문 경로의 재검증은 `cpu::diagnostic_482_tests::pr1344::frozen_source_whole_triplets_before_runtime`다. 이 ignored selector는 기존 same-VM/source/host/request/전체 triplet 인증을 거친 Actions 경로로만 실행하며 단독 로컬 실행하지 않는다. 새 host detector가 false인 환경에서는 FP16 image를 계속 거부하는 것이 정상이며, true여도 다른 미지원 확장·누락 host 기능은 독립적으로 거부한다.

로컬 rustfmt1.94 `--edition 2021 --check`와 diff 공백 확인을 통과했다. 신규 test 블록3개·매핑1개·detector1개·설명 주석을 제거하고 기존 unknown test의 host 입력만 역투영하면 cached_isa.rs 기존 전체17667B/SHA256 `167cfbe83193264a52c9076d6126c971f89f6aa319efa4b09724befd379ae2cf`가 복원된다. 이는 source-only 검증이며 Cargo/build/test/Clippy/모델/프로젝트 CLI 로컬 실행0이다. **GHA UNRUN**: 기존8+신규3의 전체11개 test/Clippy·실제 host 탐지/whole ISA 재수용·D1/D2·원본 수치 비교 성공은 아직0이다. Focus/pins/원장/공유 CI 등록·commit/push/이슈 조작은 부모 담당이며 기기 수용·full128/half·sNaN5/전체 모델 수용 및 retirement credit0을 유지한다.

### #587 실제 수용 및 #590 호출부 연결 — 2026-10-26

source00c899291의40205795864/job121206329807에서5개 관련build/test와2개strictClippy가 모두0이고, 최초 held stager를 포함한 final-seals pass=true/failures=[]/all_owned_children_reaped=true였다. source/index·전체SDK·독립봉인·업로드·두cleanup도 성공했다. artifact12603663262의771 safe/unique/전체CRC를 감사했고 #587만 종료했다. 전체run은 실제AVX512_FP16 admission에서 실패했으며 모델수치·D1/D2는미도달이다.

해당관측은 [#590](https://github.com/bin9208/openpilot-rust/issues/590)에분리했다. 부모는 현재same-VM helper에서 선택한library test ELF로 cached_isa::instruction_isa::tests::11개를60초한도에서직접실행하고 정확11PASS/0FAIL/0ignored를요구하도록연결했다. Cargobuild·profile·기대수치·CPU선택을바꾸지않고 그뒤같은전체triplet admission을수행한다. 새정확source의actualunit/ISA/수치결과는GHA대기다.
