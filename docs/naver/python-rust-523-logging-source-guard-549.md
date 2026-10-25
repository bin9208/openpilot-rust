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
Git metadata 실제 할당량과 entry별 보수적 block을 계산한다. 기존 target을 제외한 실제 source owner와
독립 copy의 합계는 source2GiB 안이어야 한다. source/job이 다른 filesystem이면 각각 free reserve를 확인하고,
job filesystem에는 계획 copy 성장과512MiB reserve를 요구한다. target8GiB/proof2GiB,
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
