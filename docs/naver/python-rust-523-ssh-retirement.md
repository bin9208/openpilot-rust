# #523 SSH 원본1 퇴역 준비 구현 계획

> 아래 UNRUN/credit0은 구현 시점의 역사 기록이다. 최종 실제 수용은 문서 끝의 2026-10-25 기록을 따른다.
> 당시 단계는 아래의 postabsence 구현 후보다. 승인에 따라 `tools/scripts/ssh.py`
> 1개를 삭제했으며 새 source의 기능·부재 수용은 **UNRUN**, retirement credit은0이다.
> 먼저 보존한 아래 본문은 frozen importer 초기 구현 기록이며 당시의 미실행 상태를 뜻한다.

**목표:** 기존 SSH 원본 관측26+13건을 재capture 없이 고정 artifact에서 읽고,
기존 authored input/criteria 및 native verifier와 완전히 대응시킨다.

**구조:** setup21의 `admit → 기존 verify_envelope` 구조를 재사용한다. 두 ZIP과
GitHub metadata만 명시적 private context에서 읽으며, Python/옛 executable을 실행하거나
원본 source를 추출하지 않는다. ZIP의 모든 member를 bounded stream으로 EOF까지 읽어
CRC를 검증하되 보존하는 내용은 선택 log와 SOURCE_COMMIT뿐이다.

**구현 도구:** 기존 Rust std/rustix/zip9.0.3/serde_json/sha2. Cargo 변경·새 의존성은 없다.

**초기 승인 범위:** 사용자 지시의 SSH1 후보. 이 초기 phase에서는 `tools/scripts/ssh.py`를 보존했다.

## 소유 파일과 변경 경계

- `rust/crates/dev-tools/tests/developer_cli_original.rs`: 기존 두 SSH test body,
  Case/candidate/verify_envelope/setup21 body를 유지하고 compare를 frozen admission에 연결한다.
  모든 group이 frozen이 되므로 미사용 fresh Python oracle/실행 경로는 제거한다.
- `rust/crates/dev-tools/tests/developer_cli_original/ssh_frozen.rs`: bounded artifact importer,
  역사 metadata/raw/input/criteria 검증과 의미 있는 음성 통제.
- 이 문서: 계획·실제 정적 검증·후속 Actions 계약을 한국어로 기록한다.

공유 Cargo/workflow/Focus/원장/production/auth/API/HTTP 코드는 수정하지 않는다.
현재293교체/3부분/3284대기는 이 후보로 바뀌지 않는다.

## 고정 입력과 provenance

| 항목 | plan26 | parser13 |
|---|---|---|
| artifact | 12563952000 | 12560030996 |
| run | 40103403093 | 40092771379 |
| source | `5d376b306c93ec799b0c5346cb60e9425b1618f7` | `fb1228f02e0546b6fe7a9d8d0eec098e650a86bd` |
| ZIP bytes | 15,834,369 | 7,775,163 |
| ZIP SHA256 | `f32e796ddd7e8e4b5ff9c50d0af1bddefe460a6d4b5b489d975e449483a4ef6c` | `21c33f18576748c77664d0267b4c51edca8aedf0f8b3c345e8e91d477d739878` |
| ZIP members / expanded bytes | 55 / 24,465,234 | 47 / 14,349,404 |
| 선택 log | `ssh-alias-original.log` | `ssh-parser-original.log` |
| log bytes / CRC32 | 42,741 / `868a933b` | 11,643 / `c6015ad0` |
| log SHA256 | `a1ac7a4514be5ec6ccf7e73f0ed23e6364f4e71ed6e8553d453feb29e7e3dbdb` | `3c6ac45ece10931d5f0149e3bba3d0f99d0afdf55053491ce580055183216a29` |
| JSON payload bytes | 21,957 | 9,313 |
| JSON payload SHA256 | `c7ee0ba37a74aa9c58bd9c333281b8405548c2267e832f6606b5ac0311bd4b46` | `8ce2735003cd58258142aa16c9716988da143cff37b2d8ce1a920df58ae27076` |

payload는 `DEVELOPER_CLI_ORIGINAL_JSON ` 뒤의 원문부터 해당 LF 직전까지다.
libtest progress prefix는 허용하되 marker는 정확1개여야 한다. 선택 log의 개별
test 이름과 `1 passed; 0 failed; 0 ignored` 결과도 고정한다. fb1228의 실패한 alias22는
소비하지 않는다. 원문을 JSON 재직렬화하여 새 expected로 저장하지 않는다.

historical metadata는 CPython `3.12.15 (main, Oct  1 2026, 02:35:59) [GCC 13.3.0]`,
requests `2.34.2`, 원본 source2 SHA를 정확히 유지한다. 원본 SSH SHA는
`f4fb8dff60a0bab57643172093a6f437acb0d001929103f081d872280710eeff`,
setup SHA는 `ab157e4943734c969ffc1e8113dc9d09ded355f3463da34c2b9132fc24181f3f`다.
plan26에만 기록된 historical native_sources5도 exact literal로 확인한다. parser13에
이를 추가하거나 과거 값을 current provider로 덮어쓰지 않는다.
이 metadata는 역사 전체 provider/DSO closure를 기록한 것이 아니다.

## 부모 Actions 입력 계약

`DEVELOPER_SSH_FROZEN_DIR`은 absolute/canonical/owned0700 directory다. 정확한 roster는:

```text
artifact-12563952000.zip
artifact-12563952000.json
artifact-12560030996.zip
artifact-12560030996.json
```

JSON은 각각 `gh api repos/bin9208/openpilot-rust/actions/artifacts/ID`의 원문이다.
id/name/size/digest/expired=false/run/source/branch/repository_id를 확인한다.
네 regular file은 owned0600/nlink1로 stage한다. 다른 파일·alias·누락은 거부한다.
authenticated download는 부모 Actions가 담당한다. importer에는 network·download·cwd
fallback·임의 artifact/path/pin 설정을 추가하지 않는다. 로컬 조사에서는 기존
`focused-5d376b.zip`/`focused-fb1228.zip`만 inert로 읽었고 새 ZIP을 받지 않았다.
captures는 Git 밖에 둔다. metadata는 archive 전체 SHA를 대체하지 않는다.

directory/각 파일을 NOFOLLOW FD로 유지하고 type/uid/nlink/size/mtime/ctime와 path/FD identity,
전체 SHA를 admission/반환 전 확인한다. ZIP은 duplicate/unsafe path/특수 member/크기 초과를
거부한다. source 본문을 filesystem으로 extract하거나 archive 프로그램을 실행하지 않는다.

## 구현과 검증 순서

- [x] 기존 source·setup frozen pattern·로컬 원문과 읽기 전용 GH metadata를 확인한다.
- [x] 이 계획을 먼저 저장한다.
- [x] importer: fixed metadata/ZIP/payload pins와 모든 CRC, source/provider metadata 검사를 구현한다.
- [x] 원문 record의 ID/개수/종류/순서와 authored input/criteria 완전 일치 검사를 구현한다.
- [x] 기존 두 SSH selector를 같은 verifier에 연결한다. setup21와 기존 비교 기준은 바꾸지 않는다.
- [x] wrong pin/동일길이 변조/truncate/extra, metadata/scope/중복/누락/타입,
  authored input/criteria 변경 거부 통제를 같은 child module에 작성한다.
- [x] source-only rustfmt/diff를 확인한다. 파일 SHA는 동결 인계 메시지에 기록한다.
- [ ] 부모 독립 검토 후 Actions에서 기존26+13·새 음성 통제·strict Clippy를 실행한다.

기존 selector:

```text
ssh_cli_original_aliases_prefix_unicode_duplicate_order_and_exec
ssh_cli_original_port_argparse_help_and_no_auth_effects
```

`--test developer_cli_original --features developer-ssh-native`를 유지한다.
부모는 기존 fresh-oracle marker/환경에 의존하는 Focus 등록을 명시적으로 갱신해야 한다.
새 importer 성공은 library semantic39건만 뜻하며 실제 SSH 연결 성공이 아니다.
입력 누락을 ignored/skip/빈 배열로 통과시키지 않는다.

추가 selector는 다음4개다. 기존 setup4+SSH2와 합쳐 target의 선언은10개/ignored0이며,
기능 결과는 아직 UNRUN이다.

```text
ssh_frozen::ssh_frozen_rejects_archive_and_raw_pin_tampering
ssh_frozen::ssh_frozen_rejects_metadata_scope_missing_duplicate_and_types
ssh_frozen::ssh_frozen_rejects_authored_input_criteria_and_order_drift
ssh_frozen::ssh_frozen_held_inputs_reject_mutation_replacement_and_aliases
```

첫 통제는 정상 입력 admission 뒤 archive/raw pin·동일길이 변조·truncate·extra를 거부한다.
압축 bytes를 바꾸지 않고 첫 central entry CRC만 변조해 내부 reader의 EOF 거부도 확인한다.
이 내부 음성 주입은 정상 경로의 고정 archive SHA를 바꿀 수 있는 public override가 아니다.
마지막 통제는 same-size/truncate/동일 bytes 새 inode/symlink/hardlink를 각각 거부하며,
same-size에서는 test metadata를 재바인딩한 뒤에도 이전 SHA가 별도로 거부해야 한다.

## 현재 정적 확인과 한계

기존 두 ZIP과 선택 log/payload를 inert로 읽어 literal pin/metadata를 대조했다.
새 capture·원본 source 추출·프로그램 실행은0이다. candidate/verify_envelope와
기존 SSH2/setup21 test body는 HEAD와 byte 동일함을 확인했다. setup frozen child도 수정0이다.
새 importer·음성 통제·기존39 비교·Cargo/Clippy는 실행하지 않았다.

원문2 ZIP은 합계23,609,532B이고 각 member는64KiB씩 CRC stream으로 확인한다.
ZIP 자체는 bounded read_at Vec로 보유하며 log/payload 이외 member bytes를 저장하거나
재귀 archive로 해석하지 않는다. 외부 SDK/source/ELF/provider의 complete closure와
실제 SSH entry 수용은 이 test helper가 제공하지 않으며 receipt에도 false로 남는다.

## 다음 phase: 실제 native entry 설계만

현재 `ssh_cli()`에는 이미 private HOME/OPENPILOT_PREFIX와 `API_HOST` 입력이 있다.
별도 product 환경이나 TLS 우회 없이 private loopback의 `/v1/me/devices` synthetic 응답을
사용할 수 있다. 공개 helper의 exact Cargo JSONL ELF를 먼저 고정하고, `ssh` 자리는
현재 provider SHA/FD로 묶은 무동작 ELF의 owned copy로 제한한다. exec trace로
helper→developer-ssh→해당 ELF의 정확 argv/identity와 stdout/exit를 관측한다.
real SSH/브라우저/계정/장치/기존 credential은 사용하지 않는다.

help/parser 오류는 API/token 효과0, alias 성공·중복 실패·debug argv는 기존 명시적
criteria와 별도로 검증한다. blanket path/출력 정규화 또는 argparse help byte 동등성
주장을 추가하지 않는다. 이 phase의 코드·실행은 아직0이다.

현재 phase가 수용된 뒤에만 원본 존재+exact SHA와 명시적 absent phase를 설계하고,
same-current-ELF/seals/held/upload/cleanup을 묶어 삭제1 수용을 판정한다.
이번 변경의 retirement credit은0이다.

## 2026-10-25: 원본1 postabsence 구현 후보

사용자가 전체 #523 전환 및 이번 원본1 삭제·검증을 명시 승인했다. 부모가 감사한
pre-delete c00 source `c00c7a11fbee357cbfb39bee45125d73596bf56c`, Focus
`40154418302/job121052614023`, artifact `12584392161`의 최종 성공은 그대로 보존한다.
부모 수용 범위는 336CRC·immutable21쌍·entry4-checked·native7/public1·기본 음성2와
source/index/SDK final·upload/cleanup이다. 새 ZIP 다운로드나 재감사는 하지 않았다.
이 증거는 원본이 존재했던 phase이며 새 postdelete source의 PASS가 아니다.

### 구현 경계

- `tools/scripts/ssh.py` 정확1개를 삭제했다. 원래 mode/blob은
  `100644 / fe8d7d72d3434677b4c5b4f1a3dbcacd2c24e379`, raw SHA는 위의 `f4fb8dff…`다.
  다른 경로의 복제·encoded fixture·rename·stub·Python fallback을 추가하지 않는다.
- 새 `tools/ci/ssh_source_absence_523.sh`는 기존 setup absence의 parent/entry 검사 패턴을
  SSH 정확1경로에 적용한 sourced Bash helper다. setup helper는 수정하지 않았다.
- 기존 driver에 helper literal SHA/held FD와 immutable admission5개 파일을 추가했다.
  새로운 `id` 호출도 현재 provider ELF hold 목록에 추가한다. 기존 source9·Rust의
  초기14/per-use FD·provider/registry·CLI/recorder·DSO 검증과 entry4 기준은 유지한다.
- Focus에는 실행 admission/controls/역사 삭제 확인/closure와 기존 단계 전후 검사를 연결했다.
  allowlist/cohort literal pins/원장 최종 등록은 부모 소유로 남긴다. 등록 전에는 gate의
  자연 실패가 예상되며 source admission을 완화하거나 성공으로 재해석하지 않는다.

frozen39/음성7·setup21·Case/candidate/verifier·fixture·원문 metadata·입력/criteria·원래
Rust entry test/recorder 및 production CLI/HTTP/auth는 변경하지 않았다. `op ssh`는 기존
`run-developer-tool developer-ssh`→현재 ELF 경로 그대로다. 원문의 help/error 이름도
역사 기준이므로 변경하지 않았다. 부모 소유 `…-ssh-entry.md` 수용 기록은 수정하지 않았다.

### 명시 부재 계약

`SSH_SOURCE_PHASE=absent`만 허용한다. checkout 때 canonical source root/정확 HEAD,
private job owner와 source owner marker를 검사하고 root/tools/tools/scripts의
`dev/ino/mode/uid/gid`를 고정한다. HEAD ls-tree와 index ls-files의 대상 경로는 모두
빈 raw여야 하며, 실제 디렉터리의 exact entry가 없어야 한다. dangling symlink·directory·FIFO
등의 node도 허용하지 않는다. 부모 교체/심볼릭 링크/조사 오류는 absent로 간주하지 않는다.

기존 source-reuse 단계가 fetch한 `97f7ad36dfa0919b4e5c216fb16f44ed29161663` object를
재사용한다. exact `D\ttools/scripts/ssh.py\n` diff와 `git show`의 stream SHA를 검사한다.
원문을 파일로 복원하거나 실행하지 않는다. 이 Git 삭제 확인과 역사 metadata는 서로 다른
역할이며 현재 provider/source pin으로 과거 metadata를 다시 쓰지 않는다.

private synthetic root의 controls는 **absent/regular 재출현/directory/symlink/dangling
symlink/parent symlink** 6개다. 최초 absent 성공과 각 정확한 오류 code/message가 모두
맞아야 한다. source entry는41, parent boundary는42, 문맥/조사 오류는43으로 구분한다.
임의 guard 실패를 negative PASS로 취급하지 않는다. 실제 source에서 원본을 임시로 복원하지 않는다.

helper는 source SHA/root/parent identity가 같은 private proof에만 journal을 추가한다.
최초 부재 실패는 create-new sticky marker로 남아 뒤의 일시적인 absent 성공으로 지워지지 않는다.
journal은 owned0600/nlink1 regular이며 읽기 상한1MiB를 둔다. finite phase와 각 command pair의
정확1회, sampled-budget 최소1회, raw 필드/알 수 없는 stage/누락·중복을 closure에서 검증한다.
`strace-help`를 포함한 command9개의 before/after 총18개는 각각 정확1회 필요하다.

### 실패와 최종 gate

driver의 기존 registry-copy/public-build/entry-build/recorder-build/strace-help/entry-0…3 실행 전후와
기존 2초 watcher에 같은 검사를 연결했다. 발견된 실패는 기존 resource stop 및 owner/startticks로
검증한 process group 회수 규칙을 따른다. 첫 command exit와 budget 실패를 그대로 보존한다.
EXIT에서는 부재 검사 실패와 별개로 기존 held/provider/registry/budget final 검사를 시도한다.

Focus의 frozen/Clippy 전후와 frozen input always-after/source-index always-final/SDK final에도
부재 검사를 연결한다. always-final은 helper/absence 검사와 기존 seal을 독립 subshell에서 각각
시도하고 모든 status가 성공해야 해당 step이 성공한다. 부재 오류 때문에 기존 seal을 건너뛰지 않는다.

`ssh_absence_closure`는 admission/controls/역사 삭제/frozen10/SSH27/entry4/input-after/Clippy/
source-final/SDK-final의 **실제 outcome 모두 success**와 driver의 새 source outcome을 요구한다.
required journal/control 집합을 확인한 create-new receipt만 closure 결과로 남긴다.
기존 upload는 실패에도 실행한다. source cleanup은 기존 upload id/digest·owner·source/SDK seal에
추가로 **`steps.ssh_absence_closure.outcome == 'success'`**를 AND로 요구한다.
upload 뒤 rm 직전에는 journal·sticky 파일을 변경하지 않는 read-only guard를 수행한다.
그 실패는 cleanup/후속 common cleanup을 막으며 기존 업로드 증거를 수정하지 않는다.

이 범위는 command boundaries와 driver active-command 2초 sampling이다. 다른 Focus 단계는
명시 전후 경계이며 전체 Actions 시간의 연속 감시나 hostile same-UID race 증명이 아니다.
`whole_python_absence=false`, `real_ssh=false`, `closed_trace=false`, complete provider/FD closure
미수용 및 retirement credit0을 유지한다. target6GiB/Cargo home2GiB/build8GiB/proof512MiB,
종료 reserve/inflight와2700초·각 command/fixture의 기존 시간·회수 상한을 늘리지 않았다.

### 정적 검증 및 후속

현재 로컬은 Bash 문법, YAML/AST·선택 model/문자열 diff/hash만 확인한다. helper controls,
frozen39/음성7·setup21, native7/public1·기본 음성2, Clippy/봉인/새 Actions는 실행하지 않았다.
부모 allow/pins/원장 등록 후 같은 새 SHA의 Focus에서 이 기존 대상과 새 absence6/closure를
실제로 실행하고 artifact·최종 seals/upload/cleanup을 감사해야 이 파일1의 퇴역 credit을 줄 수 있다.

## 2026-10-25 SSH 원본1 실제 부재 수용

source `01fcc36e09b39172f4c88954a702fa8353b2d1e8`의
[집중 검사40157685746/job121062463242](https://github.com/bin9208/openpilot-rust/actions/runs/40157685746/job/121062463242)는
GitHub 기본 X64 러너에서 최종 성공했다. artifact12585103692의41,120,449B와 SHA256
`ff144ca306527bc9d231beb83dcc1f60a7e7312e8e1a234ea1681261a8f6ade9`,343멤버 CRC를 확인했다.
원본 `tools/scripts/ssh.py`의 정확한 역사 SHA와 삭제 diff/HEAD/index 부재, 재출현 통제6,
127개 부재 기록과 명령9개의 전후18경계가 일치한다. frozen SSH39/setup21·importer7의
기존10회귀, Unicode7, 현재 native7/public1 및 entry4가 통과했다.
source/index·Rust/venv·held executable·고정 입력의 전후 봉인과 최종 closure/upload/소유 정리가 성공했다.
held provider의123개 기록은 두 차례 사후 검사 모두 원문과 동일하다.

baseline은 **294 replaced-local / 3 partially-replaced / 3283 pending = 3580**이며
routing map은20 accepted /3286 pending·partial이다. 이번 추가 수용은 원본1개에 한한다.
`replaced-local`은 기존 원장 이름이며 기능 검사는 Actions에서 실행했다.
실제 원격 SSH 연결, 전체 Python 부재, 완전한 build dependency closure 또는 전체 전환 완료를 뜻하지 않는다.
원시 receipt의 retirement_credit0/범위 제한은 원문 그대로 보존한다.
