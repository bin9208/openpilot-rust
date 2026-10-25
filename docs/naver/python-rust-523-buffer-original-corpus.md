# #523 buffer 원본 179경계 관측 collector 준비

작성: 2026-10-26. 상태: **collector 구현, 새 source의 GHA 실행 UNRUN, retirement credit 0**.

기존 source73의 original descriptor32·recipe147 비교 PASS는 이전 기능 증거다.
그때 보존하지 못한 stdout을 이번 collector가 소급해 확보했다고 주장하지 않는다.
Python 원본 모듈은 삭제하지 않으며 production·Cargo·workflow·pin·stage도 변경하지 않는다.

## 기존 비교를 감싸는 경계

`rust/crates/model-runtime/tests/compiler_buffers.rs`의 기존 ignored 검사
`original_python_buffer_tables_match_native` 한 곳에 관측을 추가했다.
`ORIGINAL_BUFFERS` Python snippet, AST로 추출하는 원본 method, descriptor/recipe 생성,
기존 native helper와 native assertion 본문은 그대로 유지한다.
native unit6·original ignored1, descriptor32·recipe147 합계179도 변경하지 않는다.

기존 original 프로세스 **한 번**에 같은 JSON request를 전달한다. 기존
`-I -B -c ORIGINAL_BUFFERS ROOT` 인자는 유지하고 실행 파일만 부모가 입증한 canonical
절대 Python으로 명시한다. PATH 검색·다른 interpreter fallback·추가 original/model/compiler
호출은 없다. 테스트 안에서 Python version 프로세스를 만들지 않는다.

원래 프로세스가 출력한 raw stdout/stderr와 종료코드·signal을 먼저 보존한다.
native 계산값은 원본 프로세스나 관측 corpus에 예상값으로 넣지 않는다.
그 뒤 원래 status/descriptor/trace assertions가 그대로 실행되므로, 원본 exit0이나
capture 파일 존재만으로 parity 또는 source 제거를 수용하지 않는다.

## 부모가 연결할 환경과 admission 증거

새 환경변수는 `BUFFER_REFERENCE_PYTHON` 하나다. canonical 절대 경로이며 regular file이어야
한다. symlink 경로나 `python3` 같은 PATH 이름은 거부한다. `/usr/bin/python3`를 사용할 때도
부모가 먼저 실제 canonical 경로를 얻어 그 파일의 SHA/version 증거를 연결한다.

다음은 현재 native-units의 기존 환경·파일을 그대로 사용한다.

| 입력 | 요구 계약 |
| --- | --- |
| `GITHUB_ACTIONS` | 기존대로 `true` |
| `DIAG_SOURCE` | `CARGO_MANIFEST_DIR/../../..`와 canonical 경로가 같은 admitted source |
| `CI_JOB_TEMP` | 기존 소유 job의 canonical 절대 경로. 이미 있는 `proof/`는 현재 uid 소유 |
| `GITHUB_RUN_ID`, `GITHUB_RUN_ATTEMPT`, `GITHUB_JOB` | 기존 `CI_JOB_TEMP/owner`의 `run:attempt:job`과 정확히 일치 |
| `GITHUB_SHA` | 소문자 hex40, 기존 `proof/SOURCE_COMMIT`의 한 줄과 일치 |
| `proof/buffer-oracle-python.sha256` | `SHA256␠␠canonical-python-path\n` 형식, 현재 Python full SHA와 일치, 4096B 이내 |
| `proof/buffer-oracle-python.version` | 같은 admitted Python의 기존 version proof, `Python `으로 시작하는 단일 행, 4096B 이내 |

같은 interpreter의 admission 증거가 이미 있으면 부모가 위 고정 파일명으로 복사해 재사용할
수 있다. 새 version 조회가 필요하면 부모의 admission 단계에서 준비하며 collector는 조회하지
않는다. 두 proof와 `SOURCE_COMMIT`, job owner, Python 및 source witness를 실행 전후로
다시 읽어 full SHA와 dev/inode/size/mode/uid/gid/nlink/mtime/ctime을 비교한다.
선택 witness와 Python 파일은 각각64MiB 이내 regular file이며 최종 symlink를 따라가지 않는다.

GitHub SHA는 기존 source admission의 `SOURCE_COMMIT`과 환경을 결합한 값이다. collector가
별도 Git 명령으로 HEAD를 검증했다거나 SDK 전체 closure를 다시 수용했다고 주장하지 않는다.

## 보존 파일과 상한

출력은 기존 upload 대상 아래 `CI_JOB_TEMP/proof/compiler-buffer-original/`이다.
디렉터리는 새로700으로 만들며 이미 존재하면 실패한다. 아래 모든 record는
`create_new`·0600으로 생성하고, record당1MiB·합계8MiB 상한을 검사한다.

| record | 내용 |
| --- | --- |
| `request.json` | 실제 stdin에 전송하는 descriptor32·recipe147의 정확한 JSON bytes |
| `oracle.py` | 실제 `-c`에 전달하는 기존 `ORIGINAL_BUFFERS` 원문 |
| `python.sha256`, `python.version` | 부모의 두 admission proof 원문 |
| `stdout.bin`, `stderr.bin` | 같은 원본 프로세스의 raw bytes, UTF-8 재인코딩이나 pretty-print 없음 |
| `observation.json` | SHA/argv/request hash, raw 길이·hash, rc/signal, 전후 witness, binding 결과, 한정 scope |

stdout/stderr는 pipe의 무제한 메모리 capture 대신 위 새 regular file로 직접 받는다.
같은 Python child의 `pre_exec`에서 `RLIMIT_FSIZE=1MiB`, core limit0을 설정하므로 실행 중부터
파일 증가를 제한한다. 부모 limit은 바꾸지 않는다. 초과로 인한 `SIGXFSZ`는 signal과 음수
returncode로 기록하며 성공으로 바꾸지 않는다. 제한 실패의 raw 파일은 전체 논리 출력이
아닌 실제 파일에 남은 prefix일 수 있다. stdin 쓰기 실패도 직접 child wait 뒤 기록하고 거부한다.

selected source witness는 원본 `buffers.py`·`offline_buffers.py`와 schema, snippet에서 직접
import하는 Tinygrad device/dtype/UOp, CPU copy의 ops_cpu/HCQ/memory/libc 및 관련 renderer/helper
파일이다. native `compiler_buffers.rs`·`export.rs`·`lib.rs`·현재 테스트와 Cargo manifest/lock도
묶는다. 정확한 목록은 테스트의 `BUFFER_WITNESSES`이며 전체 import·DSO·stdlib closure는 아니다.
원본 stdout 자체가 보고한 여섯 source SHA도 전후 witness의 값과 비교한다.

원본 실패·이후 native parity 실패 때도 이미 쓴 원문을 유지한다. `observation.json`에는
`collector_only=true`, `retirement_credit=0`을 쓰며 native 비교 성공을 대신하는 PASS record는 없다.
부모는 기존 always upload로 이 하위 디렉터리를 보존하고, 아래 정확1개 ignored 검사 성공과
source/SDK 최종 봉인을 별도로 확인해야 한다.

## 부모 GHA selector

기존 buffers branch의 명령과 count는 유지하고 original 명령에 환경만 연결한다.
아래 명령은 인계용이며 로컬에서 실행하지 않았다.

```bash
# 부모의 admission이 준비한 실제 canonical Python과 두 proof를 먼저 연결한다.
run_once compiler-buffer-units "$sdk/bin/cargo" test --manifest-path rust/Cargo.toml --locked \
  --no-default-features --features native-skip-miri -p openpilot-model-runtime --test compiler_buffers
grep -F 'test result: ok. 6 passed; 0 failed; 1 ignored;' "$CI_JOB_TEMP/proof/compiler-buffer-units.log"

run_once original-buffers /usr/bin/env BUFFER_REFERENCE_PYTHON="$buffer_python" \
  "$sdk/bin/cargo" test --manifest-path rust/Cargo.toml --locked \
  --no-default-features --features native-skip-miri -p openpilot-model-runtime --test compiler_buffers \
  original_python_buffer_tables_match_native -- --ignored --exact --nocapture
grep -F 'test result: ok. 1 passed; 0 failed; 0 ignored;' "$CI_JOB_TEMP/proof/original-buffers.log"
sha256sum --check --strict "$CI_JOB_TEMP/proof/buffer-oracle-python.sha256"

run_once compiler-buffer-clippy "$sdk/bin/cargo" clippy --manifest-path rust/Cargo.toml --locked \
  --no-default-features --features native-skip-miri -p openpilot-model-runtime \
  --lib --test compiler_buffers -- -D warnings
```

collector를 검증하려고 성공한 불변 gate를 로컬에서 반복하지 않는다. 이번 로컬 검증은
source/구문/서식/diff와 원문 보존 비교뿐이다. 새 GHA 원문·179경계 수용·모델/장치 기능 수용은
부모가 실제 결과를 받은 뒤 구분해 기록한다.
