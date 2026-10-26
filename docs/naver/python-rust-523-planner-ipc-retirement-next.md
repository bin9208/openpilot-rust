# #523 Planner IPC 독립 출력 대조와 부분 전환 후속 설계

2026-10-26. 부모 지시에 따라 후속 퇴역 설계는 이 새 문서에서만 관리한다.
기존 planner-ipc-reader-next.md는 부모와 실제 qualification 감사 통합용이며 이번 작업에서 수정하지 않았다.
앞서 그 문서에 기록했던 계획을 원문 그대로 복사했다. 기존 문서의 앞선 append 정리는 부모 담당이다.
source47fb의 네 C++ run qualification은 API SUCCESS이며 실제121×4/native160/cleanup 판정은 Bacon 감사 대기다.
최신 부모 지시: policy/peers_ready/OUTPUTS는 원래 plannerd_ipc.py에 그대로 남긴다.
그 정의를 다른 Python 파일로 옮겨 전체 퇴역 credit을 주는 계획은 철회한다.
plannerd_ipc.py는 부분 전환/credit0이고, scheduler test14 파일의 완전 삭제 가능성은 별도로 판정한다.
이 문서의 아래 초기 caller안은 구현 승인이 아니며, 새 outputs reader 구현 상태는 마지막 절을 따른다.

## source47fb qualification 뒤 두 Python 경로 퇴역 준비 — 읽기 전용 계획

2026-10-26. 부모는 source47fb의 HOST run40246606213/job121329384086과
ARM run40246606219/job121329384161의 실제 API SUCCESS를 확인했다.
이 긍정 상태는 재사용한다. **새 C++ peer의 actual121/run×4, native160, source/SDK/cleanup과
artifact 최종 수용은 Bacon raw 감사 확정 대기**이며, 이번 담당은 그 실행을 중복 감사·재실행하지 않는다.
아래는 그 감사 뒤 적용할 최소 후속안이다. 현재 코드/WF/삭제/GHA 변경은0이다.

### 실제 소비자와 남길 역사 참조

저장소에서 `rg -l --hidden`으로 plannerd_ipc/test_plannerd_native_scheduler/ModelScheduler의
직접 경로·native scheduler env를 추적했다. .git/.analysis는 제외했고, generic unittest/pytest
검색도 별도로 했다. main rust.yml은 rg-l로 관련 참조 존재만 확인하고 전체 본문을 출력하지 않았다.

| 소비자 | 현재 역할 | 퇴역 전에 닫을 경계 |
| --- | --- | --- |
| planner_ci_controller/plan.rs | runtime 12leaf 중 ipc가 Python plannerd_ipc.py 실행, native scheduler env와160 요구 | 순서10/이름ipc/12개/첫 실패 전파를 유지하며 기존 native IPC 실행 경로로 교체 |
| rust/tools/plannerd_lifecycle_peer.py | plannerd_ipc의 OUTPUTS/peers_ready/policy 직접 import | 세 정의와 필요한 import를 원래 plannerd_ipc.py에 원문 그대로 유지. lifecycle 의존은 미전환이며 이 파일 credit0 |
| rust/tools/check_plannerd_lifecycle.py | 위 peer의 Case/run을 import하는 실제 lifecycle leaf | 기존 Python lifecycle oracle·원본 daemon·기한·출력은 남김. IPC 파일 삭제로 import가 깨지지 않아야 함 |
| rust/tools/tests/test_plannerd_native_scheduler.py | 원래 adapter13의 AST/실제 native160 + collector1, 현재 총14 method | 원래13 의미를 native15와 실제 IPC로 폐쇄한 뒤 Python test 실행 연결과 파일을 함께 제거 |
| tests/planner_scheduler_client.rs | 원래13 의미 + parse35 관련2개 = native15. 실제160은 독립 finite76 관측에서40×4 선택 | include_str의 삭제 예정 .py 직접 의존부터 동결한 원문 text fixture로 이동 |
| tests/planner_ci_controller.rs | 12leaf 순서/argv/scheduler env160과 첫 실패 전달 검사 | 변경된 ipc 한 슬롯의 정확 argv/output/evidence 계약만 갱신. 나머지11개/메모리3/실패코드 유지 |
| tests/planner_controller_frozen.rs | source73의 원래5개 관측과 runtime-plan 전체 argv 비교 | 원래 JSON/SOURCE_PINS는 바꾸지 않음. ipc 한 슬롯의 명시된 전환 계약만 별도 정확 검사하고 나머지11개 원문 비교 유지 |
| native-units.yml | old adapter14의 unittest 실제 실행과 PLANNER_NATIVE_SCHEDULER_TEST_BIN | native15/parse35/real160 유지 후 old unittest 호출과 이 실행 전용 env 제거 |
| planner-caller.yml/native-arm-more.yml/main rust.yml | 실제 runtime helper, source/binding 준비와 IPC qualification/stager/native 실행 | 아래 선행 stager 순서와 단일 실행 IPC leaf로 부모가 동기화 |
| python-rust-523.yml/ci_rust_isolation.rs | source roster·hash pin·workflow/capture 정적 계약 | 정식 caller 전환·부재 계약을 정확히 반영. 원문 gate 삭제/완화로 통과시키지 않음 |
| corpus.rs/observed_tests.rs와 외부 audit의 source9 | 역사 aa79 producer의 plannerd_ipc.py SHA 참조 | 삭제 뒤에도 그대로 보존. verify_schema는 현재 schema4만 읽으므로 이 역사 path를 현 파일 존재 요구로 바꾸지 않음 |
| memory test private fixture·manager 이전 before/after SHA·원본 PROVENANCE/문서/원장 | 실행하지 않는 과거 경로와 negative fixture | 실제 import/exec와 구분. 역사 bytes를 삭제하거나 최신 source로 덮어쓰지 않음 |

다른 workflow의 rust/tools/tests 검색 결과는 UI/Xiaoge/Params/Carrot/radard의 명시 test pattern이다.
이번 adapter를 실제 실행하는 명시 호출은 native-units의 해당 unittest pattern이다.
문자열 rg 전체0을 퇴역 조건으로 삼지 않는다. 역사 source9/PROVENANCE/부정 fixture는 남아야 한다.

### aa79에 이미 있는 독립 입력과 원래 출력

기존 raw 입력은 HOST private12615278417/ARM private12615287431이다.
각각 mode×epoch별 manifest4, initial Params184/run, 실제 CarParams pending bytes4,
raw event401/run×4와 root index를 그대로 사용한다. 어느 mode/epoch의 header·snapshot도
다른 case에 재사용하지 않고 native로 message/Params를 생성하지 않는다.

cached functional12615802090(HOST)과12614614983(ARM)의 central directory와 case receipt를 읽었다.
두 archive 모두 원래 Python controller가 관측한 source-0/source-1/rust-0/rust-1에 대해
**case receipt4개와 raw publication121×4개가 실제로 존재**한다.
각 raw path는 원래 case receipt의 절대 path를 선행 slash만 제거한 ZIP member로 정확히 연결된다.
HOST의 case receipt 합계1809386B + raw484개270048B =2079434B,
ARM은 receipt1809514B + raw270048B =2079562B다. 전체 ZIP 재다운로드·전체 CRC 재검사는 하지 않았다.
source 두 case는 원래 Python daemon 출력이고 rust 두 case는 당시 Rust daemon 출력이다.
네 case를 모두 Python daemon golden이라고 부르지 않는다.

현재 stager는 functional ZIP에서 최상위 summary receipt 하나만 추출한다.
source47fb reader도 그 summary와 네 identities/coverage만 인증했다. **아래 새 outputs.rs는121×4 raw 경로를 추가하지만 첫 실제 CI는 대기다**.
따라서 새 native 네 run끼리 동일하다는 사실만으로 과거 원문 출력과의 독립 동등성을 대신하지 않는다.

후속 최소 자료는 기존 input audit v1을 덮어쓰지 않는 별도 고정 outputs-audit JSON이다.
HOST/ARM별 origin source/run/artifact ID·ZIP bytes/SHA, 네 mode/epoch의 기존 case receipt
{member,bytes,sha256}와121 raw의 {member,bytes,sha256}만 담는다.
metadata는 기존 ZIP 원문에서만 채취하고 예상 data/value는 기존 Python case receipt 그대로 읽는다.
후보 출력·decoder·scheduler 계산으로 expected를 생성하지 않는다. raw corpus/ZIP의 Git 삽입도0이다.

기존 stager를 명시 allowlist 추출로 확장해 summary + case receipt4 + raw484만 안전 추출한다.
기존128MiB staging admission 안에서 추가량은 약2.1MB/host이며, 실제 filesystem 증가량을 다시
확인한다. 부모 승인 없이 cap/기한을 늘리지 않는다. 각 receipt1MiB, raw1MiB, case121/전체484,
합계8MiB 안의 별도 output proof와 exact roster/path/SHA/NOFOLLOW·전후 identity를 요구하는 안이다.

검증은 두 단계를 분리한다. 먼저121×4 원래 raw 각각을 native decoder로 읽고 원래 Python
receipt의 service/valid/data 및 timestamp와 대조해 full-schema projection 자체를 검증한다.
그 뒤 새 실제 run을 같은 host/mode/epoch의 원래 기록과 현재 decode::differences로 비교한다.
현재 comparator의 service/valid/data와 float bits, 이미 제외한 timing4필드만 그대로 사용한다.
새 field mask·tolerance·native expected·다른 case 대체는 허용하지 않는다.
raw header의 실제 bytes/SHA는 역사 자료 인증에 쓰며, 새 wall-clock publication header의 SHA가
과거 packet 전체 SHA와 같다고 요구하거나 같다고 주장하지 않는다. 불일치는 삭제 보류 근거다.

### 보존14일 →90일: 이미 업로드한 cohort와 다음 업로드를 구분

2026-10-26 run artifact API를 읽은 실제 만료 UTC는 다음과 같다.

| aa79 artifact | 현재 만료 | 조치 필요 |
| --- | --- | --- |
| HOST private12615278417 | 2026-11-08T21:54:46Z | private rawcapture의90일 보존 경로 필요 |
| ARM private12615287431 | 2026-11-08T21:57:37Z | private rawcapture의90일 보존 경로 필요 |
| HOST functional12615802090 | 2027-01-23T21:49:10Z | 이미90일. 후속 workflow에서는 의존하지 않도록90일을 명시 |
| ARM functional12614614983 | 2027-01-23T21:49:10Z | 이미90일. 후속 workflow에서는 의존하지 않도록90일을 명시 |

현재 private proof upload는 retention-days14이며 functional upload는 repository default다.
원본 rawcapture를 새 original 실행으로 다시 만들 필요는 없다.
[GitHub 공식 보존 정책](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository)은
설정 변경이 기존 artifact에 소급되지 않는다고 명시한다. 다음 upload를90일로 바꾸는 것만으로
위 기존 private 두 개의11월8일 만료가 연장되지는 않는다.

기존 helper ipc는 proof/ipc-historical에 audit.json/capture.zip/receipt.zip 원문 사본을 이미 보존한다.
부모가 허용하는 다음 자료 보존 upload에서 이 exact ZIP bytes를90일 carrier로 보존할 수 있다.
기존 qualification 성공을 되풀이하거나 원본/모델을 재실행할 이유는 없다.
새 carrier는 새로운 artifact ID/outer ZIP SHA를 갖는다. aa79 origin run/source/ID와 inner ZIP
SHA를 그대로 남기고 carrier provenance를 별도 고정해야 한다. origin을 새 source로 가장하지 않는다.
현재 stager는 원래 artifact API ID/source/size/SHA를 직접 요구하므로, carrier를 소비하려면
부모가 검토한 별도 transport 인증이 먼저 필요하다. 만료된 원래 API를 무조건 건너뛰는 fallback은 금지다.
새 transport가 없다면 원본을 못 읽을 때 현재처럼 fail-closed한다.

### 단일 actual IPC leaf로 바꾸는 최소 caller 제안

현재 순서는 runtime(안에 old Python IPC) → HOST memory → ipc-check → stager → native ipc다.
퇴역 뒤에는 stager가 runtime의 새 IPC leaf보다 먼저 와야 한다. stager는 controller ELF 없이
owned job/proof/source와 token만으로 동작하므로 기존 source/SDK/solver/Params/msgq 준비 뒤에
먼저 실행할 수 있다. token은 stager step에서만 쓰고 이후 compiler/daemon에 전달하지 않는다.

이전의 runtime plan → helper ipc → setsid native IPC 중첩안은 **채택 보류**다.
바깥2700/5400초 안쪽600/1200초라는 정상 기한 순서만으로 process ownership은 증명되지 않는다.
바깥 NativeRunner가 소유한 PGID에 KILL을 보내도 helper가 새 setsid로 만든 PGID에는 전달되지 않는다.
helper 자체가 SIGKILL되면 EXIT trap도 실행되지 않으므로 compiler/peer/daemon이 남을 수 있다.
따라서 기존 source47fb의 직접 helper qualification 성공을 그 중첩 cutover의 KILL 증거로 재사용하지 않는다.

부모가 plan/helper를 전환할 때는 native CLI를 한 owned QA group 안에서 직접 실행해 자식들이
그 그룹을 상속하거나, 이미 있는 current-group launch 경로를 명시적으로 재사용하는 최소안을 검토한다.
새 detached group을 만든다면 독립 owner가 그 PGID와 leader identity를 끝까지 보유하고
실제 outer timeout/SIGKILL에서도 kill/directwait/live-scan을 수행한다는 별도 근거가 필요하다.
이 담당은 helper/plan/공유 QA 코드를 수정하지 않는다. 모든 자손 reap 완료도 주장하지 않는다.

다음 output/evidence 전달 요구는 caller 구현의 조건이며, 앞선 중첩 helper안을 승인한 뜻이 아니다.
Check.invocation이 붙이는 output은 정확 기존 planner-comparison/ipc이고, inherited
PLANNER_NATIVE_SCHEDULER_EVIDENCE는 기존 ipc-scheduler.tsv여야 한다. 현재 hardcoded
ipc-native output/별도 evidence는 이 plan 경로에서 두 번째 자료로 다시 만들지 않는다.
실제 client/server가 쓴160행을 그대로 바깥 run_checks의 scheduler_calls=Some(160)가 검증한다.
wrapper는 실제 owned child의 첫 nonzero를 cleanup 뒤에도 보존하고, 후검사 실패는 기존 실패를
성공으로 바꾸지 않아야 한다. 새 임의 output·caller·SDK·profile이나 Python fallback은 허용하지 않는다.

plan의 ipc 한 슬롯은 ownership을 닫은 native 경로를 만들고 나머지11개 argv/order는
그대로 둔다. 기본 runtime build에는 검증된 planner-ipc-native profile이 필수이며 feature/profile
부재는 명시 거부한다. 메모리3/ASan receipt·다른 Python reference path는 변경하지 않는다.
새 Options의 이미 qualified IPC600/1200초·peer build180초·512/128MiB 및 각 wire2초/close3초는
확대하지 않고, 바깥12leaf의 기존 timeout/operation admission도 보존해 각 범위를 별도 기록한다.
이것은 제안이며 이번에 helper/plan/CLI/WF 소스는 수정하지 않았다.

후속 순서는 stager → runtime12(그 안 native IPC1회) → HOST ASan3 → ipc-check4/Clippy →
source/SDK/원본/caller 후검증·업로드/cleanup이다. 현재 runtime 뒤 별도 native ipc step은 제거해
같은4×40을 중복 실행하지 않는다. 최초qualification인 source47fb는 그대로 증거로 재사용한다.

### 컴파일 의존과 original13의 semantic closure

현재 native15 test의 include_str!(../../../tools/plannerd_ipc.py)는 삭제 즉시 컴파일을 깨뜨린다.
원래 next_model_target 함수의233B를 원문 그대로 작은 비실행 source text fixture로 먼저 동결한다.
현재/원래 함수 SHA256은
`a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1`다.
새 text를 include_str로 읽고 같은 SHA를 검사하며, 함수 내용/공백/2칸 return/trailing newline을
고치거나 Rust 수학에서 원문을 다시 생성하지 않는다. origin module/source/span을 PROVENANCE에 남긴다.
이 source text는 packet/Params rawcapture가 아니며 실행·AST eval하지 않는다.

기존13의 oracle bytes/실제 run loop1session1request/absent signedzero·NaN/persistent40/empty/spawn/
EOF·42·malformed·nonfinite·long·multiple/2초 timeout/latch/close42/normal TERM-KILL-wait/
예외 원오류 유지/실제4session160 의미를 native15에서 각각 보존한다.
finite fixture76개 중 경계4+기존36을 각40회×4로 쓰는 현재 독립160과 parser35의 frozen bytes/SHA는
바꾸지 않는다. parser35는 float+finite AST 단계만의 관측이며 full Python client 원본 전체
재현 증거로 과장하지 않는다. 실제 wire·native lifecycle 폐쇄는 Bacon의 새 C++ qualification과 함께 본다.
collector 추가1개는 삭제될 생산 수집기 회귀다. 원본자료는 이미 독립 수집됐으므로 새 native
message generator/expected writer를 만들어 collector를 흉내 내지 않고 reader4의 인증 경계로 남긴다.

source73 runtime-plan frozen JSON은 그대로 둔다. 나머지11개는 기존 완전 argv 비교를 유지하고,
ipc 한 곳만 old literal을 명시한 정확 native wrapper/--output/evidence 계약으로 전환한다.
새 native argv를 자동으로 expected로 저장하거나 unknown 경로 wildcard로 원문 비교를 완화하지 않는다.
역사 source9·scheduler-finite/IEEE·parser35/원본 run receipt는 최신 source hash로 덮어쓰지 않는다.

### 삭제와 최종 수량 인정 게이트

Bacon이 source47fb의 HOST/ARM actual121×4/native160/정리·봉인을 확정하기 전에는 삭제하지 않는다.
그 뒤에도 독립 old-output121×4 reader의 실제 대조, source text include_str, ownership을 닫은
plan/helper/WF caller 전환이 완료된 뒤에만 scheduler test14 파일 삭제를 준비한다.
plannerd_ipc.py에는 세 원래 정의가 그대로 남으므로 경로 부재 guard를 걸거나 전체 삭제 credit을 주지 않는다.
미사용 collector/ModelScheduler/IPC run 경로 제거는 부분 전환으로만 기록한다.
scheduler test 파일의 부재/재등장과 폐쇄한 실행 caller를 검사하되 기존 lifecycle import는 계속 허용한다.
추후 lifecycle peer/oracle 기능까지 같은 closure로 Rust 전환한 뒤 남은 세 정의와 원본 파일을 완전히 제거한다.
부모의 영향 범위 Actions에서 변경된 실제 caller/부재·원래 출력 비교를 확인한 뒤에만 원장 credit을 준다.
이미 성공한 불변 검사나 original capture를 처음부터 다시 실행하지 않는다.

이번 작업은 rg·소스·기존 ZIP member/receipt·보존 metadata 조회와 이 문서 append뿐이다.
rawcapture Git 삽입/새 expected 생성/코드·WF·삭제·원장·이슈 변경/로컬 기능·Cargo·원본·모델 실행/
GHA dispatch·재실행·취소/stage/commit/push는 전부0이다.


## compiled-in original outputs reader 구현 — source-only, 첫 실제 CI 대기

부모가 허용한 outputs.rs/mod.rs/기존 observed_tests4/metadata2/작은 source text/이 문서만 수정했다.
stager/plan/helper/CLI Options/공유 Cargo/WF/기존 Python 두 파일/native client test include는 바꾸지 않았다.
source47fb API SUCCESS와 Bacon의 실제 qualification 감사는 이 새 old-output 비교 성공 증거가 아니다.

### 새 metadata와 신뢰 시작점

새 `tests/fixtures/planner-ipc-original-outputs/{x86_64,aarch64}.json`은 각각113918B다.

| host | metadata SHA256 | 연결한 기존 input audit SHA256 | 원본 functional artifact |
| --- | --- | --- | --- |
| x86_64 | `525333b68bda155b622a2e315880bcf378b7425833760ea40a259545f7703b21` | `384bcef1bafc962019557d7b89ef5dac4c85ee2d1683906c1942458b04118e54` |12615802090 |
| aarch64 | `a28890f6f79eb24f0018d62ec21dae5d8c2228d6b7da0f62ce33ffab48e80c80` | `7dd3abad75e472a7241d16143b155dd3ad6e8bb826359bd0afedca1360689efc` |12614614983 |

JSON의 필드는 schema_version/kind/input_audit_sha256/receipt_artifact_id/runs뿐이다.
runs는 원래 source-0/source-1/rust-0/rust-1 순서이며 각 mode/epoch, 기존 case receipt descriptor,
기존 receipt.records 순서의 raw descriptor121개를 가진다. descriptor는 ZIP member/path/bytes/SHA만
담는다. 기존 cached case receipt와 raw만 읽어 metadata를 작성했으며 예상 data·float·message·Params는
생성하지 않았다. 실제 data expected의 단독 출처는 pinned 원래 case receipt다.

reader는 compiled-in include_bytes로 해당 host JSON을 고르고 기존 Capture.audit_sha256/receipt role
ID·summary path에 결합한다. 기존 input audit의 source9/origin source/run/host/ZIP digest/identities는
그대로 신뢰 체인의 일부이며 새 metadata에 가상의 origin을 만들지 않는다. 따라서17개 Options와
CLI 필수 인자는 그대로다. runtime에서 임의 output audit를 선택하는 인자나 fallback도 없다.

### 읽기·대조·실패 처리

원본 raw121×4의 완전 decode는 어떤 compiler/daemon/peer 실행과 output directory 생성보다 먼저 한다.
case receipt의 command가 같은 mode/epoch input manifest의 실제 daemon argv와 같은지,
returncode=-15/records121/timing40/summary coverage/records timestamp 정렬을 대조한다.
각 record.path와 raw member를 정확히 연결하고 raw SHA/길이를 검증한 뒤 기존 decoder를 호출한다.
기존 Python policy record의 service/valid/data 및 정확 timestamp와 일치해야 원본 dataset을 수용한다.

새 current run 네 개가 끝나면 각 mode/epoch를 대응 원본121개와 기존 decode::differences로 비교한다.
기존의 timing4필드 제외·float bits 비교를 그대로 쓰며 새 허용오차/field mask는 없다.
이 결과는 output/original-output-comparison.json sidecar에 쓴다. 원래 receipt.json의 schema는 그대로다.
sidecar의 passed는 이 비교만 뜻하며 full_runtime_acceptance=false/retirement_credit0을 기록한다.
원래 current 네 run끼리 비교·coverage 실패도 유지하고, independent old-output 불일치 역시 실패한다.

공통 summary를 포함한489개 원본 file의 첫 identity를 유지하고 전후에 canonical/regular/
NOFOLLOW/NONBLOCK/CLOEXEC/정확 길이·SHA·FD/path metadata를 확인한다.
원본을 삭제 후 같은 bytes의 새 파일·symlink로 복원해도 실패한다. 성공/실패의 work 결과 반환 전에
입력 corpus와 old-output 파일을 다시 확인한다. raw/receipt 각1MiB, 합계8MiB 한도를 유지하며
compiled metadata와 기존 summary까지 포함한 실제 proof 크기는 HOST2195385B/ARM2195515B다.

### 부모 stager 연결과 필요한 다음 actual CI

부모가 기존 functional extraction root `$CI_JOB_TEMP/planner-ipc-original/receipt/`에 다음을
allowlist대로 추출해야 한다. summary는 이미 추출하며 추가분만 약2.1MB다.

- `<tmp/openpilot-ci.XXXXXX>/planner-comparison/ipc/{source-0,source-1,rust-0,rust-1}/receipt.json`4개
- metadata runs[].publications[].path의 정확 raw484개

위 tmp path에는 metadata의 실제0oi4Ta(HOST)/hHf287(ARM)를 사용하며 wildcard로 다른 cohort를
선택하지 않는다. 기존 root/audit.json/capture.zip/receipt.zip 및 input raw401×4는 그대로 필요하다.
새 metadata JSON을 runtime 선택용으로 복사할 필요는 없고, stager는 동일 source의 고정2파일을
allowlist로 사용하면 된다. 추출이 없으면 새 reader가 실행 전에 fail-closed한다.
17개 native IPC CLI 인자와 기존 jobs1/자원·기한·SDK·ownedgroup 정책은 그대로다.

GHA의 기존 lib selector `planner_ci_controller::planner_ipc_native::observed_tests::`는 여전히4개다.
그4개 안에 원래 source425 packet3개의 새 verify_packet 양성/record 변조 음성,
HOST/ARM metadata와 input audit의 양성 결합 및 wrong hash/artifact/cohort 거부,
missing case/packet·alias·unknown field·1MiB/8MiB cap·경로 이탈·파일 변조/재등장 음성을 추가했다.
새 testcase 수는0이고 이4개가 실제 aa79 전체484 decode 성공을 대신하지 않는다.

부모 후속 CI는 같은 feature의 기존 lib4/strict Clippy와 실제 HOST/ARM `check-plannerd-ci ipc`에서
원본484 decode 후 current121×4 대조, native160, sidecar passed=true, 전후 immutable/cleanup을
확인해야 한다. 기존4 CPP qualification을 처음부터 반복하는 목적이 아니라 새 independent old-output
경계를 추가한 영향 검증이며, 이 검증 후에 caller cutover를 진행한다. GHA 실행은 이 담당0이다.

### 부분 credit과233B 함수 source fixture

새 `tests/fixtures/planner-scheduler-original/next_model_target.source.txt`는 현재 독립 함수의
원문233B/SHA256 `a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1` 그대로다.
origin은 aa79 plannerd_ipc.py SHA `a4aec1991a6751af60f5616698e75e0861b0a1f8b1c478e5382889681598c8dc`의
next_model_target 함수다. 시작 def부터2칸 return target와 trailing newline까지이며 native 수학에서
생성하지 않았다. 현재 test include 경로와 Python 함수는 바꾸지 않았고 caller 배치 때 전환한다.

plannerd_ipc.py의 policy/peers_ready/OUTPUTS와 lifecycle oracle는 이번에도 그대로다.
나중에 collector/ModelScheduler/실행 loop를 제거해도 해당 파일은 partially-replaced/전체credit0이다.
scheduler Python test14는 native15/독립 finite76 기반160/parse35/새 actual old-output 대조가
수용되고 실제 unittest caller가 제거되면 별도 파일 삭제를 검토할 수 있다. 현재 삭제와 credit은0이다.

로컬은 Rustfmt2021·diff·소스 검토와 원래 ZIP/JSON/path/size/SHA/command/summary 연결의 inert 확인만 했다.
전체 CRC 재실행/원본·모델·프로젝트 기능·Cargo/build/test/Clippy 실행은0이다.
raw Git 삽입/후보 expected 생성/원장·이슈·GHA/stage/commit/push도0이다.
