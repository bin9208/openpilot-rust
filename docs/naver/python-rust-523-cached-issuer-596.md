# #596 cached issuer의 실제 host feature 어휘 정합

2026-10-26. 상태: production2줄·기존15개 중 음성검사 한 곳의 확장, GHA UNRUN.
#593의 decoder/ISA14와 #482의 모델 수치 수용은 별도다.

실제 sourceee74 currenttriplet의 자연 Intel8573C fresh91은 PROGRAM18에 AVX512_FP16을
요구하며 `cached_isa`의 OS-aware snapshot과 A complete91 ISA는 통과했다. source425의
EPYC9V74 fresh91에는 VBMI가 있었다. 그러나 `cached_code.rs::host_has`의 별도 closed match는
두 이름을 빠뜨렸고 새 C의 issuer/prepare는 이 함수를 사용한다. 이는 source 대조로 확인한
거부 원인이며 아직 새 C 실행의 오류를 관측했다고 쓰지 않는다.

수정은 정확 `avx512fp16`와 `avx512vbmi` 두 이름을 Rust1.94 표준 runtime detector에 연결하는
것이다. decoder가 이미 쓰는 literal과 같다. CPUID/OSXSAVE/XCR0 조건을 직접 대체하거나
CPU/ISA를 강제하지 않는다. 그 밖의 모든 이름은 기존 `_ => false`로 계속 거부한다.

기존 `omitted_final_program_abi_change_and_unknown_isa_reject`에만 검증을 보완했다.
기능 활성 Linux x64에서는 production host와 native decoder snapshot을 대조하고, 잘못된 철자·
VBMI2 등 미검토 어휘는 거부한다. 동일한 inert metadata 양성에서 FP16/VBMI 각각을 빠뜨린
두 host-missing 음성을 확인하며 기존 omitted/ABI/unknown 검사는 원래 요구 목록으로 복원한다.
이 unit은 machine image를 실행하거나 실제 host feature를 강제로 활성화하지 않는다.
selector 개수15와 실제 instruction decoder14는 그대로다.

default `CpuModel::load`의 cached 거부, 독립 Admission/source/object/whole-image/ABI/issuer
폐쇄, 기존8-source guarded-add 및 기대 bytes·허용오차·compiler 수학은 바꾸지 않았다.
진단 C는 별도 명시 admission으로 같은 원래 full91 image를 시험하며 A/B 실패는 유지한다.
source-only Rustfmt2021/diff만 확인했다. 다음 exact-source Actions의15/14·Clippy·natural host
full91 issuer/package/seal/후봉인과 실제 수치 결과를 분리해 확인한 뒤 #596을 판정한다.
전체128frame/historical half·sNaN·모델/runtime·장치·전체 Python 퇴역 credit은0이다.


## ef172 exact-source 완료조건 3개 감사 — 2026-10-26

[#596](https://github.com/bin9208/openpilot-rust/issues/596)의 현재 세 완료조건을
source `ef17256300d8caf51d5b0a9b3a900fd2c9615d01`의
[run40245597205/job121326369244](https://github.com/bin9208/openpilot-rust/actions/runs/40245597205/job/121326369244)
실제 증거와 대조했다. **이 이슈가 정의한 production host 어휘 누락 경계의 세 조건은 충족한다.**
이슈 상태 변경과 최종 종료 판단은 부모 담당이며, run 전체 failure와 #482/#523 미완료는 유지한다.
앞부분의 GHA UNRUN은 당시 후보 상태이며 아래 실제 결과와 구분한다.

James가 이미 다운로드·전체 SHA/1283 CRC 감사한 artifact12618762234의 기존 캐시만 읽었다.
42,345,579B, API/James 감사 SHA256
`71209f8053826ef05fe0e2d2cb514649a73a65f52a884f2b5bcbcd63fbe69364`를 인용한다.
재다운로드·전체 archive 재해시·전체 CRC 재검사는0이며, 필요한 JSON/stdout/소스와 일부 bound
container/library member만 읽었다. 원본 whole91 및 C raw12의 전체 대조는
[James의 기존 감사](python-rust-523-model-cached-context.md#actualef172-감사--c12-실제-pass-b-strict-세-signed-zero로-전체-fail-유지)를 재사용한다.

| 완료조건 | exact-source 실제 근거 | 판정 |
| --- | --- | --- |
| 기존15/ISA14/strict Clippy, unknown/host-missing 및 전후 봉인 유지 | cached-code-controls stdout15 passed/0 failed/0 ignored, ISA14 passed/0 failed/0 ignored. current-clippy는 cached-program-isa 구성의 lib/tests `-D warnings`, exit0. source/index·SDK files/roster/programs/rejected 전후 bytes 동일, final-seals pass=true/failures=[]/all_owned_children_reaped=true | 충족 |
| 자연 host의 필요한 feature가 양쪽 경계에서 일치하고 full91 issuer/package/seal의 해당 거부 해소 | 자연 Intel8573C에서 FP16/VBMI detector 양성. production host_has와 decoder snapshot 대응 검사가 실제 PASS. FP16 요구 image를 포함한 독립 whole91의 preflight/package/seal status 모두 exit0, 뒤의 실제 C load_cached/prepare도 완료 | 충족 |
| 모델·whole128·historical half/sNaN·기기/default 정책과 분리 | C는 별도 명시 admission이며 default_cached_disabled=true/b_failure_can_be_promoted=false. C12 동일 뒤에도 B의 세 signed-zero strict 불일치와 전체 run failure 유지. full128/full-model/historical 수용 및 retirement credit0 | 충족 |

### 실제 detector 양성과 production 호출부

hosted-cpu.json은 x86_64/4 logical CPU/`INTEL(R) XEON(R) PLATINUM 8573C`다.
A triplet-readonly와 C cached-isa의 OS-aware host_features에는 `avx512fp16`와 `avx512vbmi`가
둘 다 있다. test ELF build와 Clippy 모두 `--no-default-features --features cached-program-isa`다.
따라서 기존15개 안의 `omitted_final_program_abi_change_and_unknown_isa_reject`의 feature-gated
production `host_has(feature) == cached_isa::current_host().contains(feature)` 검사도 실행됐다.
이는 이 host에서 두 정확 feature 이름이 production 경계에서도 true였다는 근거다.

production `cached_code.rs` SHA256은
`f6963fbf61b4463f80a88bd8d2d4a02f42b0c28cfa73fb9d66910b30022ddc16`,
`cached_isa.rs`는 `a62a9eca723c1133462831f0ecb847cb2cb8845acd86e6504e6a0d95d070cf76`다.
James의 published-source 기록과 현재 두 소스 bytes가 일치함을 확인했다.
`CpuModel::load_cached` → `load_inner(Some(admission))` → `cached_code::prepare` →
`validate(..., host_has)`는 dlopen 전에 실제 production 검증을 수행한다.
C native selector가 이 경로를 지나 두 frame/기존 original12 동일까지 성공했으므로,
production host_has의 FP16 누락 때문에 image를 거부하던 경계는 실제 닫혔다.

이번 ISA proof의91개 중 FP16 요구는18개이며 original_index18도 포함한다.
전체 A/C의91개 index/required/instructions/text_bytes가 같고 독립 admission의91개 metadata와
certificate의 program.original91개도 같다. preflight/package/seal의 request SHA가 각각 status에
결합되고 모두 exit0이다. retained container496176B와 library526352B의 실제 SHA도 certificate와 같다.
issuer status의 generated_code_executed=false/libraries_loaded=false는 issuer 단계의 속성이다.
뒤의 C native 실행은 cached_runtime_enabled=true이며 이 둘을 혼동하지 않는다.

**VBMI는 detector/production vocabulary 양성까지 실제 확인했다.** 이번91개 image의 required
union에는 VBMI가 없으므로, VBMI 명령을 요구하는 full91 모델 실행까지 완료했다고 쓰지 않는다.
이 이슈의 필요한 요구사항 수용과 어휘 대응 조건은 충족하지만 모든 ISA/모델 검증으로 확대하지 않는다.

### unknown·host-missing 음성 및 B sticky 경계

실제 PASS한 기존15개 본문은 `avx512_fp16`, `AVX512_VBMI`, `avx512vbmi2`,
`unadmitted-future-extension`을 production host_has에서 거부한다. inert metadata 양성에서
FP16/VBMI를 각각 하나씩 지원 집합에서 뺀 두 음성도 production validate에서 거부한다.
기존 omitted PROGRAM/ABI/unknown 및 전체 unavailable 음성도 유지된다.
ISA14에는 FP16/VBMI observed-image·host-missing·unknown 검사가 포함돼 있다.
이는 unsupported requirement를 거부하는 실제 회귀 증거이며, 이번 자연 양성 host를 물리적으로
unsupported host로 바꾸거나 별도 미지원 CPU에서 machine code를 실행한 증거는 아니다.

C receipt는 complete_roster91/frames2/existing_original_all12_equal=true/mismatches=[]이며
new_original_executions=0/new_taps=0이다. B comparison은 original_native_bytes_equal=false,
model-1.bin의 float32 indices100/101/1349에서 original -0/native +0를 보존한다.
step13의 마지막 오류는 `strict original/native byte mismatch retained; diagnostic is not numerical acceptance`다.
C 성공으로 B 실패나 full128/full-model/historical half·sNaN/기기 수용을 승격하지 않았다.
기본 `CpuModel::load`는 명시 admission 없는 cached graph를 계속 거부한다.

핵심 receipt SHA256:

- cached-code-controls.stdout: `89bdb6b10ea2beff93dd35044665716e6c2bb459fb7de6a195885ee3c83bd24d`
- current-isa-feature-tests.stdout: `b3ae4772c2fb0a3fcb966f607e1a9b858935f778119fe1c8f852eb5eba262081`
- cached-context.json: `14a51416524aa4a1c6345a2d3914eb1a3b307ce5664ab8c4c799b347703de511`
- final-seals.json: `2048830bd67d58d8d2fc6e0897f85ac462b9a16027d32800e755d0d037fbf54b`

추적 변경은 이 문서 append뿐이다. issue/코드/WF/CI/CPU/expected/수학/default 정책 변경,
로컬 기능·Cargo/build/test/Clippy·원본/모델 실행, stage/commit/push는0이다.


부모는 위 세 완료조건의 실제 충족을 확인해 [#596](https://github.com/bin9208/openpilot-rust/issues/596)을
production host 어휘 누락 해결로 종료했다. [종료 근거](https://github.com/bin9208/openpilot-rust/issues/596#issuecomment-6405171792)는
sourceef172와 같은 실제 job/artifact에 결합했다. #482/#523 및 전체 수치/기기 조건은 유지한다.
