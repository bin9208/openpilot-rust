# #523 compiler stage73 원본 관측 collector

작성: 2026-10-26. 기준 source: `1000464f57239a1d15bfba3bdb6c01550f12d8a0`.
상태: **collector source 구현, GHA UNRUN, retirement credit0**.
부모가 capture 예산 조정을 허용하여 record16MiB·총64MiB로 설정했다. 원래73건의 정적 JSON
상한은 아래와 같이 계산했다. 실제 원문 수집/compile/parity 수용은 부모 GHA가 남아 있다.

## 동일한 원본 호출에 추가한 관측

변경 코드는 `rust/crates/model-runtime/tests/compiler_schedule.rs` 한 파일이다.
기존 `original_python_pipeline_boundaries_match_native`가 생성한 같은 request bytes를 같은
원본 프로세스 한 번에 전달한다. 실행 파일만 부모가 입증한 canonical 절대 Python으로
선택하며 기존 `-I -c ORIGINAL_BOUNDARIES ROOT` 순서/인자를 유지한다. PATH fallback,
추가 Python version 호출, 다른 original/model/compiler 실행은 없다.

`ORIGINAL_BOUNDARIES`, 원래73건 생성, 원본 결과와 native를 비교하는 assertions는 바이트
그대로다. native stage core나 Python 생산 stage 계산은 바꾸지 않았다. 별도 Python wrapper나
공유 collector framework를 만들지 않고 같은 test 안의 작은 관측 module로 감쌌다.

child가 직접 쓴 stdout/stderr 파일을 그대로 읽어 기존 `std::process::Output` 계약으로
돌려준다. 원본 raw를 native 결과로 채우거나 JSON을 다시 직렬화해 대체하지 않는다.
native 비교는 관측 receipt 생성 뒤 기존 코드에서 수행한다. collector receipt에는 native
entrypoint/version 예상값이나 native 계산 결과를 넣지 않는다.

## 부모가 제공할 정확한 계약

| 입력 | 요구 조건 |
| --- | --- |
| `GITHUB_ACTIONS` | 기존대로 `true` |
| `DIAG_SOURCE` | 절대 경로, canonical 결과가 `CARGO_MANIFEST_DIR/../../..`의 canonical 경로와 같음 |
| `CI_JOB_TEMP` | 기존 job의 canonical 절대 경로. job과 `proof/`는 현재 uid 소유 directory |
| `GITHUB_RUN_ID`, `GITHUB_RUN_ATTEMPT`, `GITHUB_JOB` | `CI_JOB_TEMP/owner` 원문이 정확히 `run:attempt:job`이며 **개행 없음** |
| `GITHUB_SHA` | 소문자 hex40. 기존 `proof/SOURCE_COMMIT`의 `SHA\n` 원문과 일치 |
| `STAGE_REFERENCE_PYTHON` | canonical 절대 regular file. symlink/PATH 이름을 거부하며 선택 파일을 full SHA로 확인 |
| `proof/stage-oracle-python.sha256` | `SHA256␠␠canonical-python-path\n` 정확 형식. 현재 executable SHA와 일치, 4096B 이내 |
| `proof/stage-oracle-python.version` | 같은 admitted Python의 version proof. `Python `으로 시작하는 한 줄 + LF, 4096B 이내 |

job/owner 생산자의 `printf '%s:%s:%s'` 계약을 사용한다. collector에서 개행을 추가하거나
`trim`으로 다른 owner 원문을 허용하지 않는다. 기존 다른 관측에서 확보한 동일 Python의
SHA/version proof를 부모가 위 파일명으로 복사해 재사용할 수 있다. 새 version 확인이
필요하다면 부모 admission에서 수행하며 이 test는 version 프로세스를 만들지 않는다.

부모는 기존 `native-skip-miri` feature를 켜야 한다. optional `sha2`/`libc`를 사용하는 새
collector와 기존 ignored 원본 검사만 이 feature로 묶었다. 일반7 native stage 검사는
feature 없이도 남는다. Cargo 등록/의존성/WF 변경은 하지 않았다.

## 산출물과 bounded 실패 계약

출력 위치는 기존 proof upload 아래 `CI_JOB_TEMP/proof/compiler-stage-original/`이다.
새 directory를0700으로 만들며 이미 있으면 실패한다. record는 모두 `create_new`·0600·
`O_NOFOLLOW`로 만든다.

| record | 내용 |
| --- | --- |
| `request.json` | 실제 stdin에 쓰는73건 전체의 정확한 JSON bytes |
| `oracle.py` | 기존 `ORIGINAL_BOUNDARIES` 원문 |
| `python.sha256`, `python.version` | 부모 admission proof 원문 |
| `stdout.bin`, `stderr.bin` | 같은 원본 child가 직접 기록한 raw bytes |
| `observation.json` | 정확 argv/request hash, rc·signal, raw 길이·hash, 선택 witness 전후 정보와 binding 결과 |

현재 구현은 **record당16MiB**이며 stdout/stderr도 **각16MiB**, 총 산출물은 **64MiB**다.
receipt는 별도로256KiB 이하다. child의 `pre_exec`에서 `RLIMIT_FSIZE=16MiB`,
`RLIMIT_CORE=0`을 설정하며 부모 limit은 바꾸지 않는다. request/snippet/Python proof를 먼저
기록하고, 두 stream의 최대32MiB와 receipt 최대256KiB를 더해64MiB 이내일 때만 child를
시작한다. 부모도 파일을 bounded read하며 각 record의 regular-file 여부/16MiB 상한과
총량을 확인한다. 정상73건의 raw는 자르거나 요약하지 않는다.

stdin 쓰기 실패도 child wait 뒤 receipt에 기록한다. 원본 exit code/signal, 음수 signal
returncode, `SIGXFSZ` 여부, 각 stream의 상한 도달 여부를 남긴다. Python이 파일 크기 오류를
예외/비정상 종료로 처리할 수도 있으므로 초과를 반드시 `SIGXFSZ`라고 단정하지 않는다.
상한에 잘린 파일은 실제 남은 raw prefix이며 원래 전체 결과를 복구했다고 주장하지 않는다.
기존 status/JSON/native assertions는 그대로 실패하고 성공 fallback/skip은 없다.

raw/receipt는 source/Python binding 거부와 이후 native parity assertion 실패 때도 남는다.
spawn/admission 자체가 실패하면 원본 프로세스 결과가 생기지 않으며 성공 관측으로 세지 않는다.

## 원본 source SHA와 외부 seal 재사용 범위

원본 stdout은 이미 `schema.py`, `offline_schedule.py` 두 파일의 full SHA를 출력한다.
collector는 이 두 key가 정확히 존재하고 실행 전 witness의 SHA와 같은지 확인한다.
이후 같은 source 파일의 SHA와 metadata를 다시 확인하므로 실행 중 source 변경도 거부한다.

선택 witness는 원본 두 파일, native `compiler_schedule.rs`/`entrypoint.rs`/`lib.rs`, 현재
`tests/compiler_schedule.rs`, package Cargo manifest와 lock, canonical Python executable,
job owner, SOURCE_COMMIT, 두 Python proof다. regular/canonical/no-follow 파일을 대상으로
dev/inode/size/mode/uid/gid/nlink/mtime/ctime 및 full SHA를 전후 비교한다. witness당64MiB
상한으로 streaming hash하며, 별도 source 복사나 전체 트리 scan은 추가하지 않는다.

`GITHUB_SHA`↔`SOURCE_COMMIT`과 canonical `DIAG_SOURCE`는 부모의 기존 trusted source/index
seal에 연결한다. collector가 별도 Git 명령으로 checkout 전체를 검증했다거나 source index,
SDK, Python stdlib/import/DSO closure 전체를 다시 입증한 것은 아니다. snippet은 stdlib AST를
사용하며 Tinygrad/project Python 모듈을 import/실행하지 않는다. 이 선택 witness와 admitted
Python의 전후 결합 범위를 전체 importer/모델 수용으로 확대하지 않는다.

## JSON shape의 정적 상한과 최종 capture 예산

처음 요청된1MiB/8MiB는 capture 예산이었다. 원래73건에는65,536개 named empty stage를
허용하는 사례와65,537개를 거부하는 사례가 있어 정상 stdout부터1MiB를 초과한다.
부모의 후속 지시에 따라 capture 상한만 조정하며 입력/원본 프로세스/비교73건, 허용·거부
65,536/65,537, 모든 기능 임계값은 그대로다.

아래 계산은 source의 count와 JSON literal 길이를 사용한 보수적 **정적 상한**이다.
원본 함수·73건 생성 코드·native 알고리즘을 로컬 실행하거나 예상 결과를 만들지 않았다.
native/Python 계산값을 corpus expected로 넣지도 않았다.

| 구간 | source shape와 계산 | byte 상한 |
| --- | --- | ---: |
| 큰 request 두 사례의 stage 목록 | 총131,073개. 최대 이름 `stage-65536`, compact `{"calls":[],"name":"stage-65536"}`33B + comma1B | 4,456,482 |
| 전체 request | 위 값 + 큰 case wrapper128B×2 + 나머지71건×512B + 최상위 구분자75B | **4,493,165** |
| 큰 성공 사례의 stdout entries | 65,536개. 최대 이름 `stage-65535`, 기본 공백 포함 `{"name": "stage-65535", "start": 0, "end": 0}`45B + separator2B | 3,080,192 |
| 전체 stdout | 위 값 + 나머지72개 result×512B + source SHA 두 개/JSON envelope/newline1024B | **3,118,080** |

request의 작은71건 중64건은 stage3개 이하, retained CALL5개 이하, prefix6개 이하이다.
나머지7건도 이보다 작은 shape다. 작은 result는 stage3개 이하이고 count/range는 두 자리
이하이며, `한글 stage`가 JSON Unicode escape로 늘어나는 경우도512B 안에 들어간다.
65,537개 사례는 원래 거부되므로 stdout에서는 큰 entrypoint 목록 대신 작은 error result다.
큰 성공 사례는 모두 empty stage라 start/end가0이다. 이 성질을 바꿔 상한을 맞추지 않았다.

request≤4.286MiB, stdout≤2.974MiB로 각16MiB에 충분히 들어간다. 두 값과 receipt 최대256KiB,
snippet/Python proof용16KiB를 합쳐도 **7,889,773B**이며 정상 stderr를 제외한 상한이다.
stderr의 오류 원문도 별도로16MiB까지 보존한다. 실제 bytes/rc/parity는 GHA 관측 후 수용한다.

최종 예산은 record16MiB·총64MiB다. request를 최악16MiB로 잡아도 두 stream32MiB,
receipt256KiB, 현재 snippet/proofs의 보수적16KiB를 더한50,610,176B가64MiB 안에 들어간다.
초과 실패도 child limit와 실패 receipt로 구분하며 성공으로 바꾸지 않는다. 이 하위 directory의
64MiB는 부모 proof512MiB 예산의12.5%다. 다른 proof와 합친 전체 admission은 기존 부모
workflow/cleanup의 소유이며 이 collector가 전체 proof 예산을 독립 재수용한 것은 아니다.

## 메모리 부담

child stdout/stderr는 pipe 누적 대신 bounded regular file로 직접 기록한다. 부모는 child 종료
뒤 bounded bytes로 읽는다. capture가 유지하는 request16MiB + 두 stream32MiB + request
무결성 확인의 일시 재읽기16MiB를 합치면 logical byte payload 상한은64MiB다. 현재73 shape에서
stderr를 제외한 같은 겹침은 `2×4,493,165 + 3,118,080 = 12,104,410B` 이하이다.
이는 Vec capacity/allocator/Python object/기존 Rust JSON tree를 포함한 RSS 상한이 아니다.

관측용 source SHA 확인은 두 SHA field만 typed deserialize하고 큰 `results` 값은 skip한다.
원래 native 비교가 파싱하는65,536-entrypoint `Value` tree를 관측 단계에서 한 번 더 만들지
않는다. source/Python witness hash는64KiB buffer로 streaming한다. 기존 case 생성/Python
AST/result list와 native 비교의 메모리 부담은 그대로이며 실제 peak RSS/시간은 미실측이다.

## GHA selector, 개수, 다음 소비자

부모의 기존 source/SDK/resource admission 아래 다음 정확 selector를 사용한다.
아래 명령은 로컬에서 실행하지 않았다.

```bash
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features \
  --features native-skip-miri -p openpilot-model-runtime --test compiler_schedule \
  original_python_pipeline_boundaries_match_native -- --ignored --exact --nocapture
```

기존 일반7 + original ignored1, 원본 입력73건을 유지한다. 검사 수를 늘리거나 원래
native stage core를 수정하지 않았다. 부모는 canonical Python env와 두 proof를 준비하고,
기존 proof upload에 위 새 하위 directory가 포함되는지 확인한다. 새16MiB/64MiB는 capture
예산이며 기존 기능 gate의 cap이 아니다. main/clone WF·pins·원장·commit/push/GHA는 부모 소유다.

다음 순서는 **actual 수집 → 독립 raw/source/Python proof 감사 → frozen request/result reader
등록 → Python 생산 stage 중복 계산 제거**다. 현재 live 검사는 원본 source의 AST 문장을
읽으므로 먼저 생산 문장을 지우면 gate가 깨진다. frozen reader도 현재73건/거부 사례/native
assertions를 유지해야 하며 candidate output으로 expected를 만들지 않는다.
자세한 다음 소비자 제안은 [F02 설계](python-rust-523-compiler-next-closure.md)에 있다.

## 로컬 source 보존 확인

기준 source와 byte 비교한 구간:

| 구간 | SHA256 |
| --- | --- |
| 원본 snippet 상수 | `d537fada783a545fcdf850625a483903441c1fe42a3d877cad1d6e8a600860db` |
| 기존73 생성 + root 선언 | `6121447eb321d7157c28493e3a479ec32e72dd10e7ed8b909ab0a97356f154fe` |
| 기존 status/JSON/native 비교 본문 | `e3225253b131151091c92fb74d99ef66e80a24f83446f600cc0db77a397fc528` |

일반7 검사 본문도 불변이다. 로컬은 snippet AST parse, Rust rustfmt/check, source/diff와
byte 보존 비교만 수행했다. Cargo/build/test/Clippy/프로젝트 CLI/원본 Python child/model/compiler/
GHA 실행은0회다. 실제 compile·관측·parity PASS는 아직 주장하지 않는다.
