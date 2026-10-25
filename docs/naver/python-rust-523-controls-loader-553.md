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
