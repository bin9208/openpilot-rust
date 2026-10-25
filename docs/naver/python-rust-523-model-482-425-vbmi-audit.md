# #482 actual425 current-triplet 실패 감사 — AVX512_VBMI

추적: [#593](https://github.com/bin9208/openpilot-rust/issues/593), 상위 #482/#523.
최초425 감사와 후속 mapping 구현을 구분한다. 새 ISA14·whole triplet의 실제 수용은 대기다.

2026-10-26. 지정 completion worktree에서 기존 GHA log/artifact와 exact source425·공식 Rust 원문만
읽었다. 이번 쓰기는 이 새 문서와 private scratch의 검증된 ZIP뿐이다. 기존 model D2/history 문서,
소스·수학·expected·tolerance·ISA·source pin·workflow는 변경하지 않는다. 기능/Cargo/oracle/CI 실행0이다.

## 결론과 정확한 실행

**이번 실패는 새 수치 mismatch가 아니라, fresh image에서 실제 관측한 AVX512_VBMI를 현 ISA
정책이 미지원으로 거부한 A admission 실패다.** strict SOURCE/object/image triplet91개는 통과했고,
두-frame original/native 비교에는 도달하지 않았다. metadata 인증 불일치·deadline·이전1344의
signed-zero D1 sticky로 분류하지 않는다.

- source: `425f29d5ffa4ae0e603ceddf5dce8c605e49bc35`.
- [run40227415603/job121271640843](https://github.com/bin9208/openpilot-rust/actions/runs/40227415603/job/121271640843).
- exact workflow: `.github/workflows/python-rust-523-model-current-triplet.yml`, job=`model-pipelines`.
- artifact12612560407: `model-current-triplet-482-40227415603-1`,11760179B/SHA256
  `55914a9e333d54246d46decdbfcd163db777c7524d67c5b1dd09224159421e15`.
- 보존: `.analysis/scratch/2026-10-25-523-completion/ci/12612560407.zip`.

API의 expected bytes/digest를 먼저 확인하고20MiB 전송 상한으로 받았다. 실제 크기/SHA 일치,
575개 safe unique regular member·비정규/절대/상위 경로·역슬래시·symlink·암호화 거부와 전체 CRC를
확인했다. expanded60779202B이며200MiB 사전 상한 안이다. 파일시스템에 ZIP을 풀거나 모델/PKL/
machine image를 실행하지 않았고 큰 result/weights를 출력하지 않았다. retained index도
model_pkl_uploaded=false/full_weights_uploaded=false를 기록한다.

## 단계별 실제 범위

| 단계/command | 실제 결과 | 의미 |
| --- | --- | --- |
| current bins/libtest/Clippy | exit0 | 현재 Rust build/Clippy 성공이며 모델 수치 수용이 아님 |
| cached-code-controls | exit0,2.814389초 | 기존 cached source/object/ABI15개 조건 성공; 이 run에서 ISA11 실행 성공을 추가 주장하지 않음 |
| fresh-original | exit0,52.175302초 | 원본 compile/capture와 complete selected roster91·calls176·compile events104 수집 |
| triplet-readonly | exit101,6.361010초 | strict triplet 통과 후 AVX512_VBMI에서 거부;90초 timeout이 아님 |
| original-plain/native-plain/original-tapped/native-tapped | 미실행 | 두-frame 출력 비교/관측 영향/새 mismatch 결과 없음 |
| step14/15/16 | success | source/index·whole private SDK·immutable provider 및 child recovery 후검증 성공 |
| step17 qualification | failure | step13의 QUALIFY_OUTCOME=failure를 그대로 거부; 두 번째 독립 수치 실패가 아님 |

실제 로그 API의 본문은 `structuredContent.content`에서 읽었다. 마지막 stack은
`Controller.run → self.test(... READONLY ... 90) → command`의 `triplet-readonly: exit101`다.
step17은 먼저 QUALIFY/SOURCE/SDK/SEALS outcome이 모두 success인지 assert한다. 이 실행에서는
QUALIFY만 failure이므로 그 첫 assert에서 실패하며, 존재하지 않는 comparison을 읽어 생긴 새
metadata 오류가 아니다. 두 원인으로 중복 계산하지 않는다.

`final-seals.json`1574B/SHA256 `ed207bb80c48ef19fbd48c267557a7bb0f0e3e59b988b405393750ad1c11456f`는
source425/phase=triplet-readonly/pass=true/failures=[]/all_owned_children_reaped=true다.
children의 subreaper=true/live=[]도 확인했다. step18 upload와19/20 소유 cleanup 성공은
후검증·보존 결과이며 모델 gate 성공으로 승격하지 않는다.

## 실제 실패 image와 정책 경계

`model482/triplet-readonly.stderr`는 다음 원인을 남긴다.

```text
cached program 3: unadmitted cached instruction extension: AVX512_VBMI
ip=0xc2; code=EVEX_Vpermt2b_zmm_k1z_zmm_zmmm512
encoded_bytes=[62, 52, 7d, 48, 7d, d9]; cpuid_features=[AVX512_VBMI]
```

`triplet-readonly.json`52104B/SHA256 `17710b2e7c5c195d26eaae138faced468a7f36b46f3e0d3db25084b308d9ce62`는
strict_triplet_all=true/triplet_error=null, complete_roster91/calls176,
host_isa_all=false, generated_code_executed=false/numeric_acceptance=false를 기록한다.
실패 program3은 kernel4/compile event97이므로 `compile-3`을 임의로 조사해서는 안 된다.

| event97 보존물 | bytes | SHA256 |
| --- | --- | --- |
| `retained/triplets/compile-97.ll` | 3748 | `9ecb97976bee09a7e2befb2bacd1640bc3e971eb5c5eea02374bfe11a0b3f33d` |
| `retained/triplets/compile-97.o` | 3416 | `5c5faa5dd3688d16203bdae6596ad785141ceb9d8d71b2a42f004f4a4d719b94` |
| `retained/triplets/compile-97.bin` | 2688 | `609c6374473182cf77151ff9504197c3dab31d27544c81c4376a689c700a0689` |

인증된 text extent는2506B/alignment64, pointer7/scalar1/worker2/core_id0이다.
작성자도 image offset `[0xc2,0xc8)`를 inert byte read해 `62527d487dd9`를 확인했다.
실행이나 새 명령 삽입은 하지 않았다.

완전한 informational census는91개/text668011B/instructions110327, all_decoded_valid=true/
invalid_instructions0/complete=true다. 관측된 feature15개는
`AVX,AVX2,AVX512BW,AVX512DQ,AVX512F,AVX512VL,AVX512_VBMI,BMI1,CMOV,FMA,INTEL386,INTEL8086,MULTIBYTENOP,SSE4A,X64`
다. 현 feature_name의 ABI baseline/명시 mapping과 대조하면 미지원은
**AVX512_VBMI 하나**다. host가 가진 다른 VBMI2/GFNI/BF16 등을 필요 ISA로 승격하지 않는다.
census는 is_isa_admission=false이며 전체 branch/control-flow·host admission을 대신하지 않는다.

exact source425의 `cached_isa.rs` SHA256은
`d2e3c05a91ee7316d2787815aaf847f79b898d4d3550d6f978a5736b47cc27a8`다.
feature_name에는 AVX512F/VL/DQ/BW/FP16이 있으나 `CpuidFeature::AVX512_VBMI` mapping은 없고,
current_host도 `avx512vbmi`를 조회하지 않는다. 그래서 실제 host 지원 여부를 판단하기 전에
기존 unknown-extension fail-closed 분기에서 멈춘다. 현 정책이 정상적으로 거부한 것이며
오류를 숨기거나 unknown extension을 일괄 허용할 근거가 아니다.

## 실제 host와 공식 runtime detection 근거

hosted-cpu는 x86_64/logical4/Microsoft hypervisor/AuthenticAMD,
`AMD EPYC 9V74 80-Core Processor`를 기록한다. lscpu에는 avx512vbmi가 있고,
original compiler는 자연 target `znver4`, features에 +avx512vbmi/+avx512vbmi2와
-avx512fp16을 기록한다. triple은 x86_64-none-unknown-elf, passes=default<O2>다.
LLVM provider는 `/usr/lib/llvm-18/lib/libLLVM.so.1`/SHA256
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`다.

현재 Rust host snapshot19개에는 avx512f/vl/dq/bw 등이 있지만 vbmi는 없다. 이는 위 query가
구현돼 있지 않기 때문이지 이 자료만으로 host가 VBMI를 지원하지 않는다는 뜻은 아니다.
lscpu와 LLVM의 feature 문자열도 단독 실행 허가로 쓰지 않는다. 이 run에는 Rust의 직접
`is_x86_feature_detected!("avx512vbmi")` 관측이 없으므로 그 값까지 실제 수용됐다고 쓰지 않는다.

공식 Rust1.94 source를 inert read했다.

- [arch/x86.rs](https://github.com/rust-lang/rust/blob/1.94.0/library/std_detect/src/detect/arch/x86.rs#L182)는
  literal runtime feature `avx512vbmi`를 지원한다.14503B/SHA256
  `a3e874f324167c5a0aa4854666d4d57a6f9d8b2f3730145413a9dfeb7c486e0b`다.
- [os/x86.rs](https://github.com/rust-lang/rust/blob/1.94.0/library/std_detect/src/detect/os/x86.rs#L165)는
  XSAVE/OSXSAVE 뒤 XCR0의 AVX/SSE mask6과 AVX512 maske0를 확인한다. 그 중첩 경로에서
  FMA/F16C와 함께 CPUID leaf7/subleaf0 ECX bit1을 VBMI에 대응한다. 즉 합친 state mask는e6이며
  raw CPUID bit만 확인하는 방식이 아니다.16071B/SHA256
  `86116b52d471341868eebdf0185aef358716e9423cbe5b71e2a6974fd7bc4bc8`다.

이 공식 구현의 확인은 현재 readonly조사의 근거이며 detector나 개별 명령을 로컬에서 실행한
것이 아니다. 새 정책 구현을 한 것도 아니다.

## 이전561f signed-zero 및425 COPY 수용과 구분

이 run은 historical frozen1344 PKL을 쓰는 `model_pr1344_replay_482.py`가 아니다.
`model_current_triplet_482.py::collect`는 같은 GHA host에서 compile_modeld.py를 실행해
1928/1344용 새 PKL을 만들고1928 graph를 선택한다. request의 새 PKL은72248838B/SHA256
`d245099a7bf1f248b47f9d518e9509b868db0aac4080477d509febc1e5944fcf`이며, 역사 frozen PKL
`487ef869…`와 다르다. 원래 계획된 two_frames도 이 새 PKL을 읽고 current CPU export를 비교한다.

따라서 이전561f의 actual half8000·output99·CapturedJit retained BINARY 대 fresh image 분석을
이번 새 수치 실패의 실제 관측처럼 옮겨 적지 않는다. 이번에는 frozen D1 비교 자체가 없고
B의 whole output/tap 결과도 없다. readonly receipt의 generated_code_executed=false는 이
metadata selector의 범위다. 앞선 GHA fresh-original compile/capture는 summary상
original_capture_executes_generated_code=true였으므로 **run 전체의 모델 code 실행0**이라고
표현하지 않는다. 이 조사자의 로컬 모델/oracle 실행0과 구분한다.

source425의 staged compiler-export COPY 수용은 offline_schedule의 emit_schedule → native
export-plan 연결을 genuine COPY UOp로 확인한 별도 범위다. 그 새 검사 원문은 모델/kernel/provider
실행을 하지 않는다고 명시한다. actual425 current-triplet의 수집은 compile_modeld/original
CPULLVM 경로이고, 예정된 CPU export는 model_export.compile.export_cpu다. 이 helper와
model_export의 __init__/compile/original/pipeline 및 guarded_add/cached_isa는425 직전 source와
같다. cpu.rs의 변경도 private driver-prestate module 등록뿐이며 기존 kernel invoke 본문에
수학식 변경을 넣지 않는다. Cargo의 float_roundtrip 추가는 별도 JSON fidelity 변경이다.

COPY branch 성공을 full CPU model 수치 수용으로 확대할 수 없으며, 이번 원본 LLVM image의
VBMI 거부를 새 staged COPY planner의 산술 결과라고 볼 source 연결도 없다. 이번 실제 triplet
metadata는 전부 인증됐고 실패의 정확한 분기는 unchanged ISA mapping이다.

## 원인 해결에 필요한 최소 후속안 — 이번에는 미적용

동일425 source를 다시 실행해 같은 unsupported-extension 실패를 얻을 필요는 없다.
이미 complete91 census·SOURCE/object/image와 실패 명령이 보존돼 있어 추가 tap·원본 수치
재계산 없이 ISA coverage의 한 곳을 고칠 근거가 있다.

현재 금지된 소스/ISA/pin 변경은 하지 않는다. 부모가 별도 bounded 수정으로 검토할 최소 scope는
`rust/crates/model-runtime/src/cached_isa.rs`의 기존 feature_name/current_host 및 cfg(test)뿐이다.
관측된 `AVX512_VBMI`를 `avx512vbmi`에 대응하고, literal
`std::is_x86_feature_detected!("avx512vbmi")`로 CPUID와 OS-state 조건을 유지해야 한다.
현재 필요하지 않은 VBMI2/GFNI/기타 확장을 같이 넣지 않는다.

최소 회귀의 입력은 이번 실제6B와 synthetic ret이며 **decode/admission만** 한다.
capable snapshot의 해당 requirement 확인, AVX512F 등만 있고 VBMI가 빠진 snapshot의 거부,
current_host와 공식 detector의 대응 및 기존 unknown-extension/마지막-roster 거부를 유지한다.
실제 unsupported 명령 실행이나 CPU forcing은 필요하지 않다. 적용 뒤 필요한 GHA에서는
현재 host의 정식 detector 값과 전체91 strict ISA gate를 받아야 하며, census만으로 통과를
가정하지 않는다. 수정/pin 통합 전에는 재실행을 제안하지 않는다.

그 다음에만 기존 two-frame original/native whole bytes·plain↔tap을 수용할 수 있다.
이 audit는 그 다음 수치 결과를 예측하거나 frozen expected/guard rewrite/tolerance/timeout을
바꿀 근거를 제공하지 않는다. full128/full-model runtime/historical provider/device 수용과
retirement credit은 여전히 미완료/0이다. 이전 owned history는 그대로 보존했다.

## 2026-10-26 승인된 최소 VBMI admission 구현 — GHA 검증 전

위 감사 뒤 사용자가 `cached_isa.rs`의 명시 mapping/current_host/기존 unit 모듈에 한정해
구현을 승인했다. 앞 절의 미적용 상태는 최초 readonly 감사 시점의 기록이다.
이번 소스 변경은 production3줄과 같은 파일의 unit 회귀70줄 추가뿐이다.

- `CpuidFeature::AVX512_VBMI`만 `avx512vbmi`에 명시 대응한다.
- `current_host`는 literal `std::is_x86_feature_detected!("avx512vbmi")`를 조회한다.
  위 공식 Rust1.94 원문의 CPUID(7,0).ECX[1], XSAVE/OSXSAVE, XCR0 mask0xe6 및
  AVX512 state/FMA/F16C 중첩 조건을 그대로 사용한다. raw CPUID 대체나 강제 host 값은 없다.
- 기존 unknown-extension 거부, 다른 feature mapping/host 조회, instruction/control-flow/
  image/roster/전체 extent 검사와 기존11개 test 본문은 바꾸지 않았다.

추가 회귀는 다음3개다. 모두 CPU machine image 실행을 하지 않으며, 첫 두 검사는
inert bytes의 iced decoder 및 명시 host set에 대한 admission만 호출한다.

1. `cached_isa::instruction_isa::tests::observed_avx512_vbmi_image_requires_its_own_host_feature`
   는 actual425 compile97 image `[0xc2,0xc8)`의 `62527d487dd9`에 synthetic ret만 붙인다.
   iced code `EVEX_Vpermt2b_zmm_k1z_zmm_zmmm512`/길이6/`AVX512_VBMI` 요구와
   proof의 단일 `avx512vbmi` 요구를 확인한다. 다른 기존 AVX512 feature가 있어도
   VBMI만 빠지면 `host lacks avx512vbmi`로 거부해야 한다.
2. `cached_isa::instruction_isa::tests::avx512_vbmi_at_roster_end_and_after_return_preserves_other_requirements`
   는91개 중 마지막 image의 return 뒤에 실제 VBMI6B와 기존 AVX512F/VL witness를 둔다.
   unreachable bytes도 모두 검사하고 VBMI/F/VL 각각 누락 시 program90에서 거부하며,
   마지막 image가 빠진90개 prefix도 거부해야 한다.
3. `cached_isa::instruction_isa::tests::current_host_avx512_vbmi_matches_os_aware_rust_detection`
   는 host snapshot과 공식 Rust detector의 VBMI 값을 대조한다. 검출만 하며 cached 명령을
   실행하지 않는다. 실제 detector 결과는 아직 이 소스로 실행/수용되지 않았다.

현재 checkout의 정확한 selector `cached_isa::instruction_isa::tests::`는 변경 전11개,
변경 후14개다. 이번 파일에서18개인 baseline은 관측되지 않았다. x86_64 Linux에서
`cached-program-isa` feature를 켠 기존 lib target을 그대로 사용하며 별도 registry/Cargo/lib
등록은 필요 없다. 아래 명령은 부모 GHA 통합용이며 로컬에서는 실행하지 않았다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-model-runtime --features cached-program-isa --lib cached_isa::instruction_isa::tests:: -- --nocapture --test-threads=1
```

부모가 갱신해야 할 기존 고정 count 소비자는
`tools/ci/model_pr1344_replay_482.py`의 `current-isa-feature-tests` 결과11과
`.github/workflows/python-rust-523-history-image.yml`의 `isa-controls` 결과11이다.
각각14로 통합해야 하며, 이번 담당은 두 파일을 수정하지 않았다. 현재 triplet workflow의
unit 연결과 source pin도 부모 소유다.

로컬 확인은 Rust1.94 `rustfmt --edition 2021 --check`의 구문·서식 및 `git diff --check`,
소스 역투영뿐이다. 새73줄을 제거하면 기존 소스 전체 SHA256
`d2e3c05a91ee7316d2787815aaf847f79b898d4d3550d6f978a5736b47cc27a8`가 정확히 복원됐다.
새 witness의6B도 기존 보존 ZIP의 실제 image slice와 inert read로 일치시켰다.
변경 후 `cached_isa.rs`는24279B/SHA256
`a62a9eca723c1133462831f0ecb847cb2cb8845acd86e6504e6a0d95d070cf76`다.

Cargo/build/test/Clippy/model/oracle 및 새 GHA 실행은0이다. 실제 natural host의 detector와
14개 unit PASS, 새 source의 complete91 strict ISA gate 및 뒤따르는 B whole-output 비교는
부모의 다음 GHA에서 확인해야 한다. 이번 구현만으로 수치 실패 해결이나 model 수용을
주장하지 않는다. expected/math/tolerance/기타 ISA 확장/registry/WF/Focus/pin은 변경하지 않았다.


## 부모 호출부 폐쇄

current-triplet helper는 같은 source에서 선택한 기존 held library-test ELF로 ISA14와
세 VBMI selector의 정확 이름/성공을 먼저 확인한 뒤 기존 cached-code15 및 whole91
triplet/ISA/두-frame 원본 비교로 진행한다.60초 상한이며 기존 모델 math/expected/float·raw
비교, fresh source compile, provider/SDK/children/finalizer·cleanup은 바꾸지 않는다.
PR1344 helper와 history workflow의 ISA count도11→14로 맞췄다. base helper를 사용하는
PR1344의 BASE_SHA·각 WF의 helper 환경과 outcome hash가 같은 새 source를 가리키도록 닫았다.
두 진단의 push path에도 `cached_isa.rs`를 등록해 변경이 실제 검증을 호출한다.

순서가 올바르다는 정적 역투영과 AST/YAML/Bash/rustfmt만 확인했다. 실제 source SHA의
unit14/Clippy/whole admission·모델 수치 결과 및 #593 완료 판정은 GHA 원문 감사를 기다린다.

## #593 실제 ISA 경계 완료 증거 — ee74, 2026-10-26

source `ee74a75ffa4762397e3498d597e78135ef015d90`의 아래 두 실행에서 ISA14와 strict
model Clippy, complete fresh91/calls176 SOURCE/object/image 및 natural host ISA admission이
모두 성공했다. 두 receipt의 `strict_triplet_all=true`, `host_isa_all=true`, `isa_error=null`이다.
ISA14 stdout SHA256은 양쪽 모두
`ab478b5aa71e2a49ff5be6437babcc7b3c11e7a173d53c9c8e22078787cc38c3`다.

| 실제 실행 | 자연 host/검출 | artifact/API bytes/SHA256 |
| --- | --- | --- |
| [current40232519102/job121287077015](https://github.com/bin9208/openpilot-rust/actions/runs/40232519102/job/121287077015) | Intel Xeon Platinum8573C, Rust avx512vbmi=true |12613144560 /37211660B /`fd96ce8e52c0164e405ff99ff7a0c7ef6ccc80e376ab2cecf20b9a9dd332f9b2` |
| [PR1344 40232519230/job121287077126](https://github.com/bin9208/openpilot-rust/actions/runs/40232519230/job/121287077126) | AMD EPYC7763, Rust avx512vbmi=false |12613814043 /27446372B /`ef2bbcee54a5827f98c697d0886c93f21047839626a9107e4c4cc92b0ef1a74f` |

각 ZIP의 API expected bytes/digest, safe unique canonical path/no symlink/special/encrypted
entry 및 전체 CRC를 확인했다. 각각1026개/expanded105264007B,2104개/expanded118415659B다.
두 실행의 source/index·SDK·독립 immutable/provider/child gate와 cleanup, 내부 final-seals의
pass=true/failures=[]/all_owned_children_reaped=true도 성공했다. PR1344 retirement Clippy도
성공했다. VBMI 후보 source SHA는 위 a62a9eca…와 같다. AMD에서 VBMI를 허위 활성화하지 않고
같은 decode-only 양성/host-missing 음성 및 detector 대응14개가 통과한 증거다.

current의 `triplet-readonly.json` SHA256은
`a67789c0a616d3b1b68a38c1b7c7d3b82093b734d648cdeb1c05659a0256cf81`, final-seals는
`b023e174208743d2e4f2d936aff732dbdc55270a06d1c7de18eb978378de3fe8`다.
PR1344의 readonly SHA256은 `0572b86edacbeb8df4fbacd7e2844ea5cd6aec42427e021d61063a470b0a37cc`,
final-seals는 `3245b09bebfc2e1d301b06ca0f50d50d53e0faff9e5c095f9c681a9980e02391`다.

별도 [historical40232519164/job121287076934](https://github.com/bin9208/openpilot-rust/actions/runs/40232519164/job/121287076934)도
ISA14는 성공했고 old image는 `host lacks avx512bw`로 정상 거부했다. 지원하지 않는 명령을
실행하거나 이 거부를 우회하지 않았다.

따라서 #593의14개/Clippy/natural Intel·AMD fresh91/후봉인 경계는 실제 증거가 완료됐다.
부모가 이 기록을 검토한 뒤 이슈를 닫으며, 이 담당은 이슈 상태를 바꾸지 않았다.

모델 수치 게이트는 별도로 실패 상태다. current step13/required17은 genuinely same-host
fresh PKL91의 frame1 model[100,101,1349]에서 original -0/native +0만 다르며 다른11개
output 파일은 같다. 원본/native plain↔tap은 같다. PR1344 step12/required16은 D2 전체6output
original/native 동일과 실제 half99 양쪽8000 뒤 기존 frozen99 -0/current 양쪽+0의 D1 sticky다.
두 numerical failure를 ISA 회귀로 분류하지 않고, full128/full-model/역사 provider·기기·
retirement 수용으로 승격하지 않는다. current fresh91을 old PR1344의 미확인20개 역사 image
context와 혼동하지 않는다. raw 출력 독립 비교와 상세 triage는 private scratch의
`ci/model-ee74-readonly-triage.md`에 보존했다.
