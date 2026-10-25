# #523 SSH 원본1 퇴역 준비 구현 계획

> 현재 단계는 source-only candidate다. 기능·Cargo·Clippy는 Actions에서만 실행한다.
> 이 문서의 계획에 따라 같은 작업에서 구현하며, 원본 삭제·caller 수용·credit은 아직0이다.

**목표:** 기존 SSH 원본 관측26+13건을 재capture 없이 고정 artifact에서 읽고,
기존 authored input/criteria 및 native verifier와 완전히 대응시킨다.

**구조:** setup21의 `admit → 기존 verify_envelope` 구조를 재사용한다. 두 ZIP과
GitHub metadata만 명시적 private context에서 읽으며, Python/옛 executable을 실행하거나
원본 source를 추출하지 않는다. ZIP의 모든 member를 bounded stream으로 EOF까지 읽어
CRC를 검증하되 보존하는 내용은 선택 log와 SOURCE_COMMIT뿐이다.

**구현 도구:** 기존 Rust std/rustix/zip9.0.3/serde_json/sha2. Cargo 변경·새 의존성은 없다.

**승인 범위:** 사용자 지시의 SSH1 후보. `tools/scripts/ssh.py`는 보존한다.

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
