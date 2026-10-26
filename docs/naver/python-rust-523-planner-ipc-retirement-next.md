# #523 Planner IPC Python 두 경로 퇴역 후속 설계

2026-10-26. 부모 지시에 따라 후속 퇴역 설계는 이 새 문서에서만 관리한다.
기존 planner-ipc-reader-next.md는 부모와 실제 qualification 감사 통합용이며 이번 작업에서 수정하지 않았다.
앞서 그 문서에 기록했던 계획을 원문 그대로 복사했다. 기존 문서의 앞선 append 정리는 부모 담당이다.
source47fb의 네 C++ run qualification은 API SUCCESS이며 실제121×4/native160/cleanup 판정은 Bacon 감사 대기다.
원본 두 파일의 삭제·코드/WF 변경·GHA 실행은 이번 설계 범위가 아니다.

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
| rust/tools/plannerd_lifecycle_peer.py | plannerd_ipc의 OUTPUTS/peers_ready/policy 직접 import | 이 세 정의를 기존 lifecycle peer로 원문 그대로 이동. policy용 log import도 함께 유지. 새 Python module/worker는 만들지 않음 |
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
현재 reader도 그 summary와 네 identities/coverage를 인증하지만 **출력121×4 raw를 읽는 경로는 아직 없다**.
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

공유 QA framework를 새로 만드는 대신, 첫 최소안은 runtime plan의 ipc 슬롯이 기존 checked-in
Bash helper의 `ipc` mode를 정확 한 번 호출하도록 하는 것이다. 실제357줄 orchestration과
transport/scheduler는 그 안의 기존 `check-plannerd-ci ipc`가 계속 소유한다.
호출 graph는 runtime helper → native12 controller → helper ipc → native ipc이며 재귀하지 않는다.
기존 helper의 start_owned/setsid·kill-group/directwait/live-scan/held 후검증을 그대로 재사용한다.
모든 자손 reap 완료를 새로 주장하지 않는다.

이를 위해 helper의 ipc mode에 plan용 `--output` 전달만 엄격하게 추가하는 안을 부모가 검토한다.
Check.invocation이 붙이는 output은 정확 기존 planner-comparison/ipc이고, inherited
PLANNER_NATIVE_SCHEDULER_EVIDENCE는 기존 ipc-scheduler.tsv여야 한다. 현재 hardcoded
ipc-native output/별도 evidence는 이 plan 경로에서 두 번째 자료로 다시 만들지 않는다.
실제 client/server가 쓴160행을 그대로 바깥 run_checks의 scheduler_calls=Some(160)가 검증한다.
wrapper는 실제 owned child의 첫 nonzero를 cleanup 뒤에도 보존하고, 후검사 실패는 기존 실패를
성공으로 바꾸지 않아야 한다. 새 임의 output·caller·SDK·profile이나 Python fallback은 허용하지 않는다.

plan의 ipc 한 슬롯은 Python prefix 대신 위 Bash/native 경로를 만들고 나머지11개 argv/order는
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
그 뒤에도 독립 old-output121×4 reader와 lifecycle import 이동, source text include_str, plan/helper/
WF actual caller 전환이 모두 닫힌 한 패치에서만 정확 두 Python 파일 삭제를 준비한다.
퇴역 후에는 native ipc 진입 전과 성공/실패 반환 전에 그 두 경로의 file/symlink/재등장을 거부하는
private fixture controls와 실제 caller 부재 검증을 추가하는 안이다. 무관한 원본/daemon은 금지하지 않는다.
부모의 영향 범위 Actions에서 변경된 실제 caller/부재·원래 출력 비교를 확인한 뒤에만 원장 credit을 준다.
이미 성공한 불변 검사나 original capture를 처음부터 다시 실행하지 않는다.

이번 작업은 rg·소스·기존 ZIP member/receipt·보존 metadata 조회와 이 문서 append뿐이다.
rawcapture Git 삽입/새 expected 생성/코드·WF·삭제·원장·이슈 변경/로컬 기능·Cargo·원본·모델 실행/
GHA dispatch·재실행·취소/stage/commit/push는 전부0이다.
