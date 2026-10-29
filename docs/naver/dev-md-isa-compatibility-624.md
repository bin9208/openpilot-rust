# dev-md 고정 수치 라이브러리의 runner ISA 호환성 — #622/#624

## 범위와 상태

- 대상: [PR #494 → dev-md](https://github.com/bin9208/openpilot-rust/pull/494), 기준
  `868ea1cfff9fb46606a5e814a776b0ea93bff820`.
- main 트랙의 공통 수정 `c7e7c80deafc729839e163f3022dd197d7cc52a9`과 후속
  `a5e2d10f7ea882f7edf4453c481c8dc5b5f9ffb5`에서 확인된 원인만 이식한다.
  dev-md의 upstream source fixture와 고정 builder 원문은 유지한다.
- 실행 중 [40807533439](https://github.com/bin9208/openpilot-rust/actions/runs/40807533439)는
  기준868 소스의 검사다. 이 작업은 해당 실행을 취소·재시작하지 않았고 push/dispatch도
  하지 않았다. 그 결과를 이 수정의 성공으로 대신하지 않는다.
- 기능·빌드·테스트는 로컬에서 실행하지 않았다. 새 정확 SHA의 Actions 검증 전 상태다.

## #624: 같은 NumPy dispatch와 다른 complex product 반올림

원래 실패는 [estimation job123254262542](https://github.com/bin9208/openpilot-rust/actions/runs/40814449746/job/123254262542)이다.
원격 원문 로그를 읽어 masked-constant holes의 원본0.8660254037844387/Rust0.75,
periodic의1/0, sparse delay의0.4/0.8500000000000001을 확인했다.

868의 다음 파일은 main 수정 직전 소스와 byte가 같다.

| 파일 | SHA-256 |
| --- | --- |
| `rust/crates/lagd/src/correlation.rs` | `f2a84b59b09e74418882b13ca9b6caace63fa67fc26ea54cf2ea3a06633ea589` |
| `rust/tools/check_lagd_numeric.py` | `ad2f441856234a366b12e20b7e37dbe4c1742e3a95d4f44c2e5fc6d41fea145d` |
| `rust/crates/pocketfft/provenance.json` | `8d3372b9d73b0d6400c49ba41247154da2d624f527546dffe7a886d8f4e01fcc` |

원래 NumPy pin은2.5.3 / `dd88c0c19b54ad9ed3533224221285bf0873249a`다.
해당 Git source의 `numpy/_core/src/umath/loops_arithm_fp.dispatch.c.src`,
`numpy/_core/src/common/simd/avx2/arithmetic.h`, `meson_cpu/x86/meson.build`를
읽어 complex multiply와 X86_V3의 AVX/AVX2/FMA3/BMI/BMI2/LZCNT/F16C/MOVBE
조건을 대조했다. 등록 runner Xeon E3-1230 V2는 AVX2/FMA가 없어 X86_V2의
분리 반올림 경로를 사용하지만868 Rust는 항상 `mul_add`를 사용한다.

공통 수정과 동일한 CPU dispatch를 적용한다. X86_V3와 ASIMD는 융합, 나머지는
곱셈 후 덧셈/뺄셈의 별도 반올림을 사용한다. 두 결과를 구별하는 cancellation
회귀를 유지하고 수치 결과 artifact에 NumPy CPU/ufunc dispatch를 추가한다.
PocketFFT·입력·mask·허용 오차·delay 정책은 바꾸지 않는다.

## #622: 같은 pinned Haswell wheel을 비교 호스트에서 실행하는 경로

[실제 GDB 진단](https://github.com/bin9208/openpilot-rust/actions/runs/40814444977/artifacts/12872915150)의
`libblasfeo.so:blasfeo_dvecse+35`, `vbroadcastsd %xmm0,%ymm1` SIGILL은 main 기록과
고정 원문으로 추적한다. 868의 `build_plannerd_acados.py`도 x64 wheel SHA
`3b451852e83d62815cead999ab31073db9be60307f772650336cdd4534f12b9e`의
`libblasfeo.so` 등 라이브러리를 직접 추출하며 CI의 x64 planner와 sanitizer가
이를 호출한다. 두 트랙의 `licenses/sources.json` 및 네 notice 원문은 byte가 같다.
즉 같은 runner에서 같은 wheel ISA 문제에 노출된다. 현재868 실행에서 이미 같은
실패가 났다는 주장은 하지 않는다.

main의 명시 CI 어댑터와 source SDK helper를 이식한다. AVX2/FMA가 있는 x64는
원래 wheel을 사용한다. 둘이 없는 x64만 원래 고정 acados/BLASFEO/HPIPM/qpOASES
revision과 gitlink·license를 검증해 AVX 호스트는 `X64_INTEL_SANDY_BRIDGE`,
그 밖에는 `GENERIC`으로 빌드한다. ARM의 wheel 경로는 그대로다. qpOASES의 실제
원문 SHA와 기존 `^L` 표시 notice의 SHA를 따로 검증하는 후속 #626 수정도 포함한다.
SDK는 등록 Actions job의 소유 임시 경로에만 보존하며 compiler/recipe/CPU/output
해시를 provenance와 artifact에 남긴다. 공유 cache나 다른 실행은 정리하지 않는다.

**dev-md builder는 main의 frozen builder와 다르다.** dev-md에는 필요한
`--source-fixture`·`PLANNER_SOURCE_FIXTURE` 처리와128MiB 작업 증가량 검사가 있다.
파일을 덮어쓰지 않고 기준868의 SHA
`4bab1a9e70be6d6b3c91fe592137bc9f3d89b3179aff53736f294e4fa604d5e9`를
어댑터에서 검사한 뒤 원래 main/전체 argv를 실행한다. `planner-source.json`,
wheel pin, notice, ARM 옵션과 원본 생성 수식은 유지한다. `ci.yml`의 기존 세
builder 호출에만 어댑터를 붙이고 host SDK에 필요한 cmake/make 및 기존 artifact의
SDK 진단 경로를 추가한다. main599의 입력 제공자·발행 기능은 이식하지 않는다.

## 검증과 다음 실행

로컬에서 AST4개, YAML 전체 job/step 비교, planner 셸의 `bash -n`,
`rustfmt --check --edition 2024`, `git diff --check`를 확인했다. 기존 job/matrix/
조건/검사 순서/원래 argv가 유지됐고, 명시 접두어·필요 도구·진단 경로 외의
workflow 변경이 없음을 구조적으로 대조했다. 원래 builder와 source/NumPy/
허용 오차/license pin의 bytes도 그대로다. 이는 기능 성공 근거가 아니다.

새 회귀 `tools/ci/tests/test_dev_md_planner_isa.py`는 기존 fast의 unittest discover에
포함된다. 고정 builder 변경을 실행 전 거절하는 경계, 호환 호스트의 원래 wheel
유지, 등록 Actions 밖의 source 빌드 거절, qpOASES 원문/notice의 이중 pin,
원래 세 호출의 전체 argv를 검사한다. 기존 lagd Rust 테스트에 cancellation 회귀가
포함된다. 이 회귀들은 작성만 했으며 로컬에서 실행하지 않았다.

현재868 실행을 보존한다. 부모가 그 결과를 확인한 뒤 dev-md 전용 PR에 이 커밋을
반영하고, 새 정확 SHA에서 기존 fast·estimation의 전체 원본 수치 비교·planner
x64/ARM 및 sanitizer·필수 gate를 실행해야 한다. 기존 허용 오차나 validity/IPC
검사는 생략하지 않는다. 정상 병합·병합 후 검사·정확 source의 설치 후보와 사용자
기기 수용도 별도 조건이다. 현재 수정은 로컬 준비이며 전체 완료를 뜻하지 않는다.
