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
