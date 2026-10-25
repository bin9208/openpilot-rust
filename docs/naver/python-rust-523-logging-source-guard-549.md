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

## #582 a387 실제 index refresh와 content-aware readonly 후보

run40194802656/job121173233700, sourcea3873834aefd264e37f3513c58c1f9a7ef4b32fc는
sourceguard에서 FAIL했다. artifact12600226056는132452267B/SHA256
`adf5e355ecd23e348e09f0fa9bdc72ac254f23349b524cd7b27c11e095526e7a`, 전체9425CRC PASS다.
67unit/14+untraced1/retainedonce/15정책/legacyINT·TERM2 및 source/index/SDK11쌍은 유지됐다.
18summary의 raw3499개/1743035682B도 actual size/SHA와 맞다. CLI20/rawfixture21/policy3는
미도달이며 sourceSDK/held/resource final0, closureFAIL/uploadSUCCESS/cleanupSKIP다.

journal 최초 변경은 `after-diff-files`, 실제 private copy `git --no-optional-locks … diff
--name-only -z` exit0 직후다. 앞선cp/rev-parse/ls-files/diff--cached는 identity/hash가 같았다.
source raw SHA는 `f7e059c72154d39339cb2f174e0fb64af0bd702c48fb7cb4bd953cd47babb62c`로
같고 copy만 `db6935da1c720e9c49666dd3cdc87dca5e682f9be1e9b2d45dfd16b22c8b0221`로
변경됐다. indexinode9489465→9489557/size2671826B다. inert DIRC/v2 해석상19490entry 중
19488의 ctime_s/ctime_ns/mtime_ns/ino만 바뀌고 경로/blob/mode/dev/uid/gid/size/flags 및
TREE/UNTR 전체SHA는 같다. checksum20B를 제외한 나머지 변경도 그 stat 필드 안에만 있다.
이는 차이 분류이며 raw 정상화·기준재hold·waiver가 아니다. 전체 copy TAR/Git checksum은 앞선
실패로 미도달이므로 a387에서 PASS로 소급하지 않는다.

[Git2.55 builtin/diff.c](https://raw.githubusercontent.com/git/git/v2.55.0/builtin/diff.c)의
222–234/494/608–609행과 [공식 config](https://raw.githubusercontent.com/git/git/v2.55.0/Documentation/config/diff.adoc)는
stat-only 내용동일 판정 뒤 index refresh 경로를 설명한다. 단순 `autoRefreshIndex=false`는
stat-only pair 제거도 끄므로 사용하지 않는다. 해당 private-copy 호출에만
`-c diff.autoRefreshIndex=false`와 `--exit-code --name-only -z`를 함께 적용하는 후보로 바꾼다.
[diff.c](https://raw.githubusercontent.com/git/git/v2.55.0/diff.c)의 기본 allow_external과
--exit-code→diff_from_contents(5047–5048행), name-only 콘텐츠 판정(6799–6801행)을 사용한다.
`--no-ext-diff`나 whitespace 무시·regex/다른config는 추가하지 않는다.

고정 stage/privatecopy root/전체10argv가 일치할 때만 expected_exit1이다. 이는 의도된 정확2개
삭제가 존재하는 Git `--exit-code` 계약이며 실제1은 journal에 그대로 남는다. 다른cp/Git3개는
expected0이며 해당diff의0이나2이상도 실패다. expected-exit1 전용1행만 추가하여 journal정상30행,
기존32행/32KiB상한을 유지한다. wrapper는 조건식으로 strictcheck를 감싸서 errexit를 억제하지
않고, strictcheck가 완료된 때만 stage성공0으로 반환한다. 이후 기존 정확2NUL목록cmp와
canonicalTAR/전체Git metadata checksum은 계속 필수AND다. status1만으로 기능을 수용하지 않는다.

원래 rawindex identity/hash·원문2·예산·타이머·67/14/15/legacy2/정책/실패후봉인·upload·cleanup
조건은 그대로다. 로컬 Bash 문법·변경span 역투영·diff만 확인하고 Git/fixture 실행0이다.
콘텐츠/name-only/actual exit1/index불변과 CLI20/raw21/policy3·전체closure/cleanup은 새 source의
**GHA UNRUN**이며 원본2보존·R4false/retirement credit0을 유지한다.

## #582 실제 수용 및 pre-delete 경계 — 2026-10-25

source `5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64`의 [40199544123/job121187580810](https://github.com/bin9208/openpilot-rust/actions/runs/40199544123/job/121187580810)가 전체 성공했고 단독 원문 감사를 마쳤습니다. artifact12602016721은133573683B/SHA256 `65452f43e3093f5e09dad47bffaba586a8b273a26f869c01cff0fad5bda194f8`,9594 safe unique/9594CRC입니다. 부모가 중복 다운로드하거나 재CRC하지 않았습니다.

source/copy rawindex는6checkpoint 전체에서 각 identity와 SHA를 유지했습니다. journal30행/10836B이고 고정 copied diff만 expected1/actual1, 나머지 명령은0입니다. stdout은 정확한 원본2삭제 NUL 목록이며 canonical TAR와 전체.git34 checksum도 일치했습니다. 정규화·재hold·복원·waiver는 없습니다.

실제 CLI20의 정확 argv/env·exit1·producer 전 거부, rawexec21과 정책음성3을 확인했습니다. 기존67unit·14lifecycle+untraced1·retained-once/current15·strictClippy·legacy INT/TERM2도 성공했습니다. raw108과 summary19/raw3521의 실제 ZIP bytes/size/SHA, immutable17쌍·held/provider·source/SDK 후검사·closure·업로드·소유cleanup2를 감사했습니다.

#582의 복사본 인덱스 변경 결함 수용 조건을 충족해 종료합니다. 저장소의 원본 Python2개는 아직 보존하며 post-delete full/R4/전체 #549/#523/retirement credit은0입니다. live UNIX socket1개는 업로더가 raw payload를 전송하지 않았고 해당 metadata/sentinel/필수 raw·report는 보존됐습니다. closed PID/cwd/FD/DSO·all-adopted-reaped 수용으로 확대하지 않습니다. 상세 기록은 docs/naver/python-rust-523-logging-source-guard-549.md와 private logging-5bee-audit.md입니다.

## #554 개별 타이밍 수용 — 2026-10-25

source `5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64`의 [40199544123/job121187580810](https://github.com/bin9208/openpilot-rust/actions/runs/40199544123/job/121187580810)에서 원래 Git status를 포함하는2초 경계와 기존14 lifecycle·retained raw비교·각prefix lexer·source/SDK/provider 봉인·업로드/cleanup이 모두 성공했습니다. 독립 전수감사도 이 개별 완료조건의 충족을 확인했습니다.

actual INT backpressure0.892221514초/TERM0.860565187초이며 timer 이동·2초/600초·trace class·신호/packet·자원 상한 변경은 없습니다. 67unit·untraced1/retained-once/current15·legacy INT/TERM2를 보존했고 summary19/raw3521의 실제size/SHA 및 unknown/truncated 거부를 확인했습니다. artifact12602016721=133573683B/SHA `65452f43e3093f5e09dad47bffaba586a8b273a26f869c01cff0fad5bda194f8`의9594CRC와 immutable17쌍, 후검사·cleanup2도 통과했습니다.

#554를 이 검증 범위에서 종료합니다. CPU·Git·BPF 하나의 성능 인과나 다른 호스트에서의 시간 보장은 입증하지 않았습니다. 원본2 실제삭제·post-delete full/R4·전체 #549/#523/retirement credit0은 별도입니다. 자세한 근거는 private logging-5bee-audit.md와 docs/naver/python-rust-523-logging-source-guard-549.md에 연결합니다.

## #549 postdelete WIP 보완 및 부모 인계 — 2026-10-25

작업 루트는 `/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
`imported/continuation-523-pause-20261025-volta.md`는 이 루트의
`.analysis/scratch/2026-10-25-523-completion/imported/`에서 읽었다. 이전 checkout과
기본 dev-md checkout은 변경하지 않았다. 앞선 #582/#554 실제 결과와 본문은 유지하며,
이번 소스는 **postdelete 기능·빌드·Clippy·Actions UNRUN인 후보**다.

인계 WIP에서 실행을 막던 두 누락을 실제 수정했다. root fixture의 `command_reference`는
prebuilt/reference argv 뒤에 `retained::source_policy(&mut command)`를 호출한다.
따라서 legacy INT/TERM과 collision8도 retained fixture와 같은 finite 정책을 사용한다.
collision helper의 미정의 `source_policy`/`source_policy_env`도 초기화했다. 미설정 또는
`present`는 기존 argv를 유지하며 `absent`만 `--source-policy absent`와 fixture env를
추가한다. empty/unknown은 거부하며 파일 존재에 따라 정책을 자동 선택하지 않는다.
child `retained.rs`의 인계 구현은 그대로 유지했다. root 수정은 이 호출 한 줄뿐이다.

jq 기본값은 `//`에서 `has` 분기로 바꿨다. 인자가 없을 때만 present/current를 기본값으로
사용하며 명시적 null/false/empty/unknown은 거부한다. current binding은 같은 source_policy를
명시해야 하고 `78896f19db29d5cbe305d6f788bac9d0db4ce552`는 current로 수용하지 않는다.
과거 788의 missing-field binding은 `policy_scope=historical-788`, `source_policy=present`,
`prefix=first_int`, 정확한 고정 commit일 때만 허용한다. 과거 원문 argv/env/binding은 수정하지
않는다. 출력의 `historical_data_only`와 scope/policy를 통해 현재 absent 결과와 구분한다.

retained→legacy→source-guard의 재사용 outcome/receipt와 native raw 비교에도 정책 일치를
추가했다. legacy complete와 collision candidate에 source_policy를 기록한다. sampled source
checkpoint는 실제 root/rust/tools FD·identity, 이름 전체 scan, HEAD/index의 원본2 부재와
첫 실패 marker를 검사한다. 이것은 연속 부재 또는 closed PID/cwd/FD/DSO 증명이 아니다.
private copy diff는 present에서만 고정 exit1/정확2 NUL 삭제 목록, absent에서는 고정
exit0/정확 empty를 요구한다. 실제 exit를 보고 expected를 선택하지 않는다.

새 schema 음성 통제는 actual first_int와 source-absence의 양성 report/binding 각각에 대해
정책 인자4(null/false/empty/unknown), scope 인자4, binding 필드5(missing/null/false/empty/unknown),
현재 binding의 historical scope 오사용1, 총14개다. 각각 jq exit5·stdout empty·정확한 정책 오류를
함께 요구한다. 각 묶음 전체를 기존 `run`의 60초 timeout/자원 admission으로 제한한다.
기존 trace 음성4/3과 별도 `source-policy-controls/complete.json`을 남기며 기능은 GHA에서만
검증한다. 기존 67unit·14 lifecycle 본문+untraced1·retained-once/current15·108raw/700585B·
current19·legacy2·CLI20/raw21·collision8·2초/600초·cap·unknown/truncated strict 거부는
유지했다. 새 통제는 lifecycle 타이머 바깥의 정책 단계다. 원본2 삭제 WIP는 그대로이며
복원본/stub/encoded 본문을 추가하지 않았다. R4false/retirement credit0을 유지한다.

부모가 수행할 통합은 다음과 같다. 이 작업자는 workflow/main/Focus/pins/ledger/Cargo를
변경하지 않았고 stage/commit/push/이슈 갱신/GHA 실행도 하지 않았다.

1. `.github/workflows/python-rust-523-logging.yml`의 actual retained/legacy-runtime2/source-guard에
   `NATIVE_LOGGING_SOURCE_POLICY=absent`를 고정한다. 현재 policy 호출은 helper가 전달한다.
   과거 788 replay 호출에는 `--arg policy_scope historical-788 --arg source_policy present`를
   추가한다. replay 출력도 present/historical-788/historical_data_only=true를 요구하고,
   current 정책 출력에는 absent/current/historical_data_only=false를 요구한다. 과거 두 JSON과
   ZIP/member SHA·literal 경로·historical-only/current15_accepted=false 조건은 유지한다.
2. 같은 workflow의 helper literal pin(qualify/legacy/source-guard/cleanup의 CURRENT_HELPER_SHA256),
   jq literal pin(replay policy_sha/qualify/source-guard)을 아래 최종 SHA로 교체한다.
   `.github/workflows/python-rust-523-logging-collision8.yml`의 `COLLISION8_HELPER_SHA256`도 교체한다.
   `COLLISION8_BATCHES_SHA256`와 batch worker는 변경하지 않는다.
3. collision workflow의 admit/oversize-controls/run/final-source/cleanup admit 모두 같은
   `NATIVE_LOGGING_SOURCE_POLICY=absent`가 필요하다. current outcome/candidate/reuse의 정책
   일치와 기존 source/SDK/final/upload/cleanup AND를 유지한다. logging global proof의
   `logging-source-policy.parents.tsv`, `logging-source-policy.json`, collision global proof의
   `c8-source-policy.parents.tsv`, `c8-source-policy.json`을 봉인·업로드·closure에 연결하고
   동일 prefix `.failed`가 부재인지(정상 노드와 dangling symlink 모두) 확인한다. 새 schema
   complete14 두 개도 closure에 연결하며 기존 trace controls4/3을 대체하지 않는다.
4. `.github/workflows/rust.yml`의 `logger-runtime` step19 기존 Rust CLI argv에
   `--source-policy absent`를 추가한다. fresh CPP step18과 나머지 job/조건/타이머는 유지한다.
   별도 maintained caller 실제 성공은 dedicated legacy2로 대체하지 않는다.
5. `.github/workflows/python-rust-523.yml`의 허용 범위/Focus pin과 부모 ledger를 갱신한다.
   root fixture 한 줄 변경도 포함한다. 기존 MEMBERS108/current SOURCE_FILES19 및 역사 자료의
   marker/pin은 삭제하지 않는다. 새 삭제 commit의 clean HEAD/index/literal absence에서
   fresh checker/probe/fixture/builder와 기존 전체 GHA gate를 검증하고 exact SHA 결과를 감사한다.

| 대상 파일 | 이번 최종 SHA256 |
| --- | --- |
| `tools/ci/logging_retained_549.sh` | `1504c949d8a8f39c7e821b87db0601f5f3ce3dd7aa0eb84be13ff2f411c9d59b` |
| `tools/ci/logging_retained_exec_policy_549.jq` | `3b84adc7a2f1bd147841382fc35a9f0e456d357005206966c586308061f492b7` |
| `tools/ci/logging_collision8_549.sh` | `3b9ebc5937a412f6f68090dbf81bdaf12ad1b8cc002d1ffa029e4533dd6395bd` |
| `rust/crates/dev-tools/tests/native_logging_lifecycle.rs` | `5a029df81ef9216c9d5d920c6747b7c01d1cb238245bd4fd3b8a3041510aced0` |
| `rust/crates/dev-tools/tests/native_logging_lifecycle/retained.rs` | `9f7439a21aed62402ea4df6fb69f5ee56c0fc57a1fdaf6c272d6ffdcb6d3e46f` |

로컬에서는 두 helper의 `bash -n`, 새 schema 통제의 인용을 푼 내부 Bash 본문 `bash -n`,
root/child 두 파일의 `rustfmt --edition 2021 --check --config skip_children=true`, 소스/diff와
`git diff --check`만 확인했다. 모두 PASS이며 jq 평가/프로젝트 CLI/Cargo/build/test/Clippy/
원본·모델 실행/GHA 실행은0이다. 새 strict 정책의 실제 거부와 postdelete 전체 수용은 아직
**GHA UNRUN**이다. 다른 기능군의 동시 WIP는 보존했다.

## source126 삭제 후 최초 결과와 수정 — 2026-10-26

[retained40209583376/job121217757349](https://github.com/bin9208/openpilot-rust/actions/runs/40209583376/job/121217757349)은 실제 부재 정책의 빌드/두 strictClippy·선택 단위67개와 Git source-cache 준비까지 성공한 뒤 실패했다. artifact12605605211(12335462B/SHA256 `0f218f2ab955393e587cc3db36b77124b32091dbe904eb7a2e701801abb6b23b`)의2494 unique/safe/전체CRC를 확인했다. 최종 phase는 git-source-cache-prime/exit1이며 held-final=0이다. source-cache complete는 성공했으므로 이를 Git cache 실패로 분류하지 않는다. 직후 untraced 환경의 기존6개 개수 검사에 새 absent 정책키1개가 포함돼 실패한 것을 source/부분 출력으로 확인했다. 기존6키/순서·evidence index5를 유지하고 present6/absent7과 정확 정책키를 검증하도록 수정한다. lifecycle14/현재15/CLI/raw/policy/source-guard는 미도달이다.

[collision40209583383/job121217757350](https://github.com/bin9208/openpilot-rust/actions/runs/40209583383/job/121217757350)은 실제 fixture 실행 전 root lifecycle 소스의 옛 SHA pin에서 실패했다. 해당 현재 파일의 검토한 부재정책 한 줄을 포함하는 정확 SHA로 고정한다. 검사·기한·거부 조건을 없애지 않는다. 두 실패의 원본 삭제 credit은0이며 수정 후 실제 부재/재등장·호출부 수용이 필요하다.

## DD collision 실제 lexer 거부와 capture 보완 — 2026-10-26

[collision run40212410241/job121226282810](https://github.com/bin9208/openpilot-rust/actions/runs/40212410241/job/121226282810)의 source는
`dd6157c3984a73a59fb808b551cdab73f04dba35`다. 부모가 받던
`.analysis/scratch/2026-10-25-523-completion/ci/12606243075.zip`은 확인 시16700250B이며,
중앙 디렉터리가 있고 열린 writer FD가 없었다. 부모 session11862는 이 작업자의 session
namespace에서는 조회되지 않았다. 중복 다운로드 없이 기존 ZIP을 inert로 읽었다.
7672개 전체 이름의 safe/unique·비암호화·symlink 없음과 전체CRC를 확인했고,
expanded162055841B, ZIP SHA256은
`c8c3061eb7d4c7f04f48884838d493913d5a1965436711d4737c28cbcc26f49c`다.
ZIP의 파일을 추출·실행하거나 GHA를 시작하지 않았다.

실제 실패 step은 `Build fresh reference only and observe current collision8`, 내부 phase는
`inspect-checker-build`다. checker/probe/fixture/reference build와 collision8/inspect-collision의
command/watcher exit는 모두0이다. collision fixture는1 selector 성공이고 policy check는
`true`, rich report는 exec9(fixture1+checker8), raw10/665882B다. 그 뒤 inspector stderr가
정확히 `check-native-logging: trace syscall name invalid`(49B)를 남겼고 command exit1이다.
전체 cohort 성공으로 확대하지 않는다. outcome은 source_policy=absent,
command_exit=1/finalizer_exit=1/immutable_seals=false/collision8_current_pass=false다.

문제 원문은 `c8/cargo-trace/checker.11407`(SHA256
`ff97a61bb76b1ba61ddacdf8be6212999368767e4a111d0d5e5331359dfd6c4b`)의 두 줄이다.

```text
1792946238.358436 ???( <unfinished ...>
1792946238.359013 +++ exited with 0 +++
```

부모raw `checker.11406`에는1792946238.357738의 CLONE_THREAD 등을 포함한 clone3→11407,
1792946238.358411의 exit_group(0)이 있다. 이는 짧은 thread의 관측 순서이며 `???`를
어떤 syscall로 복원하거나 안전한 exit로 수용할 근거가 아니다. build raw 전수의
checker3801파일/36616993B/16993행, probe441/2724104B/1531행,
fixture1812/73043143B/6871행, reference10/5599181B/16618행에서 이 이름은 한 줄뿐이다.
현재 Rust `trace::name_end`가 이를 거부한 것은 유지해야 할 동작이다.

resource/source-policy sticky marker와 admission-failure/first-failure 파일은 이 ZIP에 없다.
source·system-headers·gcc-provider·reference·cases의 before/after roster/hash와 after의
stable roster는 모두 같다. held158행도 순서만 다르고 정렬 후 bytes가 같다. 따라서 이 자료를
용량 초과나 source 재등장 실패로 분류하지 않는다. finalizer_exit1의 추가 원인은 기존 자료로
확정하지 못했다. 특히 alias별 관측 결과가 이전 proof에 따로 없어 모든 finalizer 조건의 성공을
소급 주장할 수 없다.

수정은 `tools/ci/logging_collision8_549.sh`에 한정한다. build trace만 기존 retained capture의
`--seccomp-bpf`를 요청하고, 실제 보존된 strace help의 지원을 확인한다. `trace=$profile`,
`--kill-on-exit/-ff/-ttt/-yy/-xx/-v/-s65536`, argv/env, raw/lexer 및 runtime collision trace는
유지한다. `build/$label.capture.json`에는 요청 여부와 activation_proven=false를 기록한다.
BPF 실제 활성이나 `???` 발생 인과·해소는 이번 로컬 소스 검토로 입증하지 않았다. unknown을
skip·치환·재시도하지 않으며 다음 collision GHA에서도 같은 raw가 생기면 그대로 실패해야 한다.

명령/감시자/소유 group 중 하나라도 실패하면 create-new
`c8/command-first-failure.txt`에 최초 phase와 세 exit를 보존하고 CI 로그에 최대4096B stderr
prefix를 표시한다. stderr 전체는 기존 build 파일에 그대로 남긴다. 기존 exit-codes.tsv의3열도
그대로다. finalizer는 held/path/FD/alias의 실패 메시지, tree별 tree/roster/hash exit와
미도달 표시, source-policy/resource stdout/stderr 및 각 exit를 outcome의 추가 필드에 남긴다.
기존 finalizer AND와 immutable_seals/candidate 조건은 유지하며 failed marker의 dangling
symlink도 거부한다. proof의 기존 용량·원래 timeout·PGID/watcher·readonly 후검사와
old27/runtime2/caller/source_absence/closed PID·cwd·FD·DSO/retirement 경계는 완화하지 않았다.

부모 인계 pin은 다음과 같다. `logging_collision8_batches_549.sh`와 Rust/lexer/tests는 변경하지
않았으므로 Rust test 추가 통지는 필요하지 않다. workflow/main/Focus/원장/commit/push/GHA는
부모 소유로 변경·실행하지 않았다. 다음 필요한 검증은 변경된 collision만이며 retained 재실행은
필요하지 않다. collision workflow의 `COLLISION8_HELPER_SHA256`와 관련 Focus pin만 새 helper
SHA로 갱신한다.

| 파일 | SHA256 |
| --- | --- |
| `tools/ci/logging_collision8_549.sh` | `5b01ff90076714db598915bcef27c80e114d2c44f355c321b791dd1b83b2467d` |
| `tools/ci/logging_collision8_batches_549.sh` (불변) | `52d4ca212c28cad44579360b8531c915f6fe8e3060c0b480f0f7ec1cdb659a79` |

로컬은 ZIP inert 감사, Bash 구문과 diff 검토만 수행했다. build/test/Clippy/원본/모델/프로젝트
CLI/jq 정책 평가/GHA 실행은0이며 수정 후 collision 기능은 **GHA UNRUN**이다.

부모가 별도로 감사한 retained run40212410474/job121226283640/sourceDD의 전체SUCCESS와
artifact12607036666(136138137B/SHA256
`fcdd3b591e544cf57ccfd0038ec7ef6c7a1f7da6126ae92175f82d5c6aafc681`,9696safe unique·nosymlink·전체CRC)는
부모 보고로 수령했다. 이 작업자는 해당 ZIP을 다시 다운로드/CRC하지 않았다. candidate
exit/held/untraced/paired=0, CLI20/providers·parents unchanged, legacy2 및 absent 성공을 유지한다.
main logger는 APT 단계의 Azure HTTP 다운로드 지연으로20분 timeout되어 actual step19에
미도달했다는 부모 조사도 수령했다. 따라서 main maintained caller+collision이 남아 있으며
원본2 full retirement credit은 계속0이다.

## #549 종료 전 읽기전용 증거 대조 — 2026-10-26

[issue549 현재 본문](https://github.com/bin9208/openpilot-rust/issues/549)과6개 댓글을 읽었다.
본문의 원본2 보존/21core·14actual UNRUN/설계 초기 상태는 현재 증거보다 오래됐다. 이 절은
부모의 종료 판정을 위한 감사이며 issue/inventory/families/실행원장/master record를 갱신하지
않는다. 원본2의 실제 삭제와 기능 성공만으로 자동 credit을 부여하지 않는다.

**현재 결론:** predelete 동일 ELF의 기존 감사 연결은 찾았으며 누락으로 분류할 이유가 없다.
DD postdelete retained는 부모 독립 감사 완료이고 f1 collision 및 main exact caller는 실제
전체 SUCCESS다. f1 collision/main의 artifact 실물·CRC·필수 proof에 대한 부모 최종 감사와
관련 원장 갱신은 아직 이 작업자의 수령 증거에 없으므로, 이 절만으로 #549 종료 또는
`pending→replaced-local`을 실행하지 않는다. 아래 남은 실물 감사가 성공하면 두 파일의 좁은
family 수용을 기록할 수 있으며, 이를 전체 필수 PR/모델/전체 #523 수용으로 합치지 않는다.

### 조건별 연결

| issue 본문의 조건 | 확인한 증거 | 판정 및 제한 |
| --- | --- | --- |
| authenticated archive108의 strict native 준비 | 5bee [40199544123/job121187580810](https://github.com/bin9208/openpilot-rust/actions/runs/40199544123/job/121187580810)의 기존 독립 감사:108/700585B=TAR70+innerZIP37+current reference1, source19와 전후 입력 봉인, R11 outer05ff6420…/gzip2dfea2b7… 유지 | 감사 완료 기록 재사용. 원문/옛 ELF/Python 실행·새 expected 생성 없음 |
| 실제 기본 Linux/X64 fixture·source/SDK/provider/lineage/reap·600s/2s 유지 | 같은5bee 감사의 whole source/index·SDK165files/188roster/4program·held provider·관측 PID/pidfd/reap·cleanup 및 #554 원래 timer 검증; DD postdelete 부모 감사가 동일 경계를 확인 | 정의된 held/관측 범위 수용. 전체 PID/cwd/FD/DSO closure·모든 adopted reap을 증명하지 않으며 false flag 유지 |
| 21core·14actual·fresh250/697 대 원문249/694·7console/rate/thread/signal/error | 5bee의67unit(core27=기존21+guard6/import9/trace24/summary7),14+untraced1/retained-once/current15,250/697 대249/694. DD [40212410474/job121226283640](https://github.com/bin9208/openpilot-rust/actions/runs/40212410474/job/121226283640) 전체 성공·부모 artifact 감사 완료 | 현재 기능 수용 기록 있음. R4_acceptance/lifecycle_reuse의 기존 의미와 false/0 값 보존 |
| old27의 미수용10을 닫거나 정확하게 미완료 유지 | 아래10행을 historical ledger pending으로 유지한다. source `40eb0429013cd853c1e74dc9530cdb5f484aa32c`의 historical17은 별도 | 이 조건은 정확 pending 유지로 충족할 수 있다. old27 재실행을 새 요구로 추가하지 않는다 |
| 같은 current ELF의 predelete 및 명시 absence/reappearance negative 뒤 caller 전환/원본2 제거·봉인/upload/cleanup | 아래 predelete 동일 ELF 연결, DD 실제 absent CLI20/raw21/정책14 두 묶음·legacy2, f1 actual main19와 collision 전체 SUCCESS, f1 HEAD/index/작업트리의 literal2 부재 | 기능·단계 증거 있음. f1 artifact 실물 독립 감사가 남아 있어 최종 등록은 보류 |
| exact source Actions·필수 PR/통합 상태와 inventory를 실제 범위로 갱신 | DD와 f1 각 source/run을 구별하고 아래 artifact/source pin 연결. 현재 두 inventory/family 행은 pending임을 읽기전용 확인 | 부모가 최종 감사 후 두 경로/호출 edge/issue 기록을 갱신해야 한다. 전체 필수 PR 모델 실패와 통합·기기 수용은 별도 미완료 |

### predelete의 같은 current ELF 조건

기존 단독 감사 기록
`.analysis/scratch/2026-10-25-523-completion/imported/recovery/saved/logging-5bee-audit.md`의
`source/SDK/provider/budget/final/cleanup` 절은 checker SHA `3c9252ec…`, probe SHA
`180a4fe5…`가 `logging/exec-policy/binding.json`과
`logging-source-absence/inputs.before.json`의4개 provider에 **같은 identity/SHA**로 연결됐다고
명시한다. 같은 job의 retained·legacy에서 실제 선택/hold한 checker/probe/fixture와 fresh
builder를 source-guard가 재검증한 경로이며, private copy의 CLI20/raw21은 그 checker를 사용했다.
원본2 present fixed-SHA, retained14/once/current15와 negative20/producer0, 전후 provider 불변,
source/index/SDK/copy/closure/upload/소유 cleanup 성공까지 기존 감사에 함께 기록돼 있다.

따라서 단순히 predelete 테스트가 있었던 다른 ELF를 끌어온 것은 아니다. 같은 current ELF의
present 기능과 private-copy absent/reappearance 음성을 묶는 **5bee 안의 증거**가 존재한다.
이는 DD/f1에서 새로 빌드한 checker가 5bee와 byte 동일하다는 주장이 아니며, private-copy의
metadata 양성을 full absent 기능으로 바꾸지도 않는다. 실제 postdelete full 기능은 DD가
별도로 제공한다. 인계된5bee 기록에는 전체 checker SHA literal과 상세 JSON/ZIP 본체가 없으므로
새 SHA를 추정해 적지 않았고, 기존 독립 감사 결과를 재사용한 사실을 명시한다.

5bee artifact12602016721=133573683B/SHA
`65452f43e3093f5e09dad47bffaba586a8b273a26f869c01cff0fad5bda194f8`,9594safe unique/전체CRC,
expanded1969479788B의 기존 감사 manifest도 확인했다. 재다운로드/재CRC0이다.

### DD postdelete와 f1 actual 성공을 잇는 범위

DD=`dd6157c3984a73a59fb808b551cdab73f04dba35`, f1=`f1c730b029df77ed2731f86ddf2d224e74bb9f50`다.
DD→f1 Git object를 다음 범위로 직접 비교했고 변경0이었다:
`tools/ci/logging_retained_549.sh`, `logging_retained_exec_policy_549.jq`,
`native_logging_check.rs`와 하위 모듈, `rust/crates/logging`, root lifecycle fixture와 하위 모듈.
이는 성공한 DD retained를 불변 기능 증거로 재사용하는 source 비교이며 ELF/전체 dependency
closure가 같다는 증명으로 확대하지 않는다. retained 재실행0이다.

DD artifact12607036666은 부모가 독립 감사한136138137B/SHA
`fcdd3b591e544cf57ccfd0038ec7ef6c7a1f7da6126ae92175f82d5c6aafc681`,9696safe unique/non-symlink/전체CRC,
expanded1973631690B다. logging candidate exit/held/untraced/paired=0,
source_policy=absent, 실제 CLI20/providers·parents unchanged, legacy INT/TERM2,
source/index·whole SDK file/roster/program·업로드/cleanup 성공을 부모에게 수령했다.
이 작업자가 해당 ZIP을 다시 읽거나 감사했다고 표현하지 않는다.

f1 [collision40217812550/job121242667148](https://github.com/bin9208/openpilot-rust/actions/runs/40217812550/job/121242667148)은
API metadata와 성공 로그로 전체 SUCCESS를 직접 확인했다. 실제 step10 build/current8,
step11 whole SDK, step12 source/index, step13 narrow closure, step14 upload,
step15 source cleanup, step16 owned job cleanup이 모두 성공했다. closure의 actual8/exec8/exit1×8/
spawn0/foreign 동일성과 parent baseline·failed 부재·current absent·worker exact-once coverage를
요구하는 기존 AND가 통과했다. helper SHA는
`5b01ff90076714db598915bcef27c80e114d2c44f355c321b791dd1b83b2467d`, batch worker는
`52d4ca212c28cad44579360b8531c915f6fe8e3060c0b480f0f7ec1cdb659a79`다.
업로드 API와 로그가 가리키는 artifact12608900233은39232753B/SHA
`5bd11d1bb0ba5424208c8276bb1338dc89d67c24257956bfb02bdf6cb3ab7e91`다.
이는 서버 metadata/업로드 digest 확인이며 ZIP CRC·member/raw 전수 감사 완료라는 뜻은 아니다.
부모의 예정 다운로드와 중복되는 artifact ZIP 다운로드0이다(REST metadata GET만 수행).

f1 [main exact logger40217812664/job121242667579](https://github.com/bin9208/openpilot-rust/actions/runs/40217812664/job/121242667579)도
감사 도중 전체 SUCCESS로 완료됐다(API updated_at2026-10-25T18:00:31Z).
처음 snapshot은 Miri 완료/ASAN 진행이었으나 마지막 snapshot은 ASAN·artifact upload·source/index
final·ownership proof·source/target cleanup·job cleanup까지 전부 성공이다. 오래된 source126의
APT timeout을 이 성공으로 다시 이름 붙이지 않는다.

f1 Git object의 main `.github/workflows/rust.yml`은499697B/SHA
`4c538b38a252290a2e18ad4b523ee8a45d36b84fd293add47a8be3154685bf36`이며 exact caller workflow가
이 전체 SHA를 실제 검증했다. pin 검사 step 하나만 제외하면 parsed `logger-runtime` job과
global env가 main과 완전히 동일함을 읽기전용 대조했다. main 배열 index19의 maintained Rust
caller가 exact workflow 배열 index20/GitHub step22로 실제 성공했다. exact argv의
`--source-policy absent`를 유지하고 console/transport/rate-limit 비교를 통과했으며,
후속 full native logs/media/fallback/signal·diagnostic·flush/close·Miri·ASAN도 성공이다.
별도 retained 성공을 main caller로 대체한 판정이 아니다.

main artifact는 서버 metadata상 다음2개다. 두 ZIP 모두 이 작업자는 다운로드/CRC하지 않았다.

| artifact | bytes | API digest SHA256 |
| --- | ---: | --- |
| 12608477043 `rust-route-logger-evidence` | 25035246 | `564bdcbcdfd667f4e07e9ac2686cae33cc4306b696db47b81d91333de0f47061` |
| 12608786367 `private-rust-logger-runtime-0-40217812664-1` | 291002 | `9324fda49968cd322e004e994f045549a7346af59ffc019dc4f75ed7bb26d62b` |

### old27의 정확한 pending10 유지

기존 exec 문서의 historical17은 old C++ first3+runtime12+occupied-output2다.
현재 selector의 RESUME 분기와 원래 조합을 대조하면 남은10은 아래와 같다. 새 cohort 성공을
이 표의 역사 pending 해제로 대입하지 않는다. 특히 현재 retained의 native-first3은
historical C++ first3과 entry가 다르며, old17의 current provider/context bridge도 별도 HOLD다.

| pending 항목 | 기존 문맥 | 현재 별도 증거와 구분 |
| --- | --- | --- |
| collision/no-reference/live-directory | prebuilt=false | f1 actual collision8에 있으나 old27 current-epoch로 승격하지 않음 |
| collision/no-reference/symlink | prebuilt=false | 동일 |
| collision/no-reference/dangling-symlink | prebuilt=false | 동일 |
| collision/prebuilt/file | prebuilt=true | 동일 |
| collision/prebuilt/directory | prebuilt=true | 동일 |
| collision/prebuilt/live-directory | prebuilt=true | 동일 |
| collision/prebuilt/symlink | prebuilt=true | 동일 |
| collision/prebuilt/dangling-symlink | prebuilt=true | 동일 |
| legacy runtime/directory/SIGINT | old run_cli의 live C++/native 준비 뒤 runtime barrier | DD legacy2의 INT 실제 성공은 별도 cohort 수용이며 old27로 재계수하지 않음 |
| legacy runtime/directory/SIGTERM | 같은 원래 entry, 별도 TERM case | DD legacy2의 TERM 실제 성공도 동일 경계 |

이10개를 정확 pending으로 남기므로 issue의 old27 조건을 무시하거나 새 재실행을 요구할 필요가
없다. `old27_accepted`, `pending_runtime2_accepted`, closed PID/cwd/FD/DSO,
`all_adopted_reaped`, R4/full acceptance false 값은 기존 receipt에 그대로 남긴다. 부모가 두 파일의
family 교체를 수용해도 과거 receipt의 retirement_credit0을 true/2로 다시 쓰지 않는다.

### 부모의 최종 등록 전 남은 확인

1. 부모가 예정한 f1 collision artifact12608900233의 실제size/SHA·safe/unique/type/CRC,
   outcome/candidate/whole-prefix raw·batch coverage·source/index/SDK·후검사/cleanup 근거를 감사한다.
   관측값을 확인하기 전 최종 peer audit 완료로 쓰지 않는다.
2. main의 두 artifact를 실제 audit하고 native comparison/report와 final source/index·ownership·
   upload/cleanup 증거를 연결한다. job 전체 성공 metadata와 artifact bytes 감사는 구분한다.
3. 두 감사가 성공하면 `rust/tools/check_native_logging.py`, `rust/tools/native_logging_transport.py`의
   inventory/family pending2와 maintained caller edge를 실제 Rust 대체 경로·exact source/run/artifact로
   갱신한다. 예상 baseline 변화는 현재294/3/3283에서296/3/3281이지만 다른 병렬 수용이 있을 수
   있으므로 부모가 최종 원장을 기준으로 집계한다. 이 작업자는 숫자/상태를 변경하지 않았다.
4. #549 본문의 낡은 초기 상태를 위 증거/경계로 갱신한 뒤 가족2개 범위의 종료 여부를 판정한다.
   전체 필수 PR의 모델 실패와 #523/#296 통합·전달·기기 수용은 별도 미완료로 남긴다. 여기의
   `replaced-local`은 저장소 대체/검증 분류이며 실차 CPU 절감·차량 설치 수용을 뜻하지 않는다.

이번 작업은 기존 logging 기록 append만 썼다. issue/원장/inventory/families/master/WF/코드/
commit/push/GHA는 변경·실행하지 않았으며 artifact ZIP 신규 다운로드·중복 CRC도0이다.

## predelete 동일 ELF 최종 판정과 c8 f1 감사 수령 — 2026-10-26

**predelete의 같은 current ELF 조건은 기존 증거로 충족 판정 가능하다.** 새로운 예외나
검증 면제에 의존하지 않는다. 이 결론은 5bee의 기존 독립 실물 감사와 그 exact source의
실제 호출·재사용 검사를 연결한 것이다. 이 작업자가 원래 ZIP을 다시 감사했다는 뜻은 아니다.

1. 5bee/run40199544123/job121187580810은 original2 present 상태에서 full14/once/current15와
   legacy2를 실제 성공시킨 뒤 source-guard CLI20/raw21도 성공했다. 기존 독립 감사가
   retained binding과 guard의4 provider 입력에 checker/probe의 **동일 identity/SHA**를 명시한다.
2. 당시 [helper169행](https://github.com/bin9208/openpilot-rust/blob/5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64/tools/ci/logging_retained_549.sh#L169)은
   retained에서 실제 선택·hold한 checker의 Cargo log/경로/identity/SHA를 reuse receipt에
   기록한다. [325행](https://github.com/bin9208/openpilot-rust/blob/5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64/tools/ci/logging_retained_549.sh#L325)의
   재사용 admission은 source/job owner·Cargo target/kind/src/profile·원래 경로·identity/hash를
   대조하며, checker/probe/fixture를 다시 빌드하지 않는다. source-guard가 같은 재사용 함수를
   호출하는 지점은771행이다. 두 Bash 호출 사이 FD가 연속해서 열려 있었다는 주장은 하지 않는다.
3. 당시 [source_absence.rs263행](https://github.com/bin9208/openpilot-rust/blob/5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64/rust/crates/dev-tools/tests/native_logging_lifecycle/source_absence.rs#L263)은
   `CHECKER` 자체를 hold하고275행에서 actual source의 `rust/target/debug/check-native-logging`과
   정확히 같은 경로인지 확인한다.305행에 입력4개의 metadata/hash를 저장하고,
   [323행](https://github.com/bin9208/openpilot-rust/blob/5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64/rust/crates/dev-tools/tests/native_logging_lifecycle/source_absence.rs#L323)의
   `Command::new(CHECKER)`로 retained/legacy×literal2×node5를 실행한다. 각20회와 종료 후에
   held 입력 전체를 재검증한다. 기존 실물 감사는 이 raw exec21의 exact argv/env·checker20의
   exit1·producer0 및 provider 불변도 확인했다. 소스상 경로 일치만으로 실제 성공을 대신한
   판정이 아니며, 앞의 독립 실물 감사와 결합한다.
4. Git object의5bee→f1 범위에서도 `native_logging_check.rs`와 하위 모듈,
   `source_absence.rs`, `rust/crates/logging`은 변경0이었다. 이는 관련 source 연속성이고
   재컴파일된 DD/f1 ELF의 byte 동일성이나 전체 dependency closure 증명은 아니다.

따라서 수용의 정확한 표현은 **“5bee의 같은 current checker ELF로 present 기능과 private-copy
absence/reappearance 음성을 실제 통과했고, 원본2 실제 삭제 뒤에는 fresh DD의 full absent
기능과 f1의 main caller/collision을 별도로 성공시켰다”**다. private copy 음성의
`absent_root_full_functional_acceptance=false`와 당시 R4/credit0은 그대로 보존한다.
추가 predelete 재실행, original 소스 복원, 과거 receipt의 재작성은 필요하지 않다.

부모가 f1 collision artifact12608900233의 독립 실물 감사를 완료했다고 수령했다.
로컬 `ci/12608900233.zip`=39232753B/SHA
`5bd11d1bb0ba5424208c8276bb1338dc89d67c24257956bfb02bdf6cb3ab7e91`,9495safe unique/non-symlink/전체CRC,
expanded396590632B다. source/index/SDK files·roster·program의5쌍이 모두 byte 동일하며,
outcome phase=collision8-observed, command/finalizer/held/tree/source-policy/resource/stop=0,
immutable=true/collision8_current=true/logical8이다. candidate의 checker exec8/exit1×8/spawn0,
CPP/probe0/foreign 불변 및 fixture batches all-pass/exact-once도 확인됐다는 부모 감사다.
본 작업자의 추가 다운로드/전체CRC/receipt 재읽기는0이다. 이 수령으로 앞 절의 collision
실물 감사 대기는 해소됐다. closed PID/CWD/FD/DSO·old27·R4·기존 receipt credit0은 유지한다.

main exact logger40217812664의 실제 전체SUCCESS(Miri/ASAN/final/cleanup 포함)는 직접 확인한
metadata와 부모 수령이 일치한다. SDK836/run40218262329 전체SUCCESS도 부모에게 수령했으며,
이 별도 SDK 결과를 기존 #549 각 source의 전후 SDK identity/hash 증거 대신 사용하지 않는다.
main의 두 artifact 실물 감사는 부모가 진행 중이므로 아직 완료로 기록하지 않는다.

**부모의 최종 gate:** 남은 main artifact 감사가 성공하면 위 predelete 근거와 DD retained·f1
collision/main의 명시 성공으로 원본2의 좁은 #549 family 수용 및 inventory credit을 판정할 수
있다. 정확한 old27 pending10 유지 조건은 이미 기록돼 있으며 새 old27 재실행을 요구하지 않는다.
전체 필수 PR/모델·#523/#296 통합/기기 수용과 원장·issue 갱신은 여전히 부모 소유다.

## 부모의 main 실물 감사와 원본2 퇴역 수용 — 2026-10-26

source `f1c730b029df77ed2731f86ddf2d224e74bb9f50`의 [main exact logger40217812664/job121242667579](https://github.com/bin9208/openpilot-rust/actions/runs/40217812664/job/121242667579)가 준비·실제 native caller19·원본 C++/전체 route/media/flush·Miri·ASAN·source/index after·두 업로드와 소유 정리까지 모두 성공했다. 독립 workflow는 main 전체 SHA를 검사하며 main job/global env에 그 read-only pin 하나만 더한 것이다. 필수 PR 전체 성공으로 위장하지 않는다.

부모가 private artifact12608786367(291002B/SHA256 `9324fda49968cd322e004e994f045549a7346af59ffc019dc4f75ed7bb26d62b`,12 safe unique/non-symlink/CRC 정상)의 sourcef1와 source/index 전후 동일성을 확인했다. 기능 artifact12608477043은25035246B/SHA256 `564bdcbcdfd667f4e07e9ac2686cae33cc4306b696db47b81d91333de0f47061`,45249 safe unique/non-symlink/전체CRC 정상이다.

실제 `native-logging/report.json`은 source_policy=absent/result=pass, logprint7/wire168/rate516→16, threads64, strict2s 안의 send+shutdown Rust1.205748035초/C++0.228875968초와 signal/invalid-endpoint/console-failure/runtime-prefix 결과를 보존한다. native probe SHA `02a5d836e30d51dacf31e11f4bbea80ff979c875574164fc75e45562c1d856c8`, fresh C++ SHA `1a13adabeaecbbaca3fc8b726c2e73a13113533d2c495188f273e67c1f174dc2`이며 provider before/after JSON bytes도 같다. runtime-log-upload의 PASS는 owned sourcef1와 crash1/route4/HTTP2044B/upload-marked를 확인하며 완전 manager·기기 수용으로 확대하지 않는다.

위 predelete5bee의 같은 current ELF, DD의 full absent67/14/once/current15·legacy2·CLI20/policy14, f1collision8 및 실제 maintained caller19를 결합해 **#549 원본 두 경로만 native 교체 수용**한다. inventory/family의 두 행을 pending→replaced-local로 갱신한다. 원래 baseline3580은296교체/3부분/3281대기다. 기존 raw receipts의 R4_acceptance/old27/retirement_credit0을 다시 쓰지 않고, 이 수용 결정은 별도 추적 기록으로 남긴다. old27은 역사17/대기10 그대로이며 별도14를 남은10에 대입하지 않는다. 전체 #523/#296·필수 PR/통합·모델 및 실기 수용은 미완료다.
