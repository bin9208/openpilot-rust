# #549 logging source guard 사전 통합 후보

2026-10-25. Rust guard5파일은 앞선 동결본이고 이 기록은 CI 연결만 설명한다.
**새 source의 compile/Clippy/unit67/CLI20/raw21/cleanup은 GHA UNRUN**이다.
원본 `rust/tools/check_native_logging.py`, `rust/tools/native_logging_transport.py`는
그대로 보존한다. 이 후보는 postdelete full 기능, old27/R4, 원본2 퇴역 credit을 부여하지 않는다.

## 실행 경계

`tools/ci/logging_retained_549.sh` 무인자는 실제 original2 fixed SHA를 확인하고
기존 full-present14·untraced1·retained-once·exact15 정책을 실행한다.
core21의 본문은 유지하고 source-policy filesystem6을 더해 core27 exact roster를 요구한다.
prepare9/trace24/summary7을 합친 선택 suite는67개이며 각 prefix의 정확한 이름·개수·0 ignored를 확인한다.
기존 legacy-runtime2 mode의 INT/TERM2, 원래2초/별도600초와 current fresh C++ 비교도 유지한다.
현재 역사 실행의 성공은 새 source의 성공으로 소급하지 않는다.

새 명시 mode는 다음 한 가지다. 다른 인자·추가 인자는 fallback 없이 거부한다.

```bash
/usr/bin/bash tools/ci/logging_retained_549.sh source-guard
```

성공한 같은 job의 default와 legacy2 뒤에만 실행한다. 기존 CargoJSONL의 exact target/kind/src/profile,
receipt의 source SHA/root·owner·proof identity와 모든 prior held identity/hash를 다시 검사한다.
checker/probe/fixture 및 legacy2에서 fresh 선택한 builder를 새 FD로 hold한다.
두 Bash 호출 사이의 FD 연속성을 주장하거나 cached artifact를 새 compile으로 표시하지 않는다.
컴파일·설치·C++/probe/Python 실행을 이 mode에 추가하지 않는다.

## private copy와 예산

copy는 `$CI_JOB_TEMP/logging-source-absence-copy/source`다. copy 전에 실제 tracked leaf의 block 추정,
Git metadata 실제 할당량과 entry별 보수적 block을 계산한다. 기존 target을 제외한 실제 source owner는2GiB,
독립 copy의 실제 할당량+계획 성장은 별도2GiB 안이어야 한다(두 root 합산 최대4GiB).
이 단계의 기존 source 계획 성장은0이며 source/Git/index의 전후 불변 검사는 그대로 요구한다.
실제 고유 filesystem마다 해당 device에 추가될 copy 예정 성장+proof 예정 성장+finish512MiB reserve를
합산한다. copy 전 호출은 세 번째 인자67108864로 별도 manifest 예정 성장64MiB를 전달한다.
따라서 같은 FS에서는 copy 예정량+manifest64MiB+finish512MiB를 해당 operation 전에 함께 admit한다.
이는 기존 proof2GiB cap을 올리지 않는다. 서로 다른 device도 각각 검사하고, 같은 device를 중복 합산하지 않는다. target8GiB/proof2GiB,
원래 command512MiB/file·trace512MiB/file/1GiB aggregate·event100만/line200만 상한은 유지한다.
이는 command 전후 admission이며 copy/test 도중의 hard quota라고 표현하지 않는다.

target/SDK/archive를 복제하지 않는다. source의 actual tracked index와 독립 `.git` metadata를 복사하고
두 literal Python leaf만 제외한다. actual source는 변경하지 않는다. source/copy의 HEAD/index,
exact two-deletion diff, 나머지 tracked canonical tar bytes, Git metadata hashes와 현재19 source pins를 대조한다.
Git index를 고치거나 dirty 상태를 숨기지 않는다. full absent 실행 양성은 이 copy에서 요구하지 않는다.

copy의 전체 roster, regular file identity/mtime/ctime/hash와 현재 root/부모 identity를 전후 비교한다.
통제가 생성·제거하는 directory의 mtime/ctime은 의도적으로 비교에서 제외한다.
파일·symlink·mode·owner·dev/ino 변화나 남은 통제 node는 허용하지 않는다.

## 실제 CLI20와 raw21

기존 fixture의 새 ignored selector 하나만 선택한다.

```text
source_absence::source_absence_rejects_original_nodes_before_producers
```

2개 literal leaf × regular/directory/live symlink/dangling symlink/parent symlink × retained/legacy의20조합이다.
기존 Rust body는 exact guard stderr/exit1, stdout/output/barrier 부재, foreign sentinel 불변,
provider FD/hash 불변과 실제 Checker wait/reap을 요구한다. 다른 입력 오류를 guard PASS로 쓰지 않는다.

기존 secret-free `env -i` whitelist 및 fixed-true Git profile로 fixture1회를 trace한다.
`--seccomp-bpf --kill-on-exit -ff -ttt -x -v -s 65536`와 기존 full syscall classes/raw read를 사용한다.
BPF 활성 자체를 증명했다고 주장하지 않는다. inspector는 strict parser를 그대로 사용하며 unknown은 FAIL이다.

기존 `logging_retained_exec_policy_549.jq`에 finite `source-absence` 분기만 추가했다.
Rust source의 고정20조합/argv/env와 create_new receipt를 원문과 대조한다.
fixture1 + exact held checker20 = exec21, 각 checker exit1·생성/session 시도0,
builder/probe/Python/C++ exec0을 요구한다. libtest thread는 기존 bounded creation-edge 경로로 fixture에 연결한다.
빈 상세 배열만으로 child0을 수용하지 않고 전체 summary opcode counts와 상세 cardinality를 맞춘다.
각 syscall/exec의 정상 lexical parsing과 원문 SHA를 유지한다.
추가 policy 음성3은 구조 상세 누락·잘못된 argv·잘못된 env가 각각 정확 오류/exit5로 거부되는지만 검사한다.
원문·expected payload·actual raw는 변경하지 않는다.

## 후검사와 cleanup

실패에도 source/index, whole SDK, held provider, copy와 resource 후검사를 각각 시도하고 최초 실패를 보존한다.
원래 source/SDK final과 새 source_guard_closure가 성공하고 proof upload의 artifact ID/digest가 있어야 cleanup한다.
cleanup 직전 copy 부모/현재 roster·file metadata/hash, 새 summary/raw의 각 파일 SHA와 exec-policy receipt를 다시 검사한다.
새 관측 PID는 기존 all-UID `/proc/stat` 비생존 검사에 합친다. 미관측 전체 PID/FD/DSO나 all-adopted reap을 주장하지 않는다.
원래 exact source owner/job namespace·UID/devino/owner marker 및 root내 directory-only u+w/비추적 삭제 규칙을 재사용한다.
copy는 job root 안에서만 제거한다. global SDK/다른 root는 건드리지 않는다.

collision8은 fixture에 sibling 등록4줄만 추가된 새 SHA를 연결한다. 원래8 body/selector/caps/closure를 바꾸지 않는다.
shared main/Focus/ledger의 allow/pin/edge 및 향후 absent argv 전환·실제 원본삭제는 부모의 별도 단계다.


## 288 실제 실패와 독립 copy 예산 교정 후보 — Actions UNRUN

source `288824f824eb682fc6fe1f2d896a086c64442a65`의
[run40184284624/job121141668791](https://github.com/bin9208/openpilot-rust/actions/runs/40184284624/job/121141668791)은
67unit/default14·15정책/legacy INT·TERM 성공 뒤 source-guard의 copy 전 admission에서 실패했다.
artifact12595996672는128,203,435B/SHA
`d903b2a5aad584af39366a72b1f3acb71cf72840a730ea9759ac89df3c556e60`/9390CRC다.
final 재관측 source-owner allocated는1,182,326,784B이며 사건 시점의 planned 합계는 저장되지 않았다.
기존 source+copy 합산2GiB 검사가 원인이라는 판단은 마지막 생성물·함수 순서와 정적 projection에 따른 추론이다.
그 실제 산술 tuple을 뒤늦게 만든 관측값으로 대체하지 않는다. 실제 CLI20/raw21/policy3는 미실행이며,
copy 미준비로 copy-final1/closureFAIL/cleanupskip였다. source/SDK/held/resource final은0이었다.

승인된 교정은 full tracked+실제 Git copy를 유지하면서 source2GiB와 독립 copy2GiB를 분리한 것이다.
이는 기존 정책의 명시적인 변경이며 과거 실패를 PASS로 다시 표기하지 않는다.
각 cap/space 거부 전에 `copy-admission-N.tsv`에 phase/label, source·copy allocated/planned,
source/job/proof/copy의 dev·ino, 각 cap·검사결과와 고유 FS별 free/필요량을 기록한다.
기존 combined2GiB predicate의 would_pass는 새 실행의 관측값일 뿐 수용 조건이 아니다.
sidecar는 create_new, record8192B/256회로 제한하고 경로·고정 label만 포함한다.
기존 source의 추가 계획 성장0, 전후 Git/source 불변과 모든 기존 resource/finalizer/closure/cleanup 조건은 유지한다.
source/copy 허용량은 각각2GiB로 보존하며 target8GiB/SDK/proof2GiB·시간·기능 본문·JQ·원문/패킷은 변경하지 않는다.

로컬 Bash 문법·함수 외 byte projection(위 예산 인자1개 예외)·diff만 확인한다.
새 admission 숫자와 copy 실제 적합성, CLI20/raw21/policy3 및 cleanup 완료는 새 Actions에서 검증해야 한다.
원본2 삭제/postdelete full/old27/R4/retirement credit는 계속0이다.


## e859 새 admission 통과와 copy 권한 보존 후보

source `e859449e3c755044a80a1cd1c4049a5205f33784`의
[40187751113/job121152043279](https://github.com/bin9208/openpilot-rust/actions/runs/40187751113/job/121152043279)은
새 before-copy admission을 실제 통과했다. origin1,182,470,144/copy actual0/planned1,347,977,216/
proof growth67,108,864/reserve536,870,912B, FS2049 free85,563,207,680/required1,951,956,992B,
source/copy cap pass1/check_result0이었다. old combined2GiB would_pass0은 이번 실행의 관측값이며
과거288의 누락된 planned tuple을 소급하지 않는다. 최종 source1,182,470,144/copy1,198,157,824B도
각 cap 안이었다.

뒤 copy HEAD/index/빈 cached diff/정확2 Python deletion은 통과했으나 canonical tar SHA가
source `0b13598669a1436233ab895852bbfd5de6b6c0e103137f5a5644f7dbd6c4937f`와
copy `3d5a58f0735a4d1a96c5cce4a47cbaea02430cad0e29f1d8bd912bc0fa6bdaf2`로 달랐다.
artifact12597463154(130459141B/SHA256 `370bdfc0c560cb06e31f440c8fb964478dc44861685c8d5c79ebd11c8913727e`)
9397unique/safe CRC를 감사했다. actual copy mode census는 없어 차이가 권한뿐이라고 단정하지 않는다.
source/SDK/held/resource final0, copy-final1/closureFAIL/cleanupskip이며 CLI20/raw21/policy3는 미도달이다.

현재 umask077 아래 일반 사용자의 기본 tar 추출은 archive mode를 제한한다.
[GNU tar 권한 문서](https://www.gnu.org/software/tar/manual/html_node/Setting-Access-Permissions.html)에 따라
private copy의 추출에만 `--same-permissions`를 더한다. root0700/전역umask077, 원본 source 및
전체 canonical tar hash 동일성 assertion은 유지한다. 고정 tracked2파일의 source/copy mode4행을
create_new `copy-source-modes.tsv`(4096B 이하)로 기록하고 held FD/SHA 후검사에 추가한다.
이는 진단 관측이며 전체 source·Git·현재19pin·부재·기능 비교를 대체하지 않는다.
로컬 Bash 문법·exact outside projection·diff만 확인하며 새 copy/CLI20/raw21/policy3 수용은 **GHA UNRUN**이다.

## 8fcbc 실제 다음 실패와 bounded raw index 관측 후보

run40190291372/job121159648434(source8fcbc9c0b61310727a761260ff91c83076973e75)의
artifact12598301220은128891557B/SHA256
`15236fbdb9ad81d933f9ff35f19b25f55663cd3fe064837909a834c3ce1fe29d`, 전체9399CRC PASS다.
`--same-permissions` 뒤 canonical TAR hash 두 줄은 모두
`17ed2534826885690928f6784f28c8a8f57af7b7bea42b6a6e01cf2bcf6f4b5a`로 같았다.
mode4행도 Cargo.toml 양쪽0644/2758B, helper 양쪽0644/116368B/UIDGID1001로 같았다.
source/copy 각각2GiB 및 copy+64MiB manifest+512MiB reserve의 실제 admission은 PASS,
old combined2GiB would_pass0이었다. 그 뒤 copied `.git` checksum에서 `./index: FAILED`로
중단했다. source rawindex는 전후cc170f02a3c29fe20631b1036c9d08d2e3131059594b7c846b6306f7ef7cfa46
동일이지만 copy의 실제 실패 bytes/직전 hash가 없어 어느 명령이 바꿨는지는 아직 미확정이다.
CLI20/raw21/sourceguard policy3는 미도달이고 sourceSDK finalPASS/uploadPASS/closureFAIL/cleanupSKIP다.

승인된 후속은 진단만 추가한다. `$proof/index-journal/journal.tsv`는 create_new/32KiB/최대32record이고
한 append FD의 전후 identity와 path identity를 매번 대조한다. cp-a 직전 source raw baseline,
cp 직후 source/copy, 원래 rev-parse/ls-files/diff--cached/diff--name-only 각 직후와 원래 `.git`
전체 hash 대조 직전에 source/copy의 dev/ino/hexmode/uid/gid/nlink/size/mtime/ctime/SHA256을 기록한다.
Git argv는 실제 인자에 `%q`를 적용해 기록하고 status도 원래 명령 직후 저장한다. 기존 네 Git argv,
expected HEAD/roster/빈 cached diff/정확2 deletion 및 전체 Git SHA 대조를 그대로 보존한다.
metadata/hash 불일치나 command 실패는 afterraw를 보존하고 sticky FAIL로 끝난다.
정상 경로는 source 사전 캡처1회와 postcopy/네 Git/pre-manifest의6 checkpoint×2로 총13캡처,
journal29행이다. pre-manifest는 실행 전 관측이며 SHA 명령의 성공 status로 기록하지 않는다.

입력 index는 파일당4MiB 이하의 owned regular/nlink1만 읽는다. 기존 collision capture의
`dd iflag=nofollow,nonblock,fullblock,count_bytes` 단일 입력 FD와 `conv=excl` 출력 패턴을 재사용한다.
size+1 read와 전후 source path full identity/부모 devino-modeuidgid를 검사하므로 short read/성장/
교체/alias를 정상 복사로 세지 않는다. dd 입력 FD의 fstat를 직접 출력하는 도구는 아니며 이 사실을
journal header에 false로 남긴다. 캡처는 별도 held FD와 `/proc/FD`를 통해 hash하고 그 FD/path identity를
다시 대조한다. 새 Python/Perl/compile/일반 controller는 추가하지 않는다. 사용하는 dd/mv/rm ELF와
ldd closure는 기존 hold/후검사로 묶는다.

보존 raw는 `source-before.raw`, `copy-before.raw`와 최초 실패의 `first-source-after.raw`,
`first-copy-after.raw`뿐이다. 정상 checkpoint의 임시 current raw는 두 baseline을 바꾸지 않고 제거한다.
첫 copy 비교부터 실패하면 accepted copy baseline은 만들지 않는다. 캡처 rename의 원래/새 이름과
새 identity도 journal에 명시한다. 입력 index 자체를 재hold·수정·재생성·복원하지 않으며 캡처 bytes를
native functional expected로 사용하지 않는다. read/shape 자체 실패 시 partial current raw와 해당
stage/side stderr·오류를 남기고 그대로 실패한다. 새로운 index가4MiB를 넘으면 진단도 FAIL하며
prefix/truncation을 full raw로 수용하지 않는다.

동시에 최대 raw4개=16MiB다. dd 실패 stderr는 per-command file limit4097KiB 안에 갇히고4096B 초과는
strict FAIL이다. operation 직전에는 두 raw와 이 bounded stderr 여유까지 포함한 pending16MiB+32KiB
(처음 source-only8MiB+32KiB)를 실제 proof2GiB와 filesystem free+finish reserve에 함께 admit한다.
기존 copy 전64MiB manifest계획/각 source-copy2GiB/proof2GiB/target8GiB/512MiB reserve는 올리지 않는다.
후속 stage의 실제 사용량도 기존 admission/finalizer에서 검사한다. 일반 hard quota나 peak 보장으로
표현하지 않는다. 새 retry/원문 waive/정상화/dirty 숨김/타이머 변경은0이다.

이번 변경의 로컬 검증은 Bash-n/소스·outside projection/diff뿐이다. journal/복사/Git 명령/fixture를
실행하지 않았으며 실제 최초 변경 지점·CLI20/raw21/policy3 및 cleanup은 새 source의 GHA UNRUN이다.
원본2·frozen inputs·원래67/14/legacy2·15정책은 그대로이며 old27/R4/retirement credit0을 유지한다.
