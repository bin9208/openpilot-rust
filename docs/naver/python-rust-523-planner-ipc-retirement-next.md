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


## 0283 HOST/ARM old-output actual 수용 감사 — 2026-10-26

source `0283e6345fc8c8e60cd3537424f1c2df383d9088`의 다음 두 실행은 attempt1/push이며
run과 Planner job 모두 실제 SUCCESS다.

- HOST [40250301867/job121340512349](https://github.com/bin9208/openpilot-rust/actions/runs/40250301867/job/121340512349), 완료00:46:25Z
- ARM [40250301909/job121340512538](https://github.com/bin9208/openpilot-rust/actions/runs/40250301909/job/121340512538), run 완료00:48:04Z

위 시간은 API updated_at이며 native 연산 시간 측정치가 아니다. ad1e hotfix와 공유 worktree의
진행 중 helper 변경은 이 source에 섞지 않았다. 감사에 사용한 source는 Git의 정확0283 blob과
실제0283 API/artifact다. 기존47 qualification을 다시 감사하거나 실행하지 않았다.
앞부분의 summary-only stager/first CI 대기는 해당 구현 전 역사 상태다. 아래가 새 경계의 실제 결과다.

### 새 archive와 원문 로그

다운로드 전 실제 가용74577764352B를 확인했고 새 archive 합계193992886B를 admission했다.
각 artifact의 API source/bytes/digest와 직접 ZIP SHA를 확인한 뒤 safe unique canonical member,
symlink/특수파일/암호화 부재와 전체 CRC를 각각 한 번 확인했다. 전체를 파일시스템에 추출하지 않았다.

| host / artifact | bytes | safe members / expanded bytes | SHA256 |
| --- | ---: | --- | --- |
| HOST private12621300114 |49780298 |2447 /70412611 |`ac8885a715a38ce3eef525a55677276afb7ae8e8d5d9d057681bb96c82f7c54e` |
| HOST functional12620204855 |57098198 |37137 /174302791 |`99a0b7b57457a734e59394b070cf0bfa2919ce94e8149983238ed2cce041a443` |
| ARM private12620690744 |40359546 |2436 /58706531 |`c8373789c3445bdaca7bae1402a42d70019e971f1cc30e49deb49afe61439170` |
| ARM functional12620765642 |46754844 |32534 /145266289 |`55198e49a2a10bba82fb412c2eac5f6361fb4473a1a440d69392372cad7ad89d` |

decoded UTF-8 raw job log는 HOST360106B/SHA256
`dd9c69a6f71f4d5ef5745b1fe99e3595b0aab00e82d6cff6972e0d13ada5c952`,
ARM339640B/SHA256 `40aae9e87648036251ce65f236252c721156e453645486371410d89e7c7324f4`다.
private scratch/ci의 outputs0283-{host-job121340512349,arm-job121340512538}.log와
outputs0283-independent-values.json/outputs0283-controls-audit.json에 근거를 남겼다.

### 실제 old484 decode와 independent121×4 일치

양쪽 lib stdout는 정확4 passed/0 failed/0 ignored이며 strict Clippy status0이다.
stager status는 receipt_members489/output_case_receipts4/output_publications484다.
489 allowlist는 summary1 + case receipt4 + raw484의 정확한 unique 집합이며,
output-members.tsv488행의 path/bytes/SHA 전체가 compiled-in historical metadata와 같다.
metadata bytes와 source/host/input audit/receipt artifact ID도 원래 pin과 같다.

128MiB staging 범위의 실제 allocated/apparent는 HOST80318464/69563323B,
ARM68722688/57964858B다. ipc-historical에 보존한 capture.zip/receipt.zip 원문도 각각
원래 aa79 archive SHA와 같다. archive를 새 expected로 대체하지 않았다.

OriginalOutputs::open은 정확0283 source에서 원본484개 raw를 실제 decoder로 읽고 원래
Python policy record와 timestamp/service/valid/data가 같아야 current process로 진행한다.
양쪽 실제 peer provenance와 sidecar는 original_packets_decoded484, 올바른 output/input audit SHA와
각 original functional ID를 기록했다. 이 decode는 CI의 실제 Rust 실행 증거이며 로컬에서
native decoder/프로젝트/원본을 다시 실행하지 않았다.

sidecar passed=true만 믿지 않고 기존 aa79 case receipt의121×4와 새 functional case receipt를
로컬에서 데이터로 직접 대조했다. mode/epoch를 각각 source0/source1/rust0/rust1에 고정하고
service/valid/data의 모든 key·list 순서·길이·값과 signed-zero를 보존하는 float64 bits를 비교했다.
HOST/ARM 각각 float64 bits46544개/정수1636개, **차이0**이다.
현재 각 run은121 records/40 timing/returncode=-15이며 valid counts는
longitudinal40/lateral39/driverAssistance39, triggers modelV2=6/liveTracks=35, fast_overlays34다.
현재 네 run 사이의 기존 비교3개도 differences=[]다. 새 current raw도484개/270048B다.

| host | original-output-comparison.json SHA256 | 실제 native C++ peer ELF SHA256 |
| --- | --- | --- |
| HOST |`148a3df84ddf89c5c7e1a75a963b5de9a51339d2207ab54daa8f96b1d2c28daa` |`7bac79452675a9e4efee46597a308fe2f1dead4ea299c60007480af40e9ba5f6` |
| ARM |`389bfa26f9e09972b043a15c502d6855a800d0e38aad26642515b01a42094b90` |`5cb05a3580ef3a077169a6f669c4b1cb07759962c2455652c6e318b99843ce2c` |

peer retained bytes도 provenance의 hash와 같고 ELF64 native machine은 각각 x86_64=62/ARM64=183이다.
이 일치는 기존 projection 정책의 data 비교이며 새 packet 전체 wall-clock header SHA까지 원래와
같았다는 의미는 아니다. timing4필드 제외·float bits·expected를 변경하지 않았다.

### 동일 source/SDK/controller, endpoint160과 직접 group 종료

source-before/after 및 tracked index before/after가 동일하다. 실제 SDK file/roster는
HOST159/181개, ARM158/180개다. HOST runtime/memory/ipc-check/ipc, ARM runtime/ipc-check/ipc의
전후 SDK 전체 hash와 stable roster가 각각 before와 같다. ARM ASan step은 기존대로 skipped다.
HOST ASan step은 success이며 양쪽 기존 runtime12 result도 PASS다.

runtime build controller의 feature는 ci-receipt-metadata-native,planner-ipc-native이며
source/host와 각 mode 후 SHA check, IPC held metadata12개가 일치한다.

- HOST controller SHA256 `c910abbd59ebd8656f0b23d2415b000ca3839fb3c8893c3e756a54c6ae21258d`
- ARM controller SHA256 `594a67d60fbdd67b9cd4c9dc0961ce905e3273bea0016a58083ea78826ef8f9f`

controller payload 자체는 archive에 없으므로 로컬에서 그 ELF를 직접 재hash했다고 주장하지 않는다.
실제 commands.nul의 실행물과 --scheduler가 같은 controller path이며 held before/after의
path→identity/SHA map이 정확히 같다. associative map 출력의 행 순서는 달라 집합으로 대조했다.
actual current_exe endpoint 계약과 같은160행 evidence를 source에 결합했다.
모든160행 target은 각 current timing.target의 bits와 같고, 각 run의1~39행은 앞 timing의
sent/model_done과 bits가 같다(각156쌍). 로컬에서 next_model_target 정책 계산을 재실행하지 않았다.

현재 직접 owned 세션의 leader는 HOST16843/ARM16007이며 T state, PID=PGID=SID와 ready identity를
확인했다. 실제 status0, direct wait0, leader 제외 전 scan과 wait 뒤 scan이 모두 같은 PGID를
가리킨다. 최종 original_exit/cleanup_exit/cleanup_wait는 모두0,
owned_groups_nonlive=true/all_adopted_children_reaped=false다. 두 source alias/temporary cleanup도
job API에서 success다. 따라서 **현재 직접 group scope**를 수용하며 모든 입양 자손 reap이나
향후 plan→helper 중첩의 outer SIGKILL 회수까지 증명했다고 쓰지 않는다.

이 actual은 새 old-output 대조 경계를 완료한다. sidecar의 full_runtime_acceptance=false와
retirement_credit0, plannerd_ipc.py 부분 전환 경계 및 lifecycle oracle 미전환은 그대로다.
Python 두 파일은 이 source에서 삭제하지 않았다. 이번 감사의 tracked 변경은 이 문서 append뿐이며
CI 조작·source/expected/임계 변경·로컬 기능·Cargo/원본/모델 실행·stage/commit/push는0이다.

## old-output 수용 뒤 부분 source 전환 후보 — 2026-10-26

위0283 HOST/ARM actual 감사 완료 뒤 아래 소스 후보를 작성했다. **이 후보의 삭제 후 GHA는 아직
실행하지 않았고 퇴역 실적 증가는0**이다. 기존 실제 성공은 새 old-output 경계의 근거이며
이번 부분 전환·부재 guard·caller cutover의 실제 수용까지 대신하지 않는다.

이 담당의 변경은 다음 source6파일과 이 문서 append다. 부모와 다른 담당의 공유 worktree WIP를
이 목록에 포함하거나 변경하지 않았다.

- `rust/tools/plannerd_ipc.py`: lifecycle이 import하는 `OUTPUTS`, `policy`, `peers_ready`의 원문과
  필요한 `time`/`log` import만 유지했다. 사용하지 않는 collector/client/ModelScheduler/run/main을
  제거했고 직접 CLI 호출에는 native 명령을 안내하며 실패하는 guard를 남겼다.
- `rust/tools/tests/test_plannerd_native_scheduler.py`: 원래13개 adapter 회귀와 추가 collector1개로
  구성된14개 test 파일을 삭제 후보로 만들었다. 다른 Python 파일로 옮긴 구현은0이다.
- `.github/workflows/python-rust-523-native-units.yml`: old14 unittest 호출·그 실행에만 쓰는 env와
  결과 assertion의 정확8행만 제거했다. 나머지 workflow bytes와 native15/controller14/
  retirement6/frozen5/memory15 및 기존 Clippy 호출은 그대로다.
- `rust/crates/dev-tools/tests/planner_scheduler_client.rs`: 첫 test의 include를 이미 발행한
  `fixtures/planner-scheduler-original/next_model_target.source.txt`로 바꾸고233B·고정 SHA·함수 전체
  일치를 확인한다. 같은 test에서 삭제 경로의 실제 부재도 확인한다. 나머지14개 test 본문은
  bytes 불변이며 원래13 의미의 framing/latch/lifecycle·2초/3초·native160/parse35 검사를 유지한다.
- `rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/mod.rs`: native IPC 진입 전과
  기존 실행의 성공/실패 반환 전에 정확한 삭제 test 경로의 부재를 검사한다. 원래 실행 본문은
  private 함수로 이름만 분리했고 bytes 불변이다. 별도 CLI Options나 공통 framework는 추가하지 않았다.
- `rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/observed_tests.rs`: 기존 네 번째
  test 내부에 private fixture root의 file/symlink/dangling symlink/directory 재등장 거부를 추가했다.
  absent일 때 원래 성공값/실패값(`original work exit42`)을 그대로 전달하며, 재등장하면 work의
  성공/실패 어느 쪽도 반환하기 전에 거부한다. 새 testcase/selector 증가는0이다.

### 부분 module과 독립 원문의 고정

`plannerd_lifecycle_peer.py`는 계속 세 정의를 원래 module에서 import한다. 해당 lifecycle 파일에
정의나 기능을 relocation하지 않았다. 남긴 `plannerd_ipc.py`는 **partially-replaced/전체 credit0**이며,
향후 같은 closure의 Rust 전환 전까지 파일 전체를 퇴역했다고 세지 않는다.

| 보존 source span | bytes | SHA256 |
| --- | ---: | --- |
| `OUTPUTS` |66 |`f046e6368939c6c11565dadb98ba307ad220ec92bd6a59d4d454b07c64e630c4` |
| `policy` |327 |`d009bd8321c8361224e69cbb196714bc15aa26b7834048c15741c007af23639f` |
| `peers_ready` |434 |`2c10be84083db5b0f5e217577bd786c9dcd8cc8d5aa7ed7d12ba77a4cd4b77b6` |

기존 module은30096B/SHA256
`a4aec1991a6751af60f5616698e75e0861b0a1f8b1c478e5382889681598c8dc`,
부분 module은1108B/SHA256 `f800915a946ff2a39b50eb576bdd4edc4631c6740f09e6e65b61cec3c5d684c5`다.
삭제 후보 test는22265B/SHA256
`bc9e2beb046308aab5c017950e90fabc627bf11a175bdda0799341be8349f32c`였다.

233B fixture는 위 원래 module의26..30행 `next_model_target` 원문과 byte-identical하며 고정 SHA는
`a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1` 그대로다.
historical source9와 원래 full module hash, finite/IEEE corpus, parser35 raw JSON 및 old-output
고정 metadata/expected는 수정하지 않았다. 현재 부분 module의 hash로 역사 source pin을 덮어쓰지 않는다.

부재 guard의 정확 경로는 `rust/tools/tests/test_plannerd_native_scheduler.py` 하나다.
부분 module `rust/tools/plannerd_ipc.py`의 존재는 계속 허용한다. native15의 실제 repo 부재 assertion과
feature-gated native IPC lib4의 전후 guard는 다른 목적이며, lib4 private fixture가 실제 전체 IPC 실행을
대신했다고 주장하지 않는다. before 이후 복원된 경로도 after 검사에서 거부하지만 두 검사 사이에
생겼다가 사라지는 모든 transient path까지 감시하는 방식은 아니다.

### 부모 일괄 연결과 남은 실제 수용

부모는 이 후보를 Bacon의 plan/helper/CLI caller 전환 및 Locke의 workflow 연결과 함께 검토해야 한다.
기존 Python IPC CLI 실행 경로가 남으면 이제 명시 실패하므로 source만 분리해 수용했다고 보지 않는다.
이 담당은 plan/helper/CLI, main workflow/ci-isolation, Focus/pins/원장/inventory를 수정하지 않았다.
owned group의 outer timeout/SIGKILL 범위도 위0283 직접 group 감사보다 확대해 주장하지 않는다.

부모가 기존 native15 target `--test planner_scheduler_client`와 feature-gated lib selector
`planner_ci_controller::planner_ipc_native::observed_tests::`의 정확4개, strict Clippy,
실제 HOST/ARM caller 및 남긴 lifecycle import 경로를 삭제 후 source로 검증해야 한다.
기존 native160/parse35와 old-output484 decode/121×4 대조·전후 source/SDK/ELF/cleanup 경계도 유지한다.
새 selector 수는15/4 그대로이며 원래 성공한 변경 없는 cohort 재실행을 이 담당이 요청하거나 실행하지 않았다.

로컬 검토는 Python AST와 보존 span 비교, workflow YAML/Bash 구문·정확8행 차이,
Rustfmt2021 및 diff 검사뿐이다. Cargo/build/test/Clippy/원본/모델/프로젝트 기능 실행은0이고
source 후보의 실제 성공 결과는 아직 없다. GHA 조작·stage/commit/push·raw Git 삽입·새 expected 생성도0이다.

## 후속 lifecycle closure 읽기 전용 설계 — 2026-10-26

앞선7파일 source 후보는 동결했다. 이 section은 남은 lifecycle의 최소 전환 설계이며 구현·등록·삭제
권한을 확대하지 않는다. 부모가 현재 배치에서 진행하는 native 단일 IPC leaf activation과 기존 lib4
성공을 재실행하거나 재다운로드하지 않았다. 기존 cached ZIP의 필요한 summary/소형 log/Params member만
데이터로 읽었고 전체 CRC 재감사는0이다. source 비교 기준은0283이며 아래 네 Python source는 현재와 같다.

| source | bytes / lines | SHA256 |
| --- | --- | --- |
| `rust/tools/check_plannerd_lifecycle.py` |2577 /49 |`17a012f7fa5675a2826b2ed6a5d1cbc5b2ff95c4a8bdcd7428b0caa1950c1697` |
| `rust/tools/plannerd_lifecycle_peer.py` |6173 /145 |`c81e1c54ccf97325544457cd2fa5b5d1130f5f3e7bffd0228458642a0e926169` |
| `rust/tools/plannerd_owner_fixtures.py` |5918 /127 |`03b22ad4d2d0fb87270338e0f485a9936f7c4c8f48613bb9a44887add19da98c` |
| `rust/tools/plannerd_daemon_source.py` |1058 /33 |`4c5ac6ca4d01dfbf748975e8005bbb03502c906f69e63f4cd2353fcd21f9a15f` |

### 실제 소비자와 제거 가능한 경계

`rg`의 실행 source 추적으로 확인한 경로는 다음과 같다. main workflow는 파일명 검색만 했고 전체
single-flow body를 출력하지 않았다. 이 범위에 `INPUTS`라는 Python 정의는 없으며 실제 input roster는
`plannerd_owner_fixtures.SERVICES`의10개다.

| 실제 소비자 | 현재 기능 | 최소 Rust 소유 경계 |
| --- | --- | --- |
| `planner_ci_controller/plan.rs::selected_checks` 마지막 index11 | Python `check_plannerd_lifecycle.py`를 `--python/--binary/--artifact/--binding/--source-native/--output`으로 실행 | 같은 `check-plannerd-ci` ELF의 `lifecycle` subcommand 한 leaf로 교체 |
| `check_plannerd_lifecycle.py` | `Case/run` import,13 case의 source→rust 순서 실행, 중간 cases.json/최종 receipt/PASS 판정 | 새 lifecycle module이 case table과 결과 기록 소유 |
| `plannerd_lifecycle_peer.py` | Params fault 배치, daemon spawn, wait/recover, pub/sub, signal/wait/reap | Rust driver + 현재 C++ peer의 직접 msgq transport |
| 위 peer의 `from plannerd_ipc import OUTPUTS, peers_ready, policy` | 남은 세 helper의 유일한 실제 import 소비자 | 현재 native OUTPUTS, readers loop, `decode::packet` 재사용 후 세 파일 동시 퇴역 후보 |
| 위 peer의 `SERVICES/parameters/car_params/messages` |10개 publisher, Params defaults, CP bytes,9개 input packet | 독립 lifecycle raw 입력/Params corpus를 검증해 replay |
| `plannerd_daemon_source.py`와 원래 `plannerd.main` | 실제 source daemon oracle와 Python 내부 original msgq binding | 유지. source daemon 내부 Python transport까지 없앴다고 주장하지 않음 |

`plannerd_owner_fixtures.py`는 `plannerd_owner_source.py`와 `plannerd_runtime_source.py`도 직접 import한다.
따라서 lifecycle 전환 때문에 이 fixture source나 original loader/daemon을 삭제하지 않는다.
historical source9/고정 output metadata의 문자열 경로도 실제 import가 아니며 원문 pin을 보존한다.
마지막 소비자가 닫힌 뒤 삭제 후보는 `check_plannerd_lifecycle.py`, `plannerd_lifecycle_peer.py`,
부분 module `plannerd_ipc.py`의 정확3파일이다. 아직 삭제/추가 credit은0이다.

### 원래13 case/26 run과 보존할 판정

순서는 wait의 missing→directory→empty마다 sigint→sigterm, 아래 나머지7행 순서이며 각 case마다
source→rust다. 실패한 case의 `ok=false`도 즉시 `cases.json`에 append하고 나머지 case를 계속 수집한다.
26행 뒤 최종 FAIL/exit1이 되는 동작과 setup 자체가 실패하면 중단하는 경계를 구분한다.

| case | run 수 | 원래 판정/종료값 source → rust |
| --- | ---: | --- |
| `wait-{missing,directory,empty}-{sigint,sigterm}` |12 | wait log와 child 생존 뒤 signal; **양쪽 signal 모두** source -2, rust0 |
| `ready-sigint` |2 |3프레임 발행 뒤 SIGINT; -2 →0 |
| `ready-sigterm` |2 |3프레임 발행 뒤 SIGTERM; -15 →-15 |
| `recover-directory` |2 | wait 확인 뒤 CP directory rmdir→valid bytes 직접 write,3프레임,TERM; -15 →-15 |
| `settings-directories` |2 | EnableRadarTracks/UseLaneLineSpeed/LatMpcPathCost/LongActuatorDelay를 directory로 둔3프레임,TERM; -15 →-15 |
| `fatal-integer` |2 | EnableRadarTracks=`invalid`; source SIGABRT -6 →typed Rust1 |
| `fatal-float` |2 | LatMpcPathCost=`invalid`; readers 뒤9 packets 1프레임만 보내고 출력 구독 없이 대기; -6 →1 |
| `malformed-CarParams` |2 | CP=`b'x'`; 양쪽1 |

정상 발행4 case는 mode별3프레임×3서비스, 전체72 raw다. 원래는 서비스 수집과 full-schema parse 성공을
검사하며 valid=true나 projected data의 source/rust 동등성을 요구하지 않는다. 이 검사에 새 valid 조건,
historical 값과 현재 값의 전체 동일성, 출력의 전역 순서 고정을 추가하지 않는다. 원래 한 frame의 drain은
OUTPUTS 순서로 하되 늦게 도착한 서비스의 interleaving을 허용하고, frame마다 세 서비스의 집합을 확인한다.

### 순서·기한·실패/EOF/회수

- case마다 고유 `/dev/shm/msgq_planner_fault_*` namespace, Params directory와10개 publisher를 먼저
  만든다. 기본 Params + EnableRadarTracks=1/MyDrivingModeAuto=0에서 directory/invalid/CP fault를 적용한
  뒤 daemon을 launch한다. CP recovery는 원래처럼 직접 write하며 IPC의 pending/rename 절차로 바꾸지 않는다.
- Python venv 실행명은 absolute만 취하고 symlink resolve하지 않는다. source argv의 `-u -P`, 원래
  `--log-root`, native `--solver`, cwd/Params prefix/PWD/LOGPRINT=info/PYTHONUNBUFFERED=1/SIMULATION=0/
  PYTHONDONTWRITEBYTECODE=1/OPENBLAS_NUM_THREADS=1을 보존한다. `no_core`는 이미 의존 중인
  `openpilot_process_limits::disable_core_dump(&mut Command)`로 child의 soft/hard0만 적용할 수 있다.
- wait는15초 동안 양쪽 log의 `plannerd is waiting for CarParams`와 child 생존을10ms 간격으로 확인하고,
  발견 뒤120ms를 기다린 후 다시 생존을 확인한다. recovery의 CP write는 그 뒤다. readers도 별도의15초/
  10ms poll이며 각 poll 전에 daemon 종료를 검사한다. C++ `readers`의10서비스 전체 AND는 같은 원래
  `msgq_all_readers_updated`를 직접 호출한다. 해당 C++ 함수는 reader/write pointer를 읽는 판정이다.
- ready/recover에서만 output3개를 conflate=false/timeout0으로 구독한다. 매 frame의 input은
  carControl,carState,controlsState,liveParameters,radarState,selfdriveState,carrotMan,livePose,modelV2 순서다.
  liveTracks는 publisher/readiness roster에는 있지만 **이 lifecycle에서는 송신하지 않는다**.
  `i=45+index`, experimentalMode=true, 원래 monotonic header를 갖는 raw를 사용한다. modelV2가 항상 마지막이며
  IPC drive40/scheduler160/초기 liveTracks를 lifecycle에 끼워 넣지 않는다.
- 구독한 frame은 세 서비스가 모일 때까지3초/1ms drain, frame 뒤50ms를 유지한다. fatal-float는9개 송신
  뒤 즉시 frames에서 돌아오며 구독·drain·50ms sleep을 하지 않는다. 정상 signal 후 wait5초, fatal wait15초,
  실패 finally의 KILL/wait5초 및 직접 child reap 뒤 `/proc/PID` 부재를 보존한다.
- 현재 `ChildOwner::finish_daemon`은 TERM/-15/3초에 고정되어 있어 lifecycle에 그대로 호출할 수 없다.
  lifecycle 안에 phase별 signal/expected/wait5·15초를 받는 작은 직접-child 종료 경계를 둔다. source wait의
  SIGTERM=-2, source 숫자 abort=-6, Rust의 typed1을 clean exit0으로 정규화하지 않는다.
- C++ peer의 READY/readers/subscribe/publish ACK/drain END/stop를 그대로 쓴다. `receive(None)`는 원래
  nonblocking no-packet이며 stdio EOF와 다르다. END 전 EOF, partial PACKET/잘못된 ACK, parse error, daemon
  조기 종료는 case 실패다. 빈 발행이나 Python fallback으로 바꾸지 않는다. peer stop/비정상 EOF에는 기존
  3초 회수를 적용하되 원래 case 실패를 보존하고 cleanup 실패도 별도 기록한다.
- outer `NativeRunner`가 lifecycle worker의 process group을 소유하고 peer/daemon은 이를 상속하게 한다.
  `NativeRunner`의 동기 run만으로는 살아 있는 daemon에 중간 Params/IPC/signal 작업을 할 수 없다.
  `qa_process::LiveChild`는 현재 controlsd feature 전용이므로 그 feature를 추가하는 대신 기존 Planner
  ChildOwner/Peer 경계를 작게 재사용한다. 새 generic process framework/worker/Python relay는 불필요하다.
  compiler/peer용 setsid/new detached group을 추가하지 않는다. outer timeout/cancel 때 group kill·직접 wait·
  live-scan은 실제 새 caller에서 확인해야 하며 모든 자손 reap을 주장하지 않는다.

`NativeRunner::capture`는 자체 worker에 `process_group(0)`을 적용한다. 따라서 setsid가 없어도
runtime controller의 outer PGID만 SIGKILL하면 별도 lifecycle worker PGID는 자동으로 죽지 않는다.
현재0283의 직접 IPC group 성공을 이 중첩 kill의 증거로 전용할 수 없다. 최소 통합은 이 worker PGID를
기존 outer 소유/최종 live-scan 범위에 실제 결합하거나, 이미 그 결합이 있다면 source와 실제 관측으로
확인하는 것이다. 현재 진행 중인 부모 helper 수정이 이를 완료했다고 가정하지 않는다.
후속 qualification에서 wait/readers/frame/fatal wait 중 cancel 및 outer SIGKILL의 peer/daemon/worker
비생존을 확인해야 한다. compiler 재사용으로 compiler의 새 중첩 자손은 추가하지 않는다.

### 기존 ZIP에서 이미 확보한 독립 자료와 정확한 빈틈

0283 cached functional HOST12620204855와 ARM12620765642의 lifecycle 경로는 각각
`tmp/openpilot-ci.8iVTmo/planner-comparison/lifecycle/`,
`tmp/openpilot-ci.96xhsD/planner-comparison/lifecycle/`다. 두 `cases.json`은5425B/SHA256
`d14a75be885b14e337887377e6e38aff29d2fd450672ee848ba3f222bbe304a4`로 동일하며26행 모두 ok=true다.
각 archive에는 lifecycle4972 member, output raw72개/40128B, Params4794개/11244B가 있다.
이는 ZIP metadata로 확인한 수량이며 Params 전체 payload나 raw 전체를 다시 읽지 않았다.

최소 payload 읽기는 host마다 receipt/cases, ready-sigterm의 source/rust invocation, 선택한5개 log,
source CP5개뿐이다. receipt348B의 SHA는 HOST
`0717f5dbc540ab9c587acfcf983e32a703340ac44418cbb70a9e3aa22ace2f19`, ARM
`7d10452a813b7f505ca7c3f8ee7971d99ab08f4d96246dc4c160308071fc8fa0`이다.
원래 daemon binary SHA는 각각 `e4e8b5fc4ad6bf8ae2f8a111daee0288bae2c452dd931161fedbb71c01f3cdf5`,
`07dcb113d89ccacb6bd643fe7acbd46b9048070e4b3cdd390b5c18dbde93f7ad`다.
선택한 ready/recover/fatal-float의 source CP는 양 host에서280B/SHA256
`74de9e672b5d93d21bfeeb57ec6f45677bec4dce85afeffcee027b11c15763df`로 같다. malformed는 원래1B `x`,
empty는0B이며 기존 bytes를 재사용할 수 있다. 이것을 모든26 run의 launch 전 상태까지 검증한 것으로 쓰지 않는다.

| 자료 | 현재 보존 상태 | 최소 후속 조치 |
| --- | --- | --- |
|13 case/26 run의 exit/publications/ok 및 invocation/log | 원래 receipt와 raw log 존재 | archive ID/SHA/member SHA를 고정한 작은 audit + versioned case JSON으로 소비 가능 |
| output72 raw | 존재. policy의 반환 중 service만 receipts에 남음 | 후속 native CI에서72 decode와 원래 서비스/프레임 경계 확인. 현 계약은 discarded valid/data의 값 비교를 요구하지 않음 |
| 유효/invalid/empty CP | 선택 case의 실제 bytes 존재 | 필요한 case별 immutable roster를 만들 때 전부 동일성·role을 검증. directory/missing 상태는 별도 상태 manifest 필요 |
| launch 전 Params fault 유형과 recovery 전 상태 | archive는 종료 후 snapshot뿐 | prelaunch 파일/dir/absent 상태와 recovery 전후 변경을 독립 capture로 보강 |
| 실제 송신 packet | lifecycle에 events/inputs/manifest가 없음 | **hard gap**. 원래9×3 또는9×1의 동일 송신 bytes를 capture해야 raw replay가 가능 |
| policy의 독립 projected JSON | 원래 계산했으나 버림 | 필요하면 새 capture에서 이미 계산한 tuple을 기록. decoder 값 회귀용 보강이며 기존 case 합격 조건을 확대하지 않음 |

기존 aa79 IPC401 corpus는 experimentalMode/초기 liveTracks/monotonic header/Params 전개가 lifecycle과
다르므로 처음3 frame을 재사용하거나 native로 bit를 고쳐 독립 입력으로 제시할 수 없다. cached lifecycle
output raw에서 input을 역산해도 안 된다. 현재 확인한 자료만으로 원래 입력 전체의 독립 closure를 선언하지 않는다.

필요한 새 관측은 향후 부모가 배정하는 **같은 원래 lifecycle26 run 안**에서 수집하는 옵션 collector다.
추가 source/native daemon·모델·Python worker/process는0이며, 이미 성공한 CPP lib4/IPC160을 재실행하는
요청이 아니다. hook은 `run`의 launch 전 faulted Params와 CP recovery write, `frames`의 기존 send 두 위치,
기존 policy 반환 tuple이다. capture on에서 PubMaster가 원래 하는 `to_bytes()`를 정확 한 번 수행하고 그
동일 bytes를 같은 publisher에 전달하며, off는 현재 객체 경로를 유지해야 한다.
정상4case×2mode×3frame×9packet=216개와 fatal-float2mode×1frame×9packet=18개, 즉 **host별 input234개**를
각 case/mode/frame/service로 구분한다. source/rust나 case 간 header 동일성을 가정하지 않는다.
expected는 이 원본 자료와 위 실제 receipt에서만 취하고 native 결과로 채우지 않는다.
관측의 archive/source/SDK/binding/receipt identity를 capture 시 내부 artifact ID로 꾸미지 않고 upload 뒤
외부 audit가 고정해야 한다. 새 source/input hash가 없거나 incomplete면 native lifecycle는 출력/child 전 거부한다.

### 최소 소스/API/호출 전환안과 coverage

구현 후보는 `planner_ipc_native/lifecycle.rs`와 작은 lifecycle corpus reader/test다. 같은 parent module 아래
두면 private Peer/decoder/직접-child 도구를 접근 가능하며 대형 공통 framework나 새 bin/feature는 필요 없다.
현재 peer의 명령만으로 lifecycle transport가 충족되므로 C++ source 수정도 필요하지 않다.
현재 IPC manifest는4run/40frame에 고정되어 있어 lifecycle13case/26run을 억지로 넣지 않는다.

권장 CLI는 feature `planner-ipc-native` 안의 `check-plannerd-ci lifecycle`다. 공통 flag13개
`--root/--python/--binary/--artifact/--binding/--source-native/--original-ipc-binding/--corpus/--corpus-audit/`
`--corpus-audit-sha256/--output/--timeout-seconds/--growth-mib`를 strict parse하는 별도 Options로 충분하다.
이는 설계 제안이며 현재17개 IPC Options/API를 변경하지 않았다. lifecycle용 corpus/audit role은 별도로 검증한다.

peer는 직전 성공한 IPC leaf의 `<runtime-output>/ipc/planner-ipc-cpp-peer`와
`native-peer-provenance.json`을 **같은 owned runtime의 정해진 경로에서만** 재사용하는 것을 우선한다.
이를 통해 unchanged C++ 재compile을 피한다. 새 lifecycle leaf의 point-of-use에서 이전 commands의 ipc0,
같은 current controller/source/host/job, 정규 경로·file identity·peer ELF SHA와 실제 provenance의 source6/
header5/services/compiler/original binding SHA를 결합해 검증하고 성공/실패 반환 전에 다시 검사한다.
임의 `--peer` 자기신고 hash만으로 실행하지 않는다. standalone에 이 선행 증거가 없으면 fail-closed하며
새 build/fallback을 암묵적으로 넣지 않는다. source/host/SDK/모드 봉인은 현재 부모 helper 계약을 따른다.

`plan`은 lifecycle index11만 lazy native 준비/호출로 바꾸고 그 앞11개 argv/순서를 그대로 둔다.
기존 runtime growth의 lifecycle256MiB와 HOST2700/ARM5400 leaf timeout을 유지하고 원래128MiB
free-space admission도 낮추지 않는다. 이 outer 기한 안에 각각의15/3/5초 단계 기한을 둔다.
`MEMORY_CHECK_NAMES=[abi,owner,fault]`, ASAN/UBSAN 환경·receipt·순서·기한에는 lifecycle context/corpus/feature
준비를 추가하지 않는다. 실제 native 단일 IPC leaf 활성화는 현재 부모 배치이며 이 section은 새 성공으로 세지 않는다.

후속 coverage는 다음 경계를 닫아야 한다. 테스트 수는 아직 등록하지 않았으며 현재 native15/lib4를 바꾸지 않았다.

- 순수 case table의13이름/26순서/exit 의미, fixture provenance와 case별 input 순서·CP/Params state·원본 bytes.
- 기존 raw72를 native decoder로 읽는 독립 양성, 서비스/경로/union 변조 음성. valid/data는 원래 합격 조건과 분리.
- wait missing/dir/empty, directory→valid recovery,4 settings directory, 두 숫자 abort와 malformed CP의 실제 source/native13×2.
- native peer EOF/partial frame/오류 ACK, daemon early exit, readers/세 서비스 deadline, signal/TERM/KILL wait,
  cleanup 실패에도 원래 case 실패 보존, live group이 남지 않는 timeout/cancel의 실제 검사.
- native plan에서 마지막 leaf만 바뀌는 회귀, memory3/다른11 불변, native leaf 실패42를 commands에 먼저 기록하고
  최종 PASS를 쓰지 않는 기존 첫 leaf 실패 전달. case 내부 예상 -6/1은 정상 판정으로 유지하고 미예상 daemon42는
  원래처럼 해당 case 실패/최종 lifecycle FAIL로 구분한다.
- 삭제 후 정확3경로의 before/after 부재 및 file/symlink/dangling/directory 재등장 거부. historical source9와
  원본 case/finite/parser/233B pin은 변경하지 않는다.
- 후속 실제 HOST/ARM에서 source/native26회, 각 정상 frame의3서비스/전체72 발행,13case PASS와 기존 receipt
  scope/exit_boundary 보존, source/SDK/ELF/owned-group 증거를 확인한 뒤만3파일 완전 퇴역 credit을 검토한다.

현재 필요한 독립 관측은 lifecycle input234개/launch 전 Params state이며, 새 native가 만든 expected로 대체하지
않는다. 이 읽기 전용 설계의 변경은 이 후속 section append뿐이다. 코드/WF/핀/Expected/Cargo 및 실행·stage·
commit·push·CI·download·기존 전체 CRC 재검사는0이다.

## lifecycle234/Params optional collector 구현 후보 — 2026-10-26

부모 승인 범위에서 `rust/tools/plannerd_lifecycle_peer.py`와 `rust/tools/check_plannerd_lifecycle.py`만
추가 수정했다. 기존 준비 소스의 index/stage는 건드리지 않았고 이 기록은 기존 staged 문서 뒤 append다.
새 source의 실제 실행은 아직0이다. 기존0283 성공을 capture234의 성공으로 세지 않는다.

부모 runtime helper가 추가할 export는 다음 한 줄이다. helper/WF는 이 담당이 수정하지 않았다.

```bash
export PLANNER_LIFECYCLE_CAPTURE_DIR="$CI_JOB_TEMP/planner-comparison/lifecycle-original-inputs"
```

미설정이면 collector 생성/검증/파일 I/O 없이 원래 동작을 수행한다. 빈 값·상대 경로·다른 경로·기존 root는
거부한다. Rust run_checks가 만든 `planner-comparison`과 job root의 canonical/owner UID/mode700,
`/tmp/openpilot-ci.XXXXXX`, 현재 Planner Actions job/owner를 확인한 뒤 sibling root를 exclusive mode700으로
생성한다. 원래 `planner-comparison/lifecycle/<case>/<mode>/` 출력/Params/receipt는 그대로다.
새 root는 functional upload 안에 들어가며 부모가 설정할 functional/private90일 보존의 대상이다.

root 형식은 `index.json` + run별 `<00..25>-<case>-<mode>/manifest.json`이며 각 run에 다음 파일이 있다.

- `inputs/<sequence>-<service>.bin`: 같은 packet 객체를 정확 한 번 `to_bytes()`한 bytes를 저장한 뒤
  그 동일 bytes를 원래 PubMaster.send에 전달한다. off 경로의 객체 send 두 문장은 원문 그대로다.
- `params-prelaunch/<key>`: 실제 launch 직전의 regular file bytes다. manifest의 entries는 모든 관측 key와
  CarParams의 absent/dir/file을 구분한다. dir/absent는 bytes를 만들어 채우지 않는다.
- recover-directory의 source/rust 두 run에만 `params-recovery-before/<key>`와
  `params-recovery-after/<key>`가 있다. 원래 CP rmdir/write 사이와 뒤의 실제 상태이며 `.pending`/rename을
  추가하지 않았다. snapshot마다 실제 monotonic 관측값을 기록한다.

run_index0..25와 case/mode를 각각 보존한다. 원래 lifecycle에는 epoch 반복이 없으므로 epoch0은 각
case/mode의 단일 invocation을 뜻한다. source/rust나 case 간 event header를 합치지 않고 원래 wire bytes를
그대로 둔다. 순서는9개 service/modelV2 last로 검사하며 정상216 + fatal-float18 = input234개,
26run/26manifest, prelaunch26 + recovery전후4 = Params30 snapshot이 완료 조건이다.

output72 raw는 원래 functional 경로에 이미 남으므로 복제하지 않았다. `policy(raw)`의 기존 tuple,
service/frame 집합, discarded valid/data에는 collector 접근을 추가하지 않았다. projected JSON·새 finite/
valid/equality 조건·expected 계산은0이다. 기존 original daemon/model/process 수와13case 순서, signal/기한도
바꾸지 않았다. capture 실패로 중단한 실행을 정상26회 완료라고 기록하지 않는다.

파일은 O_EXCL/O_NOFOLLOW로 만들고 regular/owner600/nlink1 및 write 전후 identity를 확인한다.
raw/CP/manifest 각1MiB, 일반 Param64KiB, snapshot2048key/4MiB, 전체 member62000 상한이다.
기존128MiB free admission은 원문 유지하고, on일 때 추가64MiB를 합친192MiB 가용량을 별도로 확인한다.
이는 기존 lifecycle operation256MiB 안의 admission이며 helper budget을 바꾸지 않는다.
파일 payload와 directory를 포함한 apparent size 및 st_blocks×512의 du 상당량을64MiB로 제한한다.
최종 tree/member/hash/identity/source pin/owner/du 검증을 index 전후 수행하며, 실패 시 자신의 COMPLETE
marker를 제거한다. 외부 du/process를 실행하지 않는다.

provenance에는 실제 source14개 SHA, GITHUB_SHA/run/attempt/job/owner, 실제 argv/namespace/제한된 환경,
Python 실행명·실행 target SHA·현재 sys.version, daemon/Params binding/loaded original msgq binding/solver
manifest 및 원래 lateral/longitudinal solver `.so` SHA를 기록한다. 기존 helper의 controller/rustc/SDK-before
proof를 hash로 결합하고 prepared manifest는 실제 env에 있을 때만 추가한다. host에 ARM prepared SDK를
요구하거나 설치하지 않는다. source_native 경로와 environment 값은 실제 값이며 업로드 전 artifact ID는
만들지 않는다. index는 원래 최종 lifecycle receipt의 직렬화 bytes SHA에 결합된다.

collector의 첫 실패는 latch되고 fallback하지 않는다. 송신/복구 중 capture 오류도 원래 try/finally의
child KILL/wait5초를 거치며, 이미 있던 case 오류는 cleanup/capture 오류로 덮어쓰지 않는다.
전체 capture 검증은 최종 PASS receipt 쓰기 전에 수행한다. 원래 case FAIL은 기존 개별 receipt/cases와
최종 FAIL을 유지하며 capture incomplete도 stderr에 남긴다. 수용에는 COMPLETE index뿐 아니라 실제
원래 receipt SHA/status와 job exit0을 함께 확인해야 한다.

로컬은 두 파일 AST, capture=None 분기를 원래 경로로 정규화한 frames/run/main 및 no_core/waiting AST의
원문 동일성, collector의 새 process/메시지 factory/policy 접근 부재와 단일 to_bytes call site, diff만
검토했다. 프로젝트/원본/기능/테스트·CI·download·stage·commit·Cargo/WF/helper/plan/CLI/다른 fixture 수정은0이다.
최종 source SHA256은 peer `9e0ebcca984cce636e309d16f42c5c86c8adf7ffd511480e182d0602d0d3582f`,
checker `15aa1ace3b50f7b40a90532784f0236d723d185d86e2e924c90566373fc6f485`다.
