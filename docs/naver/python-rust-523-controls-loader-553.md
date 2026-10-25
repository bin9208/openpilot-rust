# #553 native loader 환경 A/B 최소 진단

## 목적과 현재 근거

2026-10-25. 602 normal과 현재 diagnostic의 구체적인 환경 차이인 `LD_LIBRARY_PATH` 한 변수만
대조한다. root 수정이나 수치 수용이 아닌 진단 후보이며 실제 결과는 Actions UNRUN이다.
현재202a에서는 original/native4호출·2329bit fields가 같고 native prefix600+target1도 one-frame과
같지만602 native의54ULP 경계는 재현하지 못했다. 실제 BLAS entry/kernel과 역사 provider 전체
closure는 미관측이며 이 변경으로 관측했다고 바꾸지 않는다.

기존 diagnostic native env에는 private numerics 경로의 `LD_LIBRARY_PATH`가 있다.
602 normal controller/trace CommandSpec에는 해당 변수가 없다. 다른 수학 source/profile/
held provider가 같다는 사실만으로 이 차이 또는 CPU가 원인이라고 확정할 수 없다.

## 구현 계획

1. 기존 original/plain/tapped 및 prefix 실행·관측·기대값을 그대로 둔다. 기존 재현 FAIL 검사 앞에
   별도 plain A/B2실행을 추가한다. 같은 VM·held plain ELF·같은 held one-frame 입력 bytes를 쓴다.
2. A는 기존 native env 그대로, B는 그 dict에서 `LD_LIBRARY_PATH` 한 키만 제거한다.
   다른 key/value와 cwd/stdin/argv/ELF를 바꾸지 않는다. getter/alignment/kernel 관측은 추가하지 않는다.
3. 별도 fresh0700 `native-loader-ab/` 아래 고정 absolute `native-result.json` 하나를 양쪽 argv에
   똑같이 준다. 기존 held `native-plain.json` 등은 건드리지 않는다. 매 실행 전 고정 경로가
   dangling symlink까지 포함해 absent인지 확인한다.
4. 종료·자손 회수 뒤 출력이 있으면 nofollow FD/identity/소유권/nlink1/16MiB read/SHA를 확인하고,
   같은 private directory의 `with-ld.json` 또는 `without-ld.json`으로 rename한다.
   rename 전후 inode/type/UID/GID/mode/size/nlink/mtime/bytes를 확인하고 rename에 따른 ctime만
   사후 identity로 구분한다. rename 뒤 hold하고 다음 실행은 같은 고정 경로 absent에서 시작한다.
5. 성공 출력은 기존 `loads()`/`bits()`로 읽는다. JSON 타입/F64 bits/전체 결과와 state를 그대로
   보존해 A/B, 기존 native-plain, frozen state,602 state와의 관계를 기록한다.
   새 출력 파일 이름과 기존 native-plain의 차이를 loader 효과로 단정하지 않는다.
6. 보통의 양수 nonzero exit는 개별 stdout/stderr/exit/error 및 있으면 partial output을 보존한다.
   timeout/예산/입력·provider·output identity/회수 실패는 일반 loader exit로 삼켜 계속하지 않는다.
   B가 loader 실패해도 원문과 실패 receipt를 남기며 최종 진단 성공으로 세지 않는다.
7. 기존 command600초/original700초/전체52분/helper60분/job75분과512MiB child fsize,
   proof1GiB/기존 전체 growth admission·최종 source/SDK/held/provider·자손 회수 조건을 유지한다.
   A/B가602와 같아져도 기존 native-plain의602 미재현 FAIL을 대신 통과시키지 않는다.

## 수정 범위와 검토 기준

- `tools/ci/controlsd_diagnostic_553.py`: controller의 별도 loader A/B 메서드와 기존 진단 결과에
  연결하는 구간만 추가한다. 기존 수학·원본 worker·expected·tap16·prefix·normal40는 바꾸지 않는다.
- `.github/workflows/python-rust-523-controls-diagnostic.yml`: helper literal SHA1개만 갱신한다.
- 이 한국어 문서. Focus/공유 등록·Cargo·기존 normal helper/원본/model은 수정하지 않는다.

동일 argv/common env/held 입력·ELF와 directory FD를 확인하고 모든 새 command도 기존
subreaper/pidfd/PGID 종료·회수와 전역 deadline 아래에서 실행한다. 원래 코드의 재현 need 두 개는
그대로 두고 A/B command/output 실패도 별도로 실패시킨다. divergence 자체는 수치 수용이 아니다.
private directory 내 단일 controller/회수된 child 경계를 사용하며 hostile same-UID 배제를 주장하지 않는다.

## 검증 및 후속

먼저 이 계획을 기록한 뒤 controller의 `loader_environment_comparison`과 기존 결과 연결을 구현했다.
로컬은 AST/문법/정적 source 비교/diff만 확인했다.
원본·모델·PKL·프로젝트·새 A/B 코드를 로컬에서 실행하지 않는다. 실제 A/B 출력·loader exit·
성능·전후 봉인 및 회수는 새 Actions에서 확인해야 한다. `actual_blas_call_observed`,
`actual_kernel_observed`, `complete_provider_closure`와 retirement credit은 기존 false/0을 유지한다.

정적 확인: helper AST, WF YAML/run13개 Bash 문법/DIAG Python AST와 scoped diff를 확인했다.
새 A/B 메서드·연결 구간만 제거하면 기존 helper 전체가 HEAD와 byte 동일하다.
따라서 원본 worker/수학·bits parser/기대값/기존 plain·tapped·prefix/기존 재현 need 두 개/
command·budget·finalizer 본문은 그대로다. WF는 helper literal SHA1개 외 원문 byte가 같다.

새 proof 경로는 다음과 같다.

- `diagnostic-before-loader-ab.json`: A/B 전 기존 layer/prefix 진단 결과를 별도 보존·hold한다.
- `native-loader-ab/native-result.json`: 두 native 명령이 공통으로 받는 고정 absolute argv다.
  각 실행이 끝나면 `native-loader-ab/{with-ld,without-ld}.json`으로 옮겨 hold한다.
  raw bytes·inode·identity와 rename 전후 ctime을 구분해 기록한다.
- `native-loader-{with-ld,without-ld}.{stdout,stderr}`: 기존 command collector의 개별 raw channels다.
- `native-loader-{with-ld,without-ld}-receipt.json`: returncode/error/elapsed/회수 상태·보존 출력·
  실제 state bits·기존 plain/frozen/602 관계다. 보통의 양수 exit 실패와 없거나 잘린 JSON도 실패로 기록한다.
  signal/timeout/예산/FD·source·provider·회수 실패는 보통의 loader exit로 바꾸지 않는다.
- `native-loader-environment-comparison.json`: 공통 argv/cwd/env/held 입력·ELF, 유일한 환경 차이,
  두 결과의 bits 관계와 개별 실패를 보존한다. 기존 `diagnostic.json`/outcome에도 연결한다.

기존 native-plain과 새 A는 출력 argv가 다르다는 flag를 남긴다. 그 비교가 다르면 output path/배치 등
미분리 요인이 남은 것으로 읽고 loader 원인으로 단정하지 않는다. A/B끼리는 고정 argv가 같다.
추가 getter·alignment·새 kernel 관측·재시도는0이며 실제 A/B와 전후 봉인·회수는 아직 UNRUN이다.

## 2026-10-25 같은 loaded instance의 계산 후 getter 후보

이 절은 위 loader A/B와 별개로 승인된 진단5파일 구현의 후속 기록이다. c00
[run40154418251/job121052613887](https://github.com/bin9208/openpilot-rust/actions/runs/40154418251/job/121052613887)의
actual에서는 현재 original/native4호출·2329bit field와 각각의 plain/tapped 출력이 같고 LD A/B도
같았다. 그러나 original의 historical frozen state는 미재현이어서 기존 strict gate는 실패했다.
새 getter는 그 사실을 고치거나 역사 provider/core/thread를 소급 관측한 것으로 쓰지 않는다.

승인된 변경은 진단 helper, `control-policy/src/numerics.rs`, `controlsd_trace.rs`, 진단 WF의 helper
literal pin 및 이 문서5개다. 새 crate/feature/Cargo/lib/원본·expected 변경은 없다. 기존
`diagnostic-553` feature는 default-off다. production loader와 float/double matrix·dimensions,
`flux.rs` 전체 수치/관측 본문은 그대로다. getter는 추가 dot/warmup/setter/CPU envforcing 없이
config → core → thread 순서로 각각1회만 호출한다.

- original-tapped: 원래 adapter.trace의4회 계산과 결과 JSON/tap 원문 저장 뒤 관측한다.
  plain worker에는 추가하지 않는다. 계산 전 provider-before에 이미 존재하는 ctypes/_ctypes,
  libffi/loader, NumPy extension 및 고정 OpenBLAS가 없으면 FAIL이며 새 import/load fallback은 없다.
- `ctypes.CDLL(path, mode=NOLOAD|LAZY)`는 사용하지 않는다. CPython3.12.15의
  [callproc.c](https://github.com/python/cpython/blob/v3.12.15/Modules/_ctypes/callproc.c#L1420-L1445)가
  RTLD_NOW를 OR하기 때문이다. 이미 존재한 pythonapi handle에서 얻은 실제 loader 함수로만
  extension/provider의 정확 절대경로에 RTLD_NOLOAD|RTLD_LAZY를 전달한다. miss는 FAIL이며
  namespace/link-map/base, extension scope의 SGEMV/SDOT symbol 주소와 provider 주소의 일치,
  주소→mapping dev/inode→heldFD/SHA를 확인한다. 획득한 NOLOAD reference만 역순으로 반환한다.
- native-tapped: feature 전용 one-case/one-frame guard 아래 기존 Controls를 살아 있게 보유하고,
  publication/snapshot과 기존 output.flush 뒤 `controls.interface.numerics`의 `_library`만 빌린다.
  새 Library/dlopen/instance를 만들거나 handle 소유권을 이전하지 않는다. 이미 저장된 네 연산
  pointer와 getter3의 실행 map 및 dladdr image base가 같은 고정 DSO인지 검사한다.

고정 provider SHA는 `432e1c1739731468ba305771af96773939e43016643e2d630f58cb45b6670e96`,
wheel SHA는 `b7e18c623bb5c95acb3b3328861272816ba199fb531921c5d6d0b675f1fde9e3`로 유지한다.
실제 ELF export는 기존 inert 자료가 확인했고, ABI는
[OpenBLAS0.3.34 cblas.h](https://github.com/OpenMathLib/OpenBLAS/blob/v0.3.34/cblas.h#L42-L51)의
config/core `char *(void)`, threads `int(void)`를 따른다. ILP64 suffix를 thread 반환형 i64로
오해하지 않는다. upstream tag가 wheel0.3.34.106.0 patch source와 byte 동일하다는 주장은 없다.

반환 문자열은 NULL/빈 문자열/NUL 누락을 거부하고 읽기 가능한 단일 map 안에서 최대4096B만
복사한다. static config의 BSS가 inode0 anonymous map일 수 있으므로 문자열도 dladdr가 같은
image base를 가리켜야 한다. file-backed이면 추가로 held provider dev/inode/path를 요구한다.
함수 pointer에는 anonymous map 예외가 없다. 같은 file SHA만으로 서로 다른 loaded instance를
같다고 하지 않으며 original/native 서로 다른 PID의 주소가 같을 것을 요구하지 않는다.

[thread getter source](https://github.com/OpenMathLib/OpenBLAS/blob/v0.3.34/driver/others/memory.c#L479-L486)는
필요시 초기화를 거칠 수 있다. 따라서 관측 자체가 무할당·무초기화라고 선언하지 않는다. config의
MAX_THREADS는 build capacity이며 현재 thread 수와 구분한다. getter 전후 executable map roster,
PID/starttime·task roster와 provider FD identity/SHA를 기록한다. 이는 시점 관측이며 모든 과거
thread/allocator lifecycle이나 실제 BLAS entry/kernel의 관측이 아니다. native load가 원래부터
config를 호출했다는 점도 receipt에 명시한다.

새 proof는 다음 세 문서다. original/native receipt는 각1개·64KiB 이하이며 원문 getter byte array와
C int 값, source/phase/PID/starttime, 같은-instance binding 및 실패를 기록한다.

- `original-tapped-getters.json`: 결과/tap 저장 후 original 관측. NOLOAD miss/참조 반환 실패도 FAIL.
- `native-tapped-getters.json`: flush 뒤 stderr의 정확1개 `CONTROLSD_553_GETTERS` receipt를 보존·hold.
- `same-instance-getters.json`: 양쪽4 complete call·receipt 건강/FD/범위/false flags를 검증한 뒤
  config/core/thread가 각각 같은지 비교한다. 값이 다른 것 자체는 자료이며 억지로 맞추지 않는다.

기존 plain/tapped 전체 결과 bits 동일성 및 2329field 관측은 실제 Actions에서 다시 확인해야 한다.
receipt checkpoint는 기존 historical/602 reproduction need 이전에 저장되며 새 자료가 기존
재현 실패를 대신 통과시키지 않는다. prefix601/LD A/B·기대값/type/수학/tolerance/normal40·기존
command700/600초·전체52분/helper60분/job75분·모든 file/log/proof/growth 상한과 final source/SDK/
provider/owned-child 검사는 유지한다. getter/receipt 오류는 strict FAIL이며 원래 finalizer는
실패에서도 독립 실행된다. get_num_threads/core의 같은 값이 개별 dot의 과거 실행 경로를 입증하지 않는다.

새 native unit2개는 실제 Library/BLAS 결과를 흉내 내지 않고 pointer→mapping의 exact identity·
중복/미매핑 경계와 bounded NUL 문자열 거부를 검사한다. 기존 진단 Clippy는 production cfg 본문을
포함하지만 unit 실행 등록은 부모 후속이다. 정확 selector는 다음과 같다(모두 **UNRUN**).

```text
numerics::diagnostic_553::tests::getter_pointer_requires_unique_mapping_and_exact_provider_identity
numerics::diagnostic_553::tests::getter_strings_require_nonempty_bounded_terminator
```

로컬은 AST/Rustfmt/YAML/Bash/diff/source 비교뿐이다. getter/원본/native/PKL/빌드/test/Clippy/GHA
실행·stage/commit/push0이다. 이 후보의 실제 parity/provider/예산 fit과 getter 값은 아직 미관측이다.
`actual_blas_call_observed`/`actual_kernel_observed`/`complete_provider_closure`/
`historical_provider_closure_recorded`/`full_normal40_acceptance`는 false, `retirement_credit=0`,
`original_expected_regenerated=false`를 계속 유지한다. Focus·normal 등 바깥 source pin/원장 등록은
부모 소유이며 기존 accepted cohort의 성공을 새 source로 restamp하지 않는다.


부모 source 검토에서 default-off diagnostic block을 제외한 numerics/example production projection이
기존과 동일함을 확인했다. normal40 helper의 CORE 중 이 두 source hash literal만 현재 bytes로
갱신한다. 원문/frozen input/output/4case/40normal 명령·수치·재현 criteria는 변경하지 않는다.
이는 새 source 바인딩이며 이전 normal40 성공을 현재 전체 수용으로 소급하지 않는다.

## 54diag tapped-build 실제 macro 거부와 최소 compile 후보

[run40164032004/job121081294666](https://github.com/bin9208/openpilot-rust/actions/runs/40164032004/job/121081294666),
source `54b7b8d326b7b1aa3e96852287297e20c5dbcc4f`는 plain-build exit0 뒤 tapped-build exit101로
실패했다. 직접 원문은 `controls-diagnostic-553/tapped-build.stdout`의 Cargo compiler-message다.
`crates/control-policy/src/numerics.rs:573:121`의 `cpu_environment` 값에서 배열 literal 뒤
`.map(...)`의 `.`를 `json!` macro가 거부했다(`no rules expected '.'`). stderr에는 crate compile
실패 요약만 있다. plain-build92.989초/tapped-build17.819초이며 deadline나 getter runtime 실패가 아니다.
Clippy·unit2·original/native plain/tapped 계산과 getter는 모두 미도달이다.

최소 후보는 기존 배열/map/collect 표현식 전체에 괄호 한 쌍을 추가하는1행 수정이다. serde_json의
macro는 `: [...]`를 JSON array로 먼저 처리하고, 괄호로 묶은 값은 Rust expression으로 처리한다.
환경4키/조회 값/None 처리/collect 결과와 관측 위치·동일 Library·production default-off projection은
그대로다. diagnostic helper의 numerics source SHA와 해당 WF helper literal만 연결해 갱신한다.
기존 unit2 exact 명령·Clippy/재현 조건·math/expected/52분 deadline·모든 cap/finalizer는 바꾸지 않는다.

artifact12588080846은88,186,524B/SHA256
`d4f9da45e25b97483e068f232ce82294f1415b04519bdd67ab95c0902c4a28f8`이며 API digest와 직접 SHA가 같다.
104개 전수 CRC/SHA 정상, expanded357,840,437B/largest101,555,623B다. source/index/전체 SDK 등
9쌍은 전바이트 같다. child receipt20개는 모두 회수 완료이며 final children.live는 빈 목록이다.
다만 `numerics/originaldeps/packages/python-provider`의 runtime baseline4개는 build 뒤 설정 전이라
미확립으로 기록됐고 final-seals.pass/immutable_seals_pass는 **false**다. 이를 true로 고쳐 쓰지 않는다.
source·SDK after와 upload는 success, immutable/qualification은 failure, 두 owned cleanup은 skipped다.
held-after140개는 존재하지만 별도 held-before-runtime 원문이 없으므로 독립 전후 pair140개라고 세지 않는다.

로컬은 source/AST/Rustfmt/YAML/Bash/diff와 ZIP inert 감사만 수행했다. macro expansion/수정 뒤
compile·Clippy·unit2·getter/수치/CI는 **UNRUN**이다. 다음 actual은 같은 tapped-build부터 시작해
기존 엄격한 흐름을 통과해야 한다. 상세 원문은 ignored `hosted-54diag-12588080846.audit/`와
`hosted-54diag-12588080846-audit.md`, 최소 계획은 `hosted-54diag-tapped-build-fix-plan.md`에 있다.
부모 소유 normal helper CORE의 numerics pin도 새 source로 갱신해야 하며 그 helper bytes가 바뀌면
diagnostic COMMON_SHA를 새 actual SHA에 다시 연결해야 한다. 여기서는 normal/Focus/원장을 편집하지 않았다.


부모는 위 macro 괄호 정정의 numerics SHA를 normal CORE의 해당 literal1곳에도 연결하고,
변경된 normal helper의 전체 SHA를 diagnostic COMMON_SHA와 WF helper literal에 순서대로 다시 묶었다.
normal40/frozen input/output/재현 비교 기준이나 production 수학 변경은 없다. 이 바인딩은 새 source를
검증하는 것이며 미도달 unit2/getter runtime의 성공을 선점하지 않는다.


## a483 실제 getter 성공·중간 bit 일치와 재현 실패


sourcea483/Actions40166180678/job121087586862는 FAIL이다. artifact12588319492는89724404B/SHA256 `22fe0caa368306b8250e0e5eb0bfbd176633d6bab6ba8547548219bba00c5540`,162CRC/expanded368221660B를 부모가 확인했다. source/SDK/provider·자식회수 final seals는 pass true다.

getter음성unit2가 실제 성공했고 original/native 각각4호출의 중간 F32/F64 bit2329개는 전부 같다. plain/tapped 관측 유효성도 통과했다. 같은 인스턴스에서 관측한 provider config/core/thread 보고는 서로 같고 core는 Haswell이다. 이는 실제 BLAS/kernel 호출 관측이 아니며 기존 false를 보존한다. with/without LD_LIBRARY_PATH 두 native 실행의 argv/cwd/held ELF/input이 같고 output/statebits도 같다.

현재 원본 oneframe은 frozen state 재현 true지만 native의602 실패state 재현은 false다. prefix4/601frame도 oneframe native state와 같지만602/frozen 전체 typed-state 재현 flags는 false다. 값이 같아 보이는 일부 필드만으로 JSON type까지 포함하는 전체 gate를 통과했다고 하지 않는다. final diagnostic은 기존 reduced-reproduction 조건에서 실패했고 fullnormal40/retirement credit0이다.

두 실제 CPU는 다르다. 54b normal40164032028은 AMD EPYC9V74이며 raw flags에AVX512F/DQ/BW/VL가 있다. 이번diag는 AMD EPYC7763이며 AVX2/FMA는 있지만AVX512는 없다. 이번 NumPy 실제 dispatch는 exp X86_V3다. 이 차이가 기존54ULP의 원인이라는 증명은 아직 없고, 실패 VM의 실제 BLAS core/kernel과 NumPy dispatch는 미관측이다.

[NumPy2.5.3 원문](https://github.com/numpy/numpy/blob/v2.5.3/numpy/_core/src/umath/loops_exponent_log.dispatch.c.src)은 exp의AVX2/AVX512 경로를 같은 polynomial 구조로 생성한다. 따라서 CPU flags만으로 exp 구현을 바꾸거나 SVML 차이라고 단정하지 않는다. native numpy_exp는AVX2/FMA에서 현재 polynomial을 쓴다. CPU·BLAS/ufunc 선택 차이는 조사 가설이다.

다음 최소 검토는 동일 job/VM의 실제 normal40 실패 ELF·provider·입력을 먼저 보존한 뒤, 같은 원본과 native의 제한된 재생을 수행해 고정 원문/현재 원본/현재 Rust 세 결과를 구분하는 것이다. 별도 VM의 nonreproduction을 성공으로 바꾸거나 CPU강제·tolerance·expected·수학을 수정하지 않는다. full40 원래700/600초와실패결과를 유지한다. 새로운 원본 관측은 frozen 기대값을 덮어쓰는 절차가 아니다. 구현은 아직0이다.
