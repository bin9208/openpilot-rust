# #476 driver 출력 차이: 최신 증거와 최소 후속 설계

작성: 2026-10-26. [#476](https://github.com/bin9208/openpilot-rust/issues/476),
[PR #343](https://github.com/bin9208/openpilot-rust/pull/343).
조사 기준 로컬 HEAD: `126fdbea36598566d1040be0a35a365c909be4e4`.
상태: 사용자 소유 확대 승인 뒤 실제 cached-driver prestate/dispatch API와 독립 original
관측 후보를 구현했다. 고정 PKL과 인증된 과거 object로 whole44 요청을 준비하는 DAG까지
연결했다. 아래 차단 prototype·7개 외부 입력 기록은 역사적 상태이며 마지막 절이 현재 계약이다.
source425의 GHA에서 새4unit과 fresh driver4case는 성공했다. retained/prestate 실행은
fresh 실패 미재현으로 미도달이며 repair 수용은 미완료다. 로컬 기능 실행은 하지 않았다.

## 실제 실패의 정확한 범위

조사한 [run40201352493 / job121194258252](https://github.com/bin9208/openpilot-rust/actions/runs/40201352493/job/121194258252)의
실행 source는 `301f88ccd19a2f0921cdefd75da7615d8cc50984`다. 조회 당시 PR의 실제 head는
이미 `126fdbea36598566d1040be0a35a365c909be4e4`였으므로 두 source를 구분한다.
job은 Ubuntu24/X64에서2026-10-25T14:20:33Z–14:26:42Z 실행 후 failure다.

실제 로그의 최초 수치 거부는 다음과 같다.

```text
same_elf_replays_all_three_native_clis_in_both_absent_roots
driverStateV2.driverStateV2.leftDriverData.faceOrientation[0]
expected=-0.08113645017147064
observed=-0.08065624535083771
tolerance=0.000001
absolute_difference=0.00048020482063293457
```

최초 ordinary1344/raw1 driver에서 실패하여 이후 전체 family/private-root 수용은 완료되지
않았다. 의존성·compile/회귀/Clippy·catalog/USB staging은 해당 job에서 success이며,
실패 뒤 provider/source guard·upload·source seal·cleanup도 success다. 이것이 수치 실패를
취소하지는 않는다. 전체 run은 조회 시 아직 진행 중이었고 취소/재실행하지 않았다.

## 최신 artifact에서 직접 읽은 경계

[artifact12601329579](https://github.com/bin9208/openpilot-rust/actions/runs/40201352493/artifacts/12601329579)는
API상162,791,418B, digest
`d3b821d41bb5a01811930accc35a52a3ff0e2c704743810b07a818b8a52dfd8e`다.
HTTP Range로 ZIP central directory1669항목과 필요한 JSON/IR/object/raw member만 메모리에서
읽었다. 읽은 member는 압축 해제 길이·CRC를 대조했다. ZIP 전체 다운로드·전체 CRC/SHA 재감사는
하지 않았으며 위 ZIP digest는 API 제공값이다. 모델/원본/프로젝트 코드는 실행하지 않았다.

`driver-first-frame-diagnostic/diagnostic.json`은 `model_run_exit=0`,
`retirement_acceptance=false`, `interpreter_execution=not_measured`를 기록한다.
원본 capture source는 `2ecf3f6813f601ea52e7a61ccf75219952daefa0`, manifest SHA는
`532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795`다.

raw6개를 직접 읽어 little-endian float32와 SHA256을 확인했다. 각 파일은2212B/553float다.

| 입력 | 독립 original | daemon | 같은 catalog의 model-run |
| --- | --- | --- | --- |
| warmup | `ace8ee6c6ff7cf8aa6f1881dd9eae6dc9cba15bb75bcc7bd70a5af03ddef500e` | original과 byte 동일 | original과 byte 동일 |
| frame0 | `99b8b2fe7aa17098a366d1bb5e40a2b5a40561f0186ab4dcd33fb29a6dc358fc` | `58456ffe906d7d9aa9dbfe7e530dc55b0251b141511018018eebf78e95800b64` | daemon과 byte 동일 |

frame0 original의 첫3float는
`(-0.08113645017147064, 0.23940423130989075, -0.047499075531959534)`,
native 둘은 `(-0.08065624535083771, 0.23905321955680847, -0.04759492725133896)`다.
diagnostic은439/553개 finite nonzero 차이, 최대 절대차0.01171875,
signed-zero/nonfinite 차이0을 기록한다. PR1344/index99 signed-zero #482와 별개다.

현재 postmortem은 입력 전체를 독립 원본과 비교한 뒤 사용한다
(`model_family_retirement_diagnostic.rs:158`). transform은
`[0.75,0,132,0,0.75,113,0,0,1]`, calibration은0이며 실제 driver source와 같다.
warp 출력은1382400B이고 prepare 직후/model 직후의 native hash가 각각 같다.
warmup hash는 `168f5a411919ccf6a888b24fe139f89b596ba0a061f814babbf0ed396564b30f`,
frame0 hash는 `1079bce5f8773b01849ff8f8ea7fda61344b3dc0f7834e318261acc97e9c8267`다.
단, 이 두 warp는 모두 native 관측이다. 이 실행에서 독립 original warp를 새로 측정한 증거로
해석하지 않는다. daemon과 model-run의 전체 raw 일치로 IPC 없이도 동일 차이가 재현된다는
범위까지만 최신 실행에서 확정한다.

## provider/codegen 경계의 기존 증거와 최신 연결

최신 driver catalog generation은
`adf8c53b9394e2a5f883e50b84fabbdddf123392adcf9f72d784810e04977b2c`다.
compiler target의 before/after는 `generic` 및 같은 실제 host feature 목록
(`+avx512f`, `+avx2`, `+fma` 포함), LLVM18 provider는
`/usr/lib/llvm-18/lib/libLLVM.so.1`, SHA
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`다.
receipt에는 `cached_code`가 없으므로 이 실패는 fresh compilation 경로다.

current1344 bundle의 call3/kernel3 object와 compilation receipt도 직접 읽었다.

- object11640B SHA: `324e24e6c0229c6afd3eeaf7dacf53a8f04abd1233af847ac4972d099fd8980e`.
- original SOURCE SHA: `93ca1a8ac1e234368e2d1eb55e34bc42d0719be636f4c3fcbe67bab20d923412`.
- renamed SOURCE SHA: `61fba094ec0f301174c64a63328df7eaaa29bc9b3102f909ff2aeade7b64d672`.
- compiled IR126433B SHA: `bac91609700ee398c6ded6e81d6c74974dc6225b0ef2cfb2f09aaab42f3fb477`.

최신 object와 original SOURCE SHA는 #476의
[최초 kernel 격리39708871859](https://github.com/bin9208/openpilot-rust/actions/runs/39708871859)
기록과 정확히 같다. 현재 `native.py:32`는 `PROGRAM.src[3]` IR을 실제 host에 재컴파일하고,
원본 retained runtime은 `PROGRAM.src[4]` BINARY를 실행하는 경계가 여전히 존재한다.
관련 diagnostic/driver checker/exporter/transform source는 실패301f와 조사 HEAD 사이에서
변경되지 않았음을 Git diff로 확인했다.

기존 #476 증거는 이미 다음 단계까지 있다. 이번 조사에서 과거 대형 artifact를 재다운로드해
전수 재검증하지 않았으므로 아래는 기존 이슈의 수용된 기록과 최신 object 연결을 구분한다.

1. [39700512883](https://github.com/bin9208/openpilot-rust/actions/runs/39700512883):
   독립 original/native full warp가 같은 실패 context에서 frame0 raw가 갈라졌다.
2. [39712961193](https://github.com/bin9208/openpilot-rust/actions/runs/39712961193):
   같은 IR/동일 prestate의 단일 call이 recorded original target와 실제 target에 따라
   각 기존 output을 재현했다. 명시 target은 진단이며 production target 정책이 아니다.
3. [39741736692](https://github.com/bin9208/openpilot-rust/actions/runs/39741736692):
   실제 원본/native 누산 정책이 하드웨어 endpoint `bfdbe345`/`bfdbe347`를 재현했다.
   최초 차이는 channel13/term2 `%v997`의 separate mul/add `bfea32f6`와 FMA `bfea32f7`다.
   IR이 contraction을 허용하므로 이것만으로 LLVM 결함이나 blanket contraction-off 수정을
   정당화하지 않는다. half 변환 전 차이이며 converter 오류로 입증되지 않았다.
4. [R2 39833861115](https://github.com/bin9208/openpilot-rust/actions/runs/39833861115):
   다른 자연 host에서 call3 전체 bases와 두 model 결과가 일치한 bounded 관측이다.
   passing context 반복이나 CPU 이름 강제로 실패 context를 대체할 근거가 아니다.

R2 observer SHA `43bdff8d13dfffc2655b1362cb07530be17e0aef715b4ca99ae1843b8b085df4`
(32192B)는 workflow `a565c35049f8a9118082dbd1671387c8ef59070c`에서 inert base64로 읽었다.
원래 queue의 core_id 치환 뒤 실제 machine ABI를 관측하므로 이전 R1의 `None` placeholder
오인을 반복하지 않는다. 새 observer가 필요해도 이 경계를 보존해야 한다.

## 권장하는 최소 후속: 인증된 retained PROGRAM의 driver 전용 후보

같은 raw·IR·실패 object가 이미 돌아왔으므로 같은 two-frame mismatch나 FMA watchpoint를
즉시 다시 실행하는 것은 새 판별력이 작다. 우선 수학 변경 없이 기존 native cached 경계를
driver에만 연결하는 후보를 판별한다. 이것은 아직 구현·실행 승인 범위를 넓힌 패치가 아니다.

현재 소스에 이미 다음 입구가 있다.

- `model_family_prepare/catalog.rs:402`: explicit cached issuer ELF/요청 SHA를 받는다.
- `native.py::CachedExport::for_model`: original model의 완전한 SOURCE/BINARY/ABI roster를
  독립 admission과 비교한다. driver warp는 같은 fresh 경로로 유지한다.
- `capture.py:106`: 승인한 model PROGRAM만 retained BINARY로 package하고 나머지는 기존
  fresh compiler를 쓴다. source/image·object 재구성·whole-image ISA·ABI/certificate 검사는
  `offline-cached-code`, `cached_code.rs`, `cached_elf.rs`, `cached_isa.rs`에 있다.
- `model_family_retirement.rs:670`의 기존 ignored selector
  `cached_driver_uses_actual_model_run_and_daemon_before_full_retirement`는 실제 driver checker의
  네 case와 model-run 비교를 수행한다. full retirement 수용을 별도로 요구한다.

다음 필요한 GHA의 같은 소유 failure context에서 한 번만 아래 분기를 제안한다.

1. 기존 실패와 postmortem의 source/fixture/held ELF/provider/target/catalog/raw를 먼저
   결합한다. 실패가 다른 입력·경계이거나 seal이 깨졌으면 이 후보로 진행하지 않는다.
   이미 끝난301f job의 source는 cleanup됐으므로 과거와 같은 VM이라고 주장하지 않는다.
2. 기존 독립 model manifest의 전체44 model PROGRAM SOURCE/retained image를 인증하고,
   전체 original-image↔object 재구성·실제 host ISA·ABI preflight를 실행한다. complete roster는
   독립 admission에서 확정한다. ELF/ISA 미지원은 실패이며 fresh fallback·partial roster·
   source/target/CPU-label 위장으로 통과시키지 않는다. 이전 packaging/ISA 성공은 수치 성공이 아니다.
3. 입력/weights/alias graph/worker ABI/warp/자연 host/compiler 정책을 유지한 별도 private
   driver catalog만 만든다. model의 code identity만 인증된 retained image로 바꾼다.
   기존 failed fresh catalog/기대값은 덮어쓰지 않는다. 초기 read-before-write buffer가 두 경로에서
   같은지 확인하며 다르면 arithmetic 결과를 해석하지 않고 별도 buffer 증거로 분리한다.
4. 기존 cached-driver selector로 실제 daemon 네 case를 비교하고, warmup/frame0의 complete
   raw2212B도 독립 original과 직접 비교한다. packet의 기존1e-6/시간·로그·cleanup 기준과
   raw 보존을 유지한다. byte 일치 후보라도 전체 six-source ordinary/private-root gate를
   대체하거나 기존 failed gate의 exit를0으로 바꾸지 않는다.

판별 결과는 명확히 나눈다. retained 후보가 independent original과 일치하고 fresh만
기존 비영 차이를 유지하면 code-image 선택을 좁은 repair 후보로 삼는다. 두 native 경로가
함께 다르면 다음은 같은 prestate/alias/실제 worker ABI 경계이고, retained warp가 original과
다르면 모델 convolution 조사 전에 그 입력 경계를 분리한다. ISA/ABI/preflight 거부는 실행
미도달이며 수치 실패/성공으로 세지 않는다.

## 초기 조사 시점의 제안 범위와 실행 책임

초기 조사 단계의 실제 쓰기는 이 새 문서 한 파일뿐이었다. 이후 제안한 disjoint 범위는
새 `tools/ci/driver_retained_program_476.py` 하나다. failure-only controller로 기존 입구와
strict selector를 연결하며, 필요 source/controller 검증도 그 파일 안의 별도 진단 입구로
제한한다. shared Cargo/mod, exporter/runtime/compiler/allocator는 수정하지 않는다.
표준 Actions 연결·자원 admission·exact pin/Focus/ledger는 부모 담당이다.
실제 native 초기 buffer 관측을 기존 입구로 얻을 수 없다면 controller가 allocator를
흉내 내지 않는다. 그 증거는 별도 buffer agent의 승인된 관측 산출물과 연결해야 하는
선행 의존성으로 보고하며, 새 runtime 쓰기 범위를 암묵적으로 추가하지 않는다.

`tools/ci/model_pr1344_replay_482.py`, 그 모델 agent의 index99/D2 파일, 다른 agent의
tinygrad buffer/compiler 파일, 부모의 CI/SDK/logging 소스는 읽기만 했거나 건드리지 않았다.
기존1e-6 tolerance·frozen expected·source/model/provider pins·생산 CPU/수학 정책은 불변이다.
새 FMA mask/epsilon/half 보정/expected 교체는 제안하지 않는다.

로컬은 AGENTS/skill/source·Git diff·GitHub job/이슈 원문과 inert artifact의 부분 읽기만
수행했다. Cargo/테스트/build/Clippy/model/oracle/원본 실행 및 Actions dispatch/cancel은0회다.
새 후보는 GHA UNRUN이며 #476 해결·원본 삭제·새 retirement credit·#523/#296 전체 완료·
기기/주행 수용을 주장하지 않는다.

## 2026-10-26 초기 prototype 기록: orchestration과 prestate 차단

사용자 승인에 따라 새 `tools/ci/driver_retained_program_476.py`를 추가했다. 임시 Python
pending execution edge이며 원장/Focus/workflow 연결은 부모 담당이다. 이 파일과 본 문서만
수정했다. 수학/allocator/exporter/공유 Cargo·mod·CI·SDK·logging 소스는 수정하지 않았다.

구현은 기존 source-pinned `controlsd_normal_523.py`의 파일 읽기/hold/snapshot 및
Linux subreaper/pidfd 회수 기능을 재사용한다. 원문 SHA는
`8a5da6caaf97dc00a0672e271130e0ce0ead8c055bee9883f7e503b9904b85a7`다.
common import는 모델/원본을 import하거나 해당 controller의 main을 실행하지 않는다.

동작 순서는 다음과 같다.

1. 실제 Actions `model-family-retirement`/Linux/X64 및 parent source trust, source/job owner,
   source SHA/index, 실제 prepared SDK1.94와 공식 program pins, private venv/base Python,
   LLVM18, 기존 held native ELF와 original fixture를 확인한다. parent의 same-source Cargo
   JSONL/ELF admission과 기존 독립 finalizer를 대신하지 않는다.
2. 기존 fresh failure postmortem이 현재 source/driver receipt/동일 held model-run과 결합되고,
   warmup 일치·frame0 비영 faceOrientation 차이·daemon/model 전체2212B 일치를 요구한다.
   독립 original raw의 두 SHA는 위 기록 그대로다. 다른 실패를 #476으로 우회하지 않는다.
3. 부모가 **독립적으로 인증한** cached issuer 요청의 explicit SHA를 받는다. 요청의 전체44
   unique PROGRAM SOURCE/image/reconstruction object와 original manifest/model 및 ISA receipt를
   hold한다. 현재 source/helper/original/whole-image ISA/ABI 인증은 기존 native issuer와
   catalog preparer가 수행한다. 새 Python은 original admission/ISA receipt를 만들어내지 않는다.
4. `$CI_JOB_TEMP/driver-retained476/preparation`에 fixture를 제한 복사하고 같은 peer ELF를
   복사한다. fresh preparation/catalog는 불변이다. 기존 `prepare-model-family-catalog`를
   한 번 호출해 cached-driver catalog만 준비한다. 임시/cache 경로는 이 새 소유 디렉터리 안이다.
   자연 target/provider before/after는 fresh receipt와 같아야 한다.
5. 후보가 준비돼도 `require_runtime_prestate()`는 현재 항상
   `BLOCKED_REQUIRED_RUNTIME_PRESTATE`로 실패한다. 실제 loaded runtime의 initial
   read-before-write bytes/alias/worker ABI 관측 API가 없다. 이번 buffer agent의
   metadata/initializer wrapper를 runtime 관측으로 인정하지 않는다. JSON/환경 flag/옵션으로
   이 차단을 해제할 수 없다. 따라서 현재 numerical selector 호출 수는 **0**이다.
6. 이후 실제 관측 API가 별도 구현·검토되면, 차단 뒤에 이미 연결한 기존 test ELF의
   `cached_driver_uses_actual_model_run_and_daemon_before_full_retirement`를 최대1회 호출한다.
   새 Cargo build나 성공한 one-call/FMA 진단은 호출하지 않는다. 기존 네 case/packet1e-6를
   유지하고, 후속 두 raw2212B는 metadata뿐 아니라 파일 전체를 독립 original과 직접 대조한다.
   지금 이 경로가 실행 가능하거나 수치 수용됐다고 기록하지 않는다.

`--fresh-exit-status`는1–255만 받으며 admission/준비/의존성 차단/후보 결과와 관계없이
최종 해당 값을 반환한다. `outcome.json`에도 sticky fresh failure와
`candidate_numeric_accepted=false`, `full_six_source_acceptance=false`, retirement credit0을
기록한다. SDK/source/fixture/request/provider hold의 독립 사후 확인과 소유 자손 회수는
오류 뒤에도 시도한다. missing baseline/회수·seal 실패는 진단 결과에도 실패로 남긴다.
후보 생성이나 이 진단의 정상 종료를 fresh/full six-source gate의 PASS로 바꾸면 안 된다.

### 부모 workflow의 exact 입구와 입력 계약

실제 fresh gate가 실패한 **같은 소유 job**의 기존 failure-only branch에 연결한다.
진입 전 원래 `gate_status`를 보존하고 기존 postmortem을 한 번 마친 상태여야 한다.
성공한 gate에서는 실행하지 않으며 기존 gate/diagnostic을 다시 실행하지 않는다.

```sh
"$python" "$CI296_SOURCE/tools/ci/driver_retained_program_476.py" \
  --request "$CI_JOB_TEMP/proof/driver476-request.json" \
  --request-sha256 "$DRIVER476_REQUEST_SHA256" \
  --fresh-exit-status "$gate_status" \
  --deadline-epoch "$DRIVER476_COMMAND_DEADLINE"
```

위 입구는 Python 명령 하나뿐이다. `$python`은 같은 fresh export에 사용한
private venv Python이고, helper 자체의 SHA256도 부모가 source literal로 먼저 pin한다.
초기 차단 prototype 당시 helper SHA256은
`06d466f60237e96c21a46009574d1013d19439490cc638702a2cbb38ca19b2d4`다.
`DRIVER476_SOURCE_ALLOWED`에는 기존 fork 거부와 동일한 GitHub source 비교 expression의
실제 결과를 바인딩한다. custom `true` 상수나 runner 이름/환경 위장은 허용하지 않는다.
현재 `CI296_SOURCE`, `CI296_SOURCE_OWNER`, `CI296_SOURCE_IDENTITY`, `CI_JOB_TEMP`,
`GITHUB_WORKSPACE`, `MODEL_FAMILY_GATE_PREPARATION`, `MODEL_FAMILY_GATE_ORIGINAL`,
`MODEL_FAMILY_STARTUP_MANIFEST_SHA256`, `MODEL_FAMILY_USBGPU_MANIFEST_SHA256`를 보존한다.
`CI_PREPARED_RUST_ROOT`는 실제 fresh compile에 사용한 공식 SDK 경로여야 한다.
새 SDK 설치/별칭 작성으로 기존 context를 바꾸지 않는다.

`driver476-request.json`의 정확 key는 아래와 같다. 추가 key/누락은 거부한다.

```json
{
  "schema": 1,
  "source_commit": "현재 GITHUB_SHA 40자리",
  "source_index": {"path": "소유 job/proof의 기존 index-before.bin 절대경로", "sha256": "parent held SHA256"},
  "preparation": "현재 MODEL_FAMILY_GATE_PREPARATION 절대경로",
  "sdk": "실제 CI_PREPARED_RUST_ROOT 절대경로",
  "python": {"path": "현재 job/venv/bin/python", "sha256": "실제 interpreter SHA256"},
  "python_base": "fresh provider receipt의 실제 sys.base_prefix 절대경로",
  "llvm": {"path": "실제 canonical LLVM18 ELF 절대경로", "sha256": "고정 LLVM18 SHA256"},
  "elfs": {"아래 정확9개 이름": {"path": "절대 ELF 경로", "sha256": "parent held SHA256"}},
  "cached_request": {"path": "job/driver476-original-request/request.json 절대경로", "sha256": "독립 인증된 issuer 요청 SHA256"}
}
```

`elfs`의 정확9개 key는 `prepare-model-family-catalog`, `offline-cached-code`,
`offline-llvm-guarded-add`, `publish-model-catalog`, `model_family_retirement`,
`check-driver-daemon`, `openpilot-dmonitoringmodeld`, `openpilot-logmessaged`, `model-run`이다.
8개 bin은 현재 source의 `rust/target/debug/<name>`, test ELF는 같은 target의
`deps/model_family_retirement-<16hex>`다. 부모가 기존 eager Cargo JSONL의 package/target/
features/source/unique success/terminal 검증으로 선택한 **같은 held ELF**를 제공한다.
helper가 임의 glob으로 고르거나 후보가 제시한 해시를 신뢰 기준으로 삼지 않는다.
current-model PR1344 helper/selector는 호출하지 않는다.

cached request는 현재 source·guarded helper·cached issuer·exporter roster·독립 original의
whole44가 결합된 기존 `IssuerRequest`다. 부모가 기존 original/ISA issuer 준비를 먼저
인증해야 하며, 과거 request의 current/source hash를 고쳐 새 요청으로 재사용하면 안 된다.
immutable 요청 디렉터리의 별도 전체 snapshot과44 source/image/object FD hold를 유지한다.
existing `offline-cached-code`가 승인하지 않으면 준비 자체가 실패한다.

### 기한·자원·보존 계약과 검증 상태

- 실행 단계 shared 최대2400초이며 부모가 준 절대 command cutoff를 넘지 않는다.
  native catalog 준비900초, 기존 selector 계획1320초(내부 driver1200+diagnostic120)는
  그대로다. 현재 selector는 prestate 의존성으로 미도달이다. 독립 finalizer는90초다.
  부모는 command cutoff 뒤90초와 기존 upload/cleanup 여유를 남기며 job 기한을 늘리지 않는다.
- 부모는 기존 growth admission을 유지하고 새 소유 디렉터리의 추가2GiB와512MiB reserve를
  실제 해당 filesystem에서 admission한다. helper도 allocated/apparent 상한2GiB와 남은
  성장량+reserve를 확인한다. fixture 복사 상한512MiB, command log각32MiB,
  child file cap512MiB다. log 한도를 model/compiler 파일 cap으로 전파하지 않는다.
- SDK/private Python/source/fresh fixture·catalog·postmortem·peer/immutable request의 전후
  snapshot, pinned LLVM/ELF·44 inputs의 FD/내용 확인, 기존 common의 subreaper/pidfd
  TERM3초/KILL7초를 재사용한다. 전체 동적 DSO 폐쇄나4GB RAM 적합성/peak 보장은 아니다.
  부모의 기존 provider/SDK/source/finalizer 검사를 모두 그대로 둔다.
- 부모 upload 대상은 `driver-retained476/proof/` 및 필요한 candidate receipt/target/descriptor와
  향후 raw sidecar다. 복사한 models/weights/PKL이나 임시/cache 전체를 upload하지 않는다.
  helper는 source/SDK/후보 디렉터리를 삭제하지 않는다. 부모가 실제 upload/독립 seal 성공 뒤
  기존 소유 cleanup 조건으로 회수한다.

로컬은 새 Python AST와 제어 흐름의 소스 검토, diff 공백 검사만 수행했다. helper/common을
import하거나 CLI로 실행하지 않았고 Cargo/test/build/원본/model/oracle/CI 실행은0회다.
preparation/ISA/ABI/회수/실제 수치 경로는 모두 **GHA UNRUN**이며, runtime prestate API는
명시적으로 미구현 선행 의존성이다. 이번 pending edge는 원본 전환 완료 credit이 아니다.

## 2026-10-26 소유 확대 후 현재 구현: 실제 prestate와 dispatch

#523 전체 작업 범위에서 부모가 driver 전용 새 Rust module/test와 필요한 cached
preparation/runtime hook을 배정했다. 구현 시작 HEAD는 `33923f8fc0b954b4797480e64b984200ac45a138`다.
공유 worktree의 다른 변경을 보존했고 commit/push/Actions/원장/Focus/workflow는 수행하지 않았다.
`cpu.rs`, `model-run.rs`, `dmonitoringmodeld/{main,runtime}.rs`, `driver_daemon_check.rs`,
`model_family_retirement_diagnostic.rs`와 기존 cached selector hook 경로를 구현 전에 공유했다.

최소 관측점은 `CpuModel::load_cached`가 allocation 초기화/weights 복사와 실제 invocation
구성을 마친 직후, 그리고 실제 input 쓰기 뒤 `prepare`/`model` 실행 직전이다. 이 지점에
새 `cpu/driver_prestate_476.rs`의 `write_driver_prestate_476`와 `run_driver_entry_476`를
연결했다. exporter의 `BufferTable.initial` 또는 allocator metadata를 runtime bytes로
대체하지 않는다. 후자는 원래 dispatcher와 같은 순서로 `core_id`를 치환한 직후 실제
function pointer·buffer pointer·scalar를 기록하고 같은 live kernel을 정확히 한 번 호출한다.
기존 `CpuModel::execute` 본문과 일반 fresh/QCOM 경로의 계산은 그대로다.

각 native 경계는 다음을 보존한다.

- 실제 모든 allocation을 `Buffer::read`로 읽은 `.bin`, 길이/SHA, weight offset, 실제 base 주소.
  각 view의 allocation/offset/길이와 실제 bytes SHA, 각 invocation pointer가 같은 live
  allocation/view를 가리키는지 검증한다. snapshot 자체는 bytes를 쓰거나 kernel을 실행하지 않는다.
- loaded graph와 disk graph 일치, driver descriptor, 전체44 독립 admission/certificate,
  original model/manifest·weights SHA, 현재 `cpu.rs`/새 module의 컴파일된 source SHA.
  PROGRAM은 certificate의 `isa_proof_index`로 독립 원본 roster에 결합한다.
- 실제 loaded symbol과 invocation function의 일치, worker/core_id/scalar ABI, 호출 순서,
  entry 실행의 실제 post-substitution dispatch. 기존 held cached library 검증과 실제
  host ISA/whole-image ABI preflight를 통과해야 한다. ISA 미지원은 abort이며 fallback은 없다.
- `loaded`, `warmup-prepare`, `warmup-model`, `frame-0-prepare`, `frame-0-model`의 다섯 관측.
  첫1344/raw1 case의 실제 daemon과 같은 candidate의 실제 `model-run`만 관측한다.
  나머지 case·1e-6·daemon timing/log/cleanup·원본 여섯 경로의 수용 의미를 바꾸지 않는다.

`model-run`의 내부 진단 argv는 기존 cached admission 두 인자 쌍 뒤
`--driver-prestate-476 ABS_DIR`다. 정확한 warmup/frame0 네 stage sequence만 받는다.
daemon도 같은 명시 flag를 사용하며 cached admission 없이는 거부한다. 기존 checker는
명시 cached-driver 경로의1344/raw1에만 flag를 붙이고, diagnostic도 cached admission이
있을 때만 붙인다. 기존 cached selector의 diagnostic 호출을 성공 assertion 앞으로 옮겨
packet이 실패해도 실제 model-run raw/prestate를 보존한다. 원래 assertion은 제거하지 않았다.

### 독립 original과의 실제 비교

기존 cached selector를 최대1회 호출한 뒤 두 native 관측 다섯 개가 모두 있어야 한다.
없으면 `ACTUAL_NATIVE_PRESTATE_MISSING`으로 멈추며 metadata wrapper로 통과시키지 않는다.
그 뒤 같은 새 helper의 내부 `--original-worker` 입구를 소유 subreaper 아래 최대1회 호출한다.
이 입구는 임시 **Python original execution edge**다. 원본 실행의 Rust 전환 완료로 세지 않는다.

worker는 기존 `model_export.original.driver`/pipeline binding을 재사용한다. 독립 요청의 실제
PKL SHA와 전체44 SOURCE/BINARY/launch ABI를 확인하고, source 재컴파일과 compiler cache를
통한 대체 image 생성 요청도 sentinel로 거부한다.
원래 retained PROGRAM을 warmup/frame0에서 실행하며 CPU queue의 `core_id` 치환 뒤 실제
ctypes 인자와 실행 image bytes를 관측한다. 저장된 function을 바꾸는 대신 호출별 facade로
같은 실제 함수를 한 번 위임한다. 기존 catalog preparer와 같은 `CPU:LLVM`/JIT/CPU_COUNT=2
설정이며 target feature/CPU 이름 강제나 FMA/수학 변경은 없다.

원본 메모리는 CPU zero-copy memoryview 또는 NPY/PYTHON 실제 backing storage에서 읽는다.
두 native 경계와 같은 다섯 시점의 모든 allocation bytes를 저장하고 alias/view/input/output,
원본 ins/outs와 PROGRAM 순서를 graph에 대조한다. 실제44 model PROGRAM 실행까지 확인한
`complete.json`은 source/request/graph SHA와 결합한다. 원본 두 출력2212B도 기존 frozen
SHA와 정확히 같아야 하며 expected나 tolerance를 갱신하지 않는다.

전체 allocation raw는 보존하되 prestate 동등성 판단 범위는 원본 **선언된 ins/outs view의
read-before-write 범위**다. instruction별 실제 load/store 또는 outs 전체의 실제 overwrite를
추적했다는 의미가 아니다. 읽히지 않는 scratch bytes를 동등성 증거로 쓰지 않는다. 최초 차이는
`actual-prestate-first-difference.json`에 runtime/phase/allocation/byte·양쪽17B 이내 window와
graph/certificate/source를 남기고 실패한다. raw dump와 원본 계약으로 다음 조사를 이어갈 수 있다.

prestate가 같아도 기존 cached selector의 네 case가 모두 성공하고, 독립 original/daemon/
model-run의 warmup/frame0 **전체2212B**가 같아야만 `candidate_numeric_accepted`를 기록한다.
그 경우에도 `full_six_source_acceptance=false`, retirement credit0이며 원래 fresh failure
exit status를 반환한다. 이미 성공한 과거 one-call/FMA 진단은 호출하지 않는다.

### 정확한 변경 파일과 부모의 실행 계약

이번 구현 파일은 아래 열 개뿐이다. release fixture, #482 Python/Rust diagnostic, compiler/
tinygrad buffer agent, Cargo/WF/Focus/ledger/SDK/logging 파일은 수정하지 않았다.

1. `tools/ci/driver_retained_program_476.py` — 실패 전용 orchestration와 독립 original worker.
2. `docs/naver/python-rust-523-driver-476.md` — 본인 한국어 기록.
3. `rust/crates/model-runtime/src/cpu/driver_prestate_476.rs` — 새 실제 관측 API와 네 unit test.
4. `rust/crates/model-runtime/src/cpu.rs` — Unix child module 연결 세 줄만 추가.
5. `rust/crates/model-runtime/src/bin/model-run.rs` — 명시 cached 진단 flag와 네 entry 관측.
6. `rust/crates/dmonitoringmodeld/src/main.rs` — 명시 flag/cached admission 연결.
7. `rust/crates/dmonitoringmodeld/src/runtime.rs` — loaded와 첫 두 프레임의 실제 경계 관측.
8. `rust/crates/dev-tools/src/driver_daemon_check.rs` — cached1344/raw1 flag 전달.
9. `rust/crates/dev-tools/src/model_family_retirement_diagnostic.rs` — cached model-run flag 전달.
10. `rust/crates/dev-tools/tests/model_family_retirement.rs` — 기존 selector에서 실패 증거 먼저 보존.

부모의 failure-only argv/request schema는 위 계약 그대로다. 새 helper/current native ELF를
실제 최종 source로 빌드하고 exact SHA를 admission해야 한다. `cpu.rs` source가 바뀌었으므로
이전 cached 요청의 current hash를 고치지 말고 기존 issuer로 source/helper/source roster/ISA
요청과 certificate를 다시 인증한다. 기존 `CACHED_SOURCES` 열 개는 임의 확장하지 않았다.
새 module은 기존 catalog publisher의 `model-runtime/src` source roster에 포함되고 receipt에
컴파일된 source SHA를 별도로 기록해 현재 source와 대조한다.

GHA에서 먼저 새 unit selector를 검증한다. 기존 helper가 기능 selector를 한 번만 실행하므로
그 selector를 workflow에서 다시 중복 호출하지 않는다.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-model-runtime \
  --features cached-program-isa --locked --lib cpu::driver_prestate_476::tests
```

네 unit test는 실제 초기 zero/input 쓴 bytes의 비변경 읽기,64MiB/출력 중복 거부,
실제 alias pointer/worker scalar 검증, 독립 tiny extern kernel에 의한 실제 worker0/1 쓰기와
post-core_id 순서를 확인한다. 전체 cached/daemon 원본 비교는 별도 실제 GHA selector의 몫이다.

shared2400초/prepare900초/selector1320초/original worker120초/독립 finalizer90초는 기존
command cutoff 안에 들어야 한다. 초과하면 실패하며 job 기한을 늘리지 않는다. 기존2GiB
출력 성장 상한·512MiB reserve·로그32MiB·child file512MiB도 유지한다. snapshot은 시점당
64MiB, receipt/dispatch 각각4MiB, call/view4096 상한이며 모두 초과 거부다. 세 관측 주체의
raw 상한 합은960MiB지만 fixture/catalog/log까지 합쳐 기존2GiB를 넘으면 실패한다.
현재 hook의 파일 I/O가 strict daemon 시간 기준 안에 드는지도 GHA에서 판별해야 한다.

부모 upload에는 `driver-retained476/proof/`뿐 아니라
`preparation/cached-first-native/model-run/prestate476/`와
`preparation/cached-first-native/driver-output/1344-raw-1/prestate476/`를 포함해야 한다.
prepared PKL/models/cache 전체를 upload하지 않는다. SDK/provider/source/fixture/held ELF/
요청의 기존 전후 봉인과 independent finalizer·자손 회수는 그대로 수행한다.

로컬 AST/rustfmt/diff 검토만 수행했다. helper/common import·CLI, Cargo/test/build/Clippy,
모델/원본/oracle 기능 실행은0회다. 컴파일/원본 실제 ABI 관측/ISA/수치/리소스·시간 경계는
**GHA UNRUN**이다. 이 소스 패치 자체를 #476 해결·6경로 수용·기기 수용으로 선언하지 않는다.

최종 소스 검토 시 공유 HEAD는 `83620dd1d74c03c84370079aada18bbcb9d2b48f`였다.
이 작업의 패치는 아직 미커밋이며 위 HEAD를 구현 commit으로 주장하지 않는다.
부모가 pin할 현재 파일 값은 다음과 같다.

| 파일 | bytes | SHA256 |
| --- | ---: | --- |
| `tools/ci/driver_retained_program_476.py` | 48417 | `80538445fc25fe179bad411f6c93d93959dda661592db3b4cfcf7a6c749a2a49` |
| `rust/crates/model-runtime/src/cpu/driver_prestate_476.rs` | 17345 | `8229e45cae1cf0a5bd30968aac22bb03fa017af7170d9ae32407efa4f145cd83` |

Python AST parse, 소유 Rust 파일의 `rustfmt --check`와 tracked/untracked diff 공백 검사가
통과했다. `git diff --no-index --check /dev/null <새 파일>`은 diff 존재에 따른 exit1이고
공백 오류 출력은0건이었다. `cpu.rs`의 새 module 선언을 제거하면 HEAD source와 완전히
같고, 기존 cached selector의 diagnostic 이동을 역투영하면 assertion 포함 전체 파일이
HEAD source와 완전히 같음을 소스 비교로 확인했다. 기능 테스트를 실행한 결과는 아니다.

## 2026-10-26 별도 failure-only workflow와 non-Unix 보완

사용자 추가 소유 승인으로 `.github/workflows/python-rust-523-driver-retained.yml`을 작성했다.
현 `rust.yml`을 template로 복제하지 않고 기존 standalone
`python-rust-523-model-pr1344-replay.yml`의 소유 checkout/SDK/provider 준비와 독립 봉인·
upload/cleanup 구조를 사용했다. 기존 peer/stager/catalog의 argv는 소스에서 확인했다.
기존 전체 native-family/Planner/Fast workflow·Focus·원장·Cargo는 이 작업에서 수정하지 않았다.

job은 `model-family-retirement`, 표시 이름은 별도 driver 진단이며 실제
`ubuntu-24.04`/GitHub-hosted/Linux/X64/`uname -m=x86_64`만 받는다. runner 이름/ARM/CPUID를
위장하거나 미지원 ISA에서 fallback하지 않는다. VM 기한40분, 첫 guard 기준 command
cutoff34분과 기존 shared2400초 중 이른 것을 사용한다. finalizer90초와 별도
source/SDK/provider 검사·upload·cleanup을 포함할 여유6분을 남긴다. 실제 소요시간은 미측정이다.

기능 순서는 다음과 같다.

1. 실제 filesystem device별 source2GiB + target6GiB + registry2GiB + SDK3GiB +
   venv1GiB + frozen0.5GiB + fresh1GiB + proof/candidate2GiB + cache0.5GiB + apt2GiB +
   reserve0.5GiB를 합산 admission한다. 같은 filesystem이면 합계20.5GiB다.
2. 기존 readonly SDK1.94/private Python3.12.15/고정 LLVM18을 준비한다. 현재 helper/common을
   source SHA로 확인하고 기존 `Candidate.command`, `Holds`, `Children`, `Probe.dso`를
   재사용한다. 새 일반 controller나 별도 Python 파일은 추가하지 않았다.
3. model-runtime을 명시 `--no-default-features --features cached-program-isa`로 빌드한 직후
   `cpu::driver_prestate_476::tests::`만 한 번 실행하고 **4 passed/0 failed/0 ignored**를
   요구한다. 이어 driver 전용 dev-tools/runtime/collector와 기존 C++ peer를 준비한다.
   Cargo JSONL의 terminal/unknown/duplicate/package/source/kind/features/unique executable을
   검증한다. 다른 모델 unit/이미 성공한 one-call/FMA 진단은 실행하지 않는다.
4. 기존 `stage-driver-daemon-fixture`, `prepare-model-family-catalog --kind driver`로
   frozen 원본과 fresh catalog만 준비한다. 실제 `check-driver-daemon`을 collector-current/
   retired-driver/현재 receipt와 명시 argv로 한 번 실행한다. startup/USB/다른 six-source
   root의 전체 replay는 호출하지 않는다. fresh가 성공하면 cached/original은 실행하지 않고
   재현 미도달로 실패한다. 아직 fresh를 실행하지 못했다면 종료 관측값은 `null`이다.
5. 실제 fresh 비영 종료 코드를 기록하고 기존
   `diagnose_first_driver_frame_on_same_host_and_catalog`를 한 번 호출한다. helper의 기존
   warmup/frame0 전체2212B·독립 frozen SHA·same-catalog equality·비영1e-6 실패 admission이
   적용된다. 다른 유형의 실패는 cached 수치 실행의 근거로 삼지 않는다.
6. 인증된 frozen `IssuerRequest`를 현재 경로에 연결한 후 기존 `Candidate.run()`을 그대로
   이어 쓴다. whole44 native preflight → 실제 cached selector 최대1회 → 독립 original
   worker 최대1회다. child owner/deadline는 새로 리셋하지 않는다. 최종 workflow 종료는
   candidate 결과와 관계없이 **실제 fresh 실패 코드 그대로**다. `full_six_source_acceptance`
   및 retirement credit은 계속 false/0이다.

### frozen 요청의 정확한 입력과 신뢰 경계

이 트리에서 전체44 원본 요청을 새로 발급하는 별도 명령/인증된 구체 artifact pin은
확인하지 못했다. 부모가 이미 독립 수용한 frozen `IssuerRequest` artifact를 제공해야 한다.
한 프로그램의 과거 관측이나 최신 fresh catalog로44 기대값을 만들어 통과시키지 않는다.
이 입력이 없으면 첫 guard에서 멈추므로, 아래7개 값이 준비되기 전에는 실행 입력까지
완비됐다고 선언하지 않는다. 부모의 전체 pin/source ledger 작업이 남아 있다.

| workflow_dispatch 입력 | branch push에서 읽는 부모 repository variable |
| --- | --- |
| `frozen_artifact_id` | `DRIVER476_FROZEN_ARTIFACT_ID` |
| `frozen_run_id` | `DRIVER476_FROZEN_RUN_ID` |
| `frozen_source_sha` | `DRIVER476_FROZEN_SOURCE_SHA` |
| `frozen_zip_sha256` | `DRIVER476_FROZEN_ZIP_SHA256` |
| `frozen_request_sha256` | `DRIVER476_FROZEN_REQUEST_SHA256` |
| `frozen_request_member` | `DRIVER476_FROZEN_REQUEST_MEMBER` |
| `frozen_input_prefix` | `DRIVER476_FROZEN_INPUT_PREFIX` |

`codex/feat-523-python-rust`의 push는 새 WF/helper/module/model-run 경로 변경에만 반응한다.
manual도 같은 branch/repository만 허용한다. dispatch 입력을 우선하며, push는 부모가 인증한
repository variable의 값을 받는다. 값은 current candidate의 self-reported SHA로 정하지 않는다.
실행 예시는 명령 계약일 뿐 이번 작업에서 실행하지 않았다.

```sh
gh workflow run python-rust-523-driver-retained.yml --ref codex/feat-523-python-rust \
  -f frozen_artifact_id=ADMITTED_ARTIFACT_ID -f frozen_run_id=ADMITTED_RUN_ID \
  -f frozen_source_sha=ADMITTED_SOURCE_SHA -f frozen_zip_sha256=ADMITTED_WHOLE_ZIP_SHA \
  -f frozen_request_sha256=ADMITTED_REQUEST_MEMBER_SHA \
  -f frozen_request_member=EXACT_ZIP_REQUEST_MEMBER \
  -f frozen_input_prefix=ORIGINAL_ABSOLUTE_INPUT_ROOT
```

artifact/run API의 repository/head_repository·run/source·완료/만료·크기·digest를 확인하고,
ZIP 전체 길이/SHA를 독립 pin과 대조한다. 중복 member를 거부하며 선택한 원본 regular member의
길이/CRC/SHA를 확인한다. 전체 ZIP CRC 검증을 했다고 주장하지 않는다. ZIP 내 코드는 실행하지
않는다. 원본 입력 경로는 `frozen_input_prefix`를 제거한 상대경로를 request ZIP member의
부모 디렉터리에 결합한다. traversal/암호화/symlink/special member는 거부한다.

원래 `admission`의 original model/manifest·44 PROGRAM/SOURCE/image/object/ABI 값은 current
binding 갱신 전후 canonical bytes가 같아야 한다. original manifest/model/ISA receipt 및
44 SOURCE/image/reconstruction object를 같은 독립 pin으로 옮긴다. current helper/ELF/source
8/10 roster/exporter binding만 이번 빌드·fresh receipt와 연결한다. frozen ISA producer의
identity/source SHA와 ISA receipt는 역사 원문으로 보존한다. 이것을 현재 decoder가 실행됐다는
증거로 바꾸지 않는다. 기존 native issuer가 **현재 source의 decoder와 실제 host 기능으로**
whole44를 다시 검사해야만 catalog와 이후 실제 selector에 도달한다.

### 봉인·한도·소유 회수와 부모 인계

source/index·SDK·venv/base Python은 build 전부터 전후 보호한다. helper의 `baseline`에는
같은 이름 재등록 시 기존 값과 일치해야 하는 작은 wiring을 추가했다. standalone의 초기 봉인을
기존 `admit()`가 덮어쓰지 못한다. compiler/linker/helper/CRT와 Rust/Python/LLVM/native ELF의
실제 경로/내용 및 기존 parser가 해석한 DSO closure를 hold한다. venv 전체 파일/링크도 봉인한다.
NumPy의 사용하지 않는 test extension까지 runtime으로 관측했다는 주장은 하지 않는다.
registry는 build 완료 뒤 봉인한다. immutable ZIP/request/ISA·원본44 입력의 hold도 유지한다.

기존 subreaper/pidfd의 TERM3초/KILL7초·adopted wait를 전 준비/실행 command에서 재사용한다.
candidate/proof의 기존 live2GiB guard·command stream각32MiB·child file512MiB·시점당 raw64MiB는
유지한다. 전체 경로의 allocated/apparent cap과 같은-FS 남은 성장량은 command 전후 확정한다.
실행 중 Cargo/publisher의 rename metadata를 부분 성공으로 세지 않으며 source/target/registry/
SDK/venv 등의 command 중 peak cap이나 hard quota를 보장했다는 주장은 하지 않는다.
기존 live candidate guard와 filesystem reserve는 실행 중에도 적용한다. 이 차이와 예측20.5GiB,
34분 cutoff의 적합성은 부모의 resource review 및 실제 GHA에서 확인해야 한다.

항상 별도 source·readonly SDK·provider/child 봉인을 검사한 뒤 upload한다. 실패/미도달도
원문 오류와 counter를 남기며, cache metadata만으로 기능 통과를 기록하지 않는다.
원본 frozen request/ISA/current request/authmetadata와 fresh/cached의 실제 packet/raw/prestate를
upload한다. PKL/models/weights/cache/ZIP 전체는 upload하지 않는다. upload id/digest와 봉인·
회수가 모두 확인된 소유 source/job만 정리한다. active CI 취소/commit/push/GHA 실행은0회다.

부모의 non-Unix closed-scope 검토도 반영했다. `model-run`의 새 len9 옵션만 sequence I/O 전에
`#[cfg(not(unix))]` early error를 반환한다. `_index`로 non-Unix unused 경고도 피했다.
기존 len3/7의 의미는 유지하고 다른 production 경계는 추가로 바꾸지 않았다.

이번 추가 stage 대상은 정확히 아래4개다. 앞 절의 선행 prestate10파일 통합은 별도로 유지한다.

| 파일 | bytes | 현재 SHA256 |
| --- | ---: | --- |
| `.github/workflows/python-rust-523-driver-retained.yml` | 45564 | `57b308e345ef6a9bfa047125a25263d8e3bbdc3d0e87bf657853024ff62a7a70` |
| `tools/ci/driver_retained_program_476.py` | 48697 | `3af04f5c447e5285a1a9ece728ad2034c067dd0c59187424e1880e72f40f2a7b` |
| `rust/crates/model-runtime/src/bin/model-run.rs` | 5335 | `44f50fc0990fa15ca934de66dd7b3c4ac8c7ae4456a65840ec7b1c046c55e14a` |
| `docs/naver/python-rust-523-driver-476.md` | 본 기록 | source index로 봉인 |

검토 시 공유 HEAD는 `73e17ff10ec9be7a7001ab9cbf415a779c436f78`이며 이 패치의 실행/구현 commit으로
주장하지 않는다. 부모가 최종 통합 source SHA와 위 helper pin을 확인해야 한다. 로컬은 YAML,
각 run의 `bash -n`, embedded Python3개/helper AST, Cargo TOML과10개 target source 경로의 정적
대조, model-run rustfmt와 diff 검사만 했다. 프로젝트 CLI/Cargo/test/build/model/oracle 실행 및
workflow dispatch는0회다. **GHA UNRUN**, frozen 입력의 실제 제공·compile/features·ISA·수치·
시간/메모리/성장량은 아직 미확정이며 #476 해결이나 원본 여섯 경로 수용은 선언하지 않는다.

## 2026-10-26 독립 whole44 요청 준비 구현: 외부7입력 계약 대체

이 절은 앞의 `frozen 요청의 정확한 입력과 신뢰 경계`에 있던7개 필수 입력/variable과 실행
예시를 대체한다. 현재 WF의 `workflow_dispatch` 입력은 빈 mapping이며 push/manual 모두
저장소에 봉인된 fixture와 아래 고정 artifact를 사용한다. 사용자가 frozen 값을 제공하거나
native fresh 기대값을 생성하는 단계는 없다. 이번 쓰기는 본인 helper/WF/이 문서3개뿐이다.
선행 Rust prestate/API·non-Unix 보완은 이번에 다시 수정하지 않았다.

### 원본 capture와 고정 PKL에서 얻는 입력

James의 `python-rust-523-model-d2-dd6157-audit.md`215행 이후 reference provenance를 읽었다.
거기의 source `e25a14386582858dae25c6daca5ced3bfb4bacab`/
[run39635596188](https://github.com/bin9208/openpilot-rust/actions/runs/39635596188)은 driving
reference의 생성 경로다. driver의 모델/expected SHA나 최초 compiler 이력을 그 자료에서
차용하지 않았다. driver 자체의 기존 stager·원본 receiver·catalog와 아래 원문을 연결했다.

| driver 고정 원문 | bytes | SHA256 |
| --- | ---: | --- |
| `driver-daemon-original/fixture.json` | tracked 원문 | `c55233339671d092fc7d67485e59c639b6fab1ffb03b1c4ece5138e4e9547a33` |
| `driver-daemon-original/observations.tar.xz` | 8254920 | `0b4cbeb4b8c6201c150ccc85c916e5ff979b66da433c217a2f198ad4bd8c1210` |
| `no-collector/models/dmonitoring_model_tinygrad.pkl` | 13056699 | `33a8ae999035383fd33954483a2e7292a7193fad0baf7126b14a4061ce3e405c` |
| `no-collector/observations/reference-manifest.json` | 29476 | `1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380` |

no-collector reference source는 `481ae6fec39ecc68786c4c2813b1205fb4567141`, 기존 donor artifact는
`12369678295`다. 원본 manifest의 `original_models`, `original_source_inputs`,
`loaded_python_sources`, `capture_wrapper`, `original_checker`, policy를 읽었다.
capture wrapper13326B/SHA `c7e8793e25cdf3a13afd8b87b1e21467c054c2d69439a1621124a3e086282d94`,
original checker14277B/SHA `ea1266dc395dab662319bd3637ad90440989db3d2937756dbf07fbb993520f5b`와
고정 PKL을 같은 manifest가 연결한다. expected packet origin은 그대로 원본
`reference.get_driverstate_packet`이며 candidate message를 oracle로 삼지 않는다.
collector-current source `2ecf3f6813f601ea52e7a61ccf75219952daefa0`도 같은 모델4개를 봉인한다.

로컬에서는 tar를 파일로 풀지 않고 stream/hash와 `pickletools.genops`로 읽었다.
44개 SOURCE/BINARY literal은 모두 원본 PKL 안에 있었다. `pickle.load`, tinygrad import,
원본/모델/compiler/프로젝트 CLI 실행은 하지 않았다. 과거 독립 IssuerRequest가 없다는 이유로
중단하거나 current fresh graph에서44개 image 기대값을 채우지 않았다.

### whole44 object witness 발견과 정확한 한계

[run39700512883](https://github.com/bin9208/openpilot-rust/actions/runs/39700512883)의
[artifact12393290469](https://github.com/bin9208/openpilot-rust/actions/runs/39700512883/artifacts/12393290469)
`model-driver-cpu-context-296-0`를 읽었다. API의 workflow head는
`0e522bf2a968082195f08ffa4692a3f2ecf6d775`, 내부 catalog receipt의 실제 checkout source는
`a29832fd0d046253465621a05f02a91e9cfdb1d1`다. 내부 `models`4개가 현재 고정 fixture의
크기/SHA와 모두 같다. 전체 ZIP20550808B/SHA256
`a318335767594bfe8bcd08878f15e4f6a03bb0d8d13541d56f3b83243b19f461`와258개 member CRC를
확인했다. API 조회 시 expired=false, expires_at=`2026-11-05T00:41:04Z`다. 부모가 새 artifact에
증거를 보존하기 전에 만료된다면 인증 단계에서 실패하며 다른 자료로 자동 대체하지 않는다.

정확한 member prefix는 다음과 같다.

```text
driver-catalog/catalog/1dc28498bdbd261b337f9872f30d68fa9361ab79ebef816737d38a42e897a191/driver-1344x760/kernel-{3..46}.{txt,o}
```

44개 `.txt`의 SOURCE SHA가 PKL의44개와 일대일 대응한다. 표준 ELF section/PC32 relocation을
stdlib로 읽어 전체 image bytes를 대조한 inert 조사에서도44개 모두 PKL BINARY와 같았다.
이를 현재 Rust parser/ABI/ISA 실행 결과로 표현하지 않는다. 비교해 본 R2 artifact12451321365는
43개만 같았고 kernel34 `E_515`는2192B로 고정2176B와 달랐으므로 그 archive를 채택하지 않았다.
sample0 kernel34의 object는3064B/SHA `e76f010e29a2482ed0daeb27ff2edd0d043aaa87e0c563ec7549a70aff5c46d8`,
text2157B/전체 image2176B/SHA `dbed0e579807a73b852cd74aab5a4c49cb46be36d0b8fe804e1edc62b9291847`다.

최종 실행 경계 검토에서 original worker의 retained warp도 별도 admission이 필요함을 확인했다.
앞3개 kernel을 PROGRAM3개라고 부르지 않는다. `kernel-0.txt=copy:36`,
`kernel-2.txt=copy:12`는 object 없는 COPY이고, 실제 retained warp PROGRAM은 `kernel-1` 하나다.
고정 warp PKL1413261B/SHA `a860848b8e91f9739496b21b6cb1b7155212831aca288dd84963d05a3624df2d`의
SOURCE15978B/SHA `2aa1e77caf772d772598811ebcdbfed9feb18e8a24d84967b2fc2141d1ee421b`,
BINARY2336B/SHA `a7a7dac5e141306898b326c5cb122fc83e082257e767dc9996717925806d2156`를 inert로 읽었다.
같은 artifact의 kernel1 object3936B/SHA `6f61d8d14508b7e536a4222d37010524ffec55213ade1ac9bdc58b1b39d32581`는
text2280B/전체 image2336B이며 고정 warp BINARY 전체와 같다. 이 추가 확인도 stdlib의
literal/ELF 데이터 읽기일 뿐 original warp·native parser·모델 실행은 아니다.

이 witness는 보존 object가 고정 image 전체를 재구성한다는 연결이다. **BINARY 최초 생성 때의
compiler/CPU/cache hit/miss 이력을 복구한 결과는 아니다.** 새 receipt에도
`original_compiler_history_proven=false`를 남긴다. 같은 image나 과거 수치 성공으로 현재 host
ISA/ABI/메모리/수치 수용을 대신하지 않는다. 기대 image를 맞추기 위한 SOURCE 재컴파일은 없다.

### 구현한 WF DAG와 최소 API 연결

기존의4unit → driver 전용 fresh catalog/checker1회 → fresh failure postmortem 뒤에 다음을
연결했다. 기존 성공한 one-call/FMA 관측과 무관한 six-source 전체 selector는 호출하지 않는다.

1. Actions API로 artifact id/run/repository/head_repository/workflow SHA/완료/만료/길이/digest를
   검증하고 ZIP 전체 SHA와258개 CRC를 검사한다. archive 코드는 실행하지 않는다.
2. 새 `--collect-retained`가 **기존** `model_export.original.driver(DriverArtifacts(...))`,
   `pipeline.linear_calls`, `_prepare_jit_inputs`, `resolve_params`, `BufferTable`을 사용한다.
   metadata 복원 전에 compiler compile/cache와 CPU PROGRAM 생성/queue 실행을 예외로 막는다.
   고정 PKL의 model stage44개와 고정 warp PKL의 PROGRAM1개에서 SOURCE/BINARY와
   global slot/scalar/worker/core_id를 읽고,
   기존 exporter 순서대로 input/output/call/alias/view/초기 weights를 수집한다.
   PROGRAM 실행0/source compilation0/expected_from_native_fresh=false를 기록한다.
3. 수집한 allocations/views/inputs/outputs/calls/entrypoints 및 초기 weights SHA를 현재
   fresh graph와 **대조**한다. 독립 PKL 수집값이 기대값이며 fresh graph로 기대값을 쓰지 않는다.
   model44와 warp1의 SOURCE SHA에 대응하는 과거 object 원문만 같은 owned request
   디렉터리에 복사한다. COPY는 실행 object를 도입할 수 없다.
4. helper의 `RETAINED_ISA_BRIDGE` 원문을 GHA의 private leaf로 컴파일한다. 같은 Cargo JSONL에서
   고른 실제 runtime/serde_json/sha2/libc rlib와 readonly SDK rustc만 연결한다. 새로운 Cargo
   target/mod 등록·decoder·수학 구현은 없다. 이 leaf는 기존 공개
   `cached_elf::reconstruct` → `cached_code::verify_original_program` →
   `cached_isa::current_host/admit_all`을 호출한다. model44+원본 warp1의 전체 image가 각 고정
   PKL과 같고 기존 ABI 및 자연 host ISA가 모두 허용돼야 receipt를 쓴다. unknown instruction/
   미지원 host는 즉시 실패다. `isa.json`은 기존 model44 schema 그대로이며 별도 `warp-isa.json`과
   `native-api-binding.json`의 warp PROGRAM/PKL pin을 outer request의 `retained_preflight`
   `{path,sha256}`로 인증한다. original worker는 실제 warp SOURCE/BINARY/ABI가 이 사전 수용과
   같아야 첫 stage를 실행한다. warp 재컴파일·runtime image 교체는 하지 않는다.
5. 기존 엄격 `IssuerRequest`를 발급한다. original model/manifest/44 PROGRAM은 독립 수집값,
   current helper/ELF/source8·10/exporter binding은 현재 held source/build 값이다.
   `library=null`, `graph=null`로 시작해 기존 `CachedExport` 및 native issuer의 실제 whole44
   preflight를 다시 거친다. 이어 기존 `Candidate.run()`의 cached selector 최대1회와 original
   prestate worker 최대1회만 허용한다. 원본 warmup/frame0 전체 raw2212B/frozen SHA·비영1e-6·
   actual loaded storage/alias/view/dispatch ABI 비교는 기존 그대로다.

source/provider/SDK 전후 봉인과 같은 `Candidate`/owned subreaper/deadline를 이어 쓴다.
실제 fresh 비영 종료는 모든 candidate 결과에서 sticky이며 실행 전 중단은 null로 기록한다.
full_six_source_acceptance=false, retirement_credit=0을 유지한다. request 생성/ISA metadata만
통과해서 기능 성공을 기록하는 경로는 없다. authmetadata와 collection/whole44 receipt·원문
SOURCE/image/object·bridge source·실제 raw/prestate를 upload하고 PKL/weights/ZIP 전체는 올리지 않는다.

### 부모 실행 계약·성장량·남은 검증

부모가 최종 source/pin/Focus cohort에 등록해 commit/push한 뒤 필요한 GHA만 실행한다.
현재 contract는 추가7입력 없이 다음과 같다. 아래 명령은 이번에 실행하지 않았다.

```sh
gh workflow run python-rust-523-driver-retained.yml --ref codex/feat-523-python-rust
```

job `model-family-retirement`, 실제 GitHub-hosted Ubuntu24.04/Linux/X64, timeout40분이다.
34분 command cutoff와 shared2400초 중 이른 값을 쓰고 finalizer90초 및 마지막6분을 남긴다.
동일-FS 초기 예상 합계20.5GiB와 각 operation cap은 유지했다. 실행 중 proof 한도는 준비
`job/proof`와 candidate 전체를 **합쳐2GiB**로 검사하도록 보완했다. source/target/registry/SDK/
venv/fixture/ZIP 등의 cap은 command 전후 확인하며 동적 Cargo 파일의 실행 중 hard peak quota를
구현했다고 주장하지 않는다. 실제40분 내 완료·RAM·디스크 증가량은 아직 측정하지 않았다.

새4unit의 정확한 GHA feature/selector는 다음이다. standalone WF에 이미 연결했다.

```text
cargo test --manifest-path <source>/rust/Cargo.toml --locked --no-default-features --features cached-program-isa -p openpilot-model-runtime --lib --no-run --message-format=json
<held lib-test ELF> cpu::driver_prestate_476::tests:: --nocapture --test-threads=1
```

4 passed/0 failed/0 ignored를 요구한다. original selector는
`diagnose_first_driver_frame_on_same_host_and_catalog`, candidate selector는
`cached_driver_uses_actual_model_run_and_daemon_before_full_retirement`이며 각각 명시한 경계에서
최대1회다. native admission leaf의 정확한 argv는
`<held retained-admission> <owned collection-with-objects.json> <explicit SHA256> <owned request dir>`다.

현재 shared source의 `ci_policy.rs::source_result_binding`이 이미 허용하는 normal40 fork refusal
원문과 `TRUSTED_SOURCE`/guarded always 조건을 새 WF에 적용했다. shared policy는 수정하지 않았다.
`.github/workflows/python-rust-523.yml`의 changed dependency whitelist는 현재 본 helper/WF/docs/
새 prestate module을 등록하지 않았고,635행의 `Changed dependency invalidates reuse`로 거부한다.
이 등록·개별 source pin·기존 cpu/mod·consumer 역투영 갱신 및 부모 최종 통합 SHA는 부모 범위다.
검사 완화나 이전 source 성공을 이번 실행 수용으로 사용하는 변경은 하지 않았다.

검토 중 부모가 Fast/Planner 통합으로 공유 HEAD를 `73e17ff10ec9be7a7001ab9cbf415a779c436f78`에서
`6d0bd2aaef1bcc0117f1419b9d93ddb7fb603b21`로 진행했다. 기존 common/driver fixture pin은 그대로임을
읽기 전용으로 확인했다. 이 HEAD를 이번 uncommitted3파일의 구현 commit SHA로 부르지 않는다.
인계 직전 공유 HEAD는 부모의 별도 routing 보완 `3f57cccff3e55906bc0fa4e175b838e4995c4537`이며
본3파일은 계속 미추적이다. common/fixture pin과 위 Focus635행 경계는 이 HEAD에서도 같다.
부모가 stage할 정확한 파일과 pin은 다음과 같다.

| 파일 | bytes | SHA256 |
| --- | ---: | --- |
| `tools/ci/driver_retained_program_476.py` | 72571 | `1d58aa2b2e2eb9b911c61051da116def2b2ac3a7f1b9b134e6fe4525c59f0202` |
| `.github/workflows/python-rust-523-driver-retained.yml` | 50808 | `fbd48f5a9e98b602aaa1cfb8e44158ca2ffc37a67e2eac56641479eeead7c329` |
| `docs/naver/python-rust-523-driver-476.md` | 본 기록 | 부모 source index로 봉인 |

embedded Rust source SHA는 `d4eba294652c2cbba380ab9c9cbb8c86e54654cf9985286438ef477439debe52`다.
helper AST/YAML/12개 Bash run의 `bash -n`/embedded Python3개 AST/embedded Rust rustfmt와
source/diff만 로컬 확인했다. 실제 Rust compile·4unit·retained collection·native full44 decoder/
ABI/host ISA·원본 warp1 preflight·cached/original prestate와 수치 비교는 **모두 GHA UNRUN**이다. 새 GHA/commit/push0,
James D2/PR1344·buffer/compiler·부모 SDK/logging/CI/ledger 파일 수정0이다.

## 2026-10-26 source425 실제 driver run: fresh 미재현과 후속 seal 기록 보완

[run40227415638/job121271641246](https://github.com/bin9208/openpilot-rust/actions/runs/40227415638/job/121271641246)은
source `425f29d5ffa4ae0e603ceddf5dce8c605e49bc35`에서 completed/failure다.
실제 시작/종료는 `2026-10-25T19:44:03Z`/`19:54:55Z`다. 현재 검토 HEAD는 부모의 별도
recovery/Focus 변경이 들어간 `dd3a41f44fadda755c90224785a84554fa62318d`이며 driver helper/WF/Rust는
실행 source와 같았다. 이번에는 새 실행 없이 완료 로그와 artifact만 읽었다.

### 최초 원인과 독립 seal 오류의 구분

4unit은 실제 `4 passed; 0 failed; 0 ignored; 68 filtered out`이다. runtime/tools/consumers/
selector build도 모두 성공했다. `fresh-driver`의 실제 command 종료는 **0**이고 stderr는0B다.
stdout6173B는1344x760·1928x1208 각각 raw on/off의4개 성공 report다. raw-on은 각각4frame/
compared_fields188, raw-off는 각각1frame/47이며 report의 parsed_float_tolerance는1e-6,
raw_comparison은 exact bytes다. 이 범위의 driver checker 성공이며 원본 six-source 전체 수용이나
retained PROGRAM/prestate API 실행 성공으로 확대하지 않는다. timing 값으로 성능도 주장하지 않는다.

최초 Python 예외는 `fresh driver did not reproduce a failure; no cached execution`이다.
failure-only WF가 비영 fresh 실패를 요구했으므로 retained 준비 전에 의도대로 중단했다.
unit 실패·ABI/ISA 실패·메모리 prestate 차이가 아니다. selector_invocations=0,
candidate_prepared=false, runtime_prestate_available=false, candidate_numeric_observed=false다.

후속 `independent finalizer failed`에는 실제 bytes/FD 변화 오류가 없다. 기록된 오류5개는
`missing baseline: cached-inputs`, `driver-catalog`, `driver-first-frame-diagnostic`,
`driver-fixture`, `peer`다. 이 baseline들은 기존 `Candidate.admit()` 진입 뒤에야 등록됐는데,
finish는 retained 미도달에도 모두 요구했다. 실제 fresh exit0도 비영 guard **뒤**에서만
args/receipt에 옮기므로 기존 outcome은 `fresh_gate_exit=null`, `fresh_exit_observed=false`로 잘못
남겼다. 독립 마지막 단계는 그 `final_seals_passed=false`에서 AssertionError다.

원래 source/index after와 whole SDK after step은 success, outcome의 all_owned_children_reaped=true다.
snapshot/held 재검사 오류가 없다는 사실과 미래 단계 baseline 미등록을 구분한다. 이 조사로
실행되지 않은 retained input이나 관측 prestate를 존재하는 자료로 취급하지 않았다.

근거 [artifact12612441493](https://github.com/bin9208/openpilot-rust/actions/runs/40227415638/artifacts/12612441493)는
42099934B/SHA256 `cbc56ca968048ffe799ca6939334ff895d193f887970e74163bbe0fdc672276a`다.
메모리에서 ZIP 전체 SHA와290개 member CRC를 확인하고 outcome/commands/4unit stdout/fresh stdout·
stderr를 읽었다. archived helper/model/테스트 코드를 실행하지 않았다.

### bounded 수정과 그대로 유지한 실행 기준

수정은 본인 helper/WF와 이 기록3파일이다. Rust production/prestate/tests는 변경하지 않았다.

1. 실제 fresh command 종료를 비영 admission 검사 **전** args와 `fresh-status.json`에 기록한다.
   정상 exit0도 `fresh_exit_observed=true`, `fresh_failure_observed=false`로 구분한다.
   비영1..255만 retained 경로를 허용하는 guard와 최종 sticky 반환은 그대로다. exit0이면 여전히
   원래 미재현 예외와 primary exit1이며 cached/original을 실행하지 않는다.
2. 기본 source/SDK/venv/base Python baseline은 계속 필수다. fresh 준비를 시작하기 전에
   fixture/catalog/peer도 필수로 선언하고 각각 완성 직후 **첫 사용 전** snapshot한다.
   postmortem/retained 수집에 진입할 때만 해당 단계의 baseline을 추가로 요구한다.
   `Candidate.admit()`에 진입하면 기존9개 전부가 필수다. 시작한 단계의 누락은 계속 실패다.
   모든 등록 snapshot의 before/after·FD/내용·소유 회수 검사는 원문 그대로이고,
   같은 이름의 baseline을 덮어쓸 수 없다는 기존 검사도 유지한다.
3. 독립 seal 단계는 fresh exit0일 때 실제 도달한7개 필수 baseline 및 registry의 정확한 집합을
   요구한다. source/provider/SDK/children 검사를 통과해야 하며 미재현 오류·실제 command exit0·
   모든 candidate flag=false·selector0회도 대조한다. postmortem/collection/native admission/
   cached/original의 command0회와 각 미도달 경로의 파일·symlink 부재까지 확인한다.
   retained가 허용된 비영 경로는 기존9개와 registry, whole44 admission/collection 각1회 조건을
   유지한다. seal 성공이 primary 미재현 실패를 지우거나 전체 기능 성공을 만드는 경로는 없다.

whole2212B/frozen SHA/1e-6·whole44+원본 warp1·자연 host ISA/ABI·필수 source/provider/SDK 전후 봉인·
owned subreaper·shared2400초/40분VM/2GiB proof/cap은 변경하지 않았다. source 재컴파일로 기대 image를
바꾸거나 새 수학·skip/retry/fallback을 추가하지 않았다. changed-files의 source425 성공은
부모가 수용한 별도 결과이며 해당 helper/WF/tests를 다시 수정하지 않았다.

### 부모 stage/pin 및 검증 인계

| 파일 | bytes | SHA256 |
| --- | ---: | --- |
| `tools/ci/driver_retained_program_476.py` | 72901 | `1b69287de0a8f5045da82f008788bf79dceac2587d9f79f2a5810212ef4ee1cf` |
| `.github/workflows/python-rust-523-driver-retained.yml` | 52980 | `5662ad3f13321685a8cae67f2740cb3b889895d007085402a1ab3f2e6a88f5d7` |
| `docs/naver/python-rust-523-driver-476.md` | 본 기록 | 부모 source index로 봉인 |

WF 내부 helper pin도 위 값으로 맞췄다. Focus/ledger/공유 pin·stage/commit/push/GHA는 부모 소유이며
이번에 실행하거나 수정하지 않았다. 부모의 최종 source SHA에 대한 GHA에서 수정한 exit0 기록과
실제 fresh output 봉인을 확인해야 한다. fresh가 다시0이면 primary는 계속 미재현 실패이고
retained 부분은 미도달이다. retained 실행 증거가 필요하다고 fresh 실패를 만들지 않는다.

로컬은 helper AST, YAML,12개 run의 `bash -n`, embedded Python3개 AST, source/diff만 확인했다.
프로젝트 CLI/Cargo/test/build/model/oracle/Clippy 실행0, GHA dispatch/재실행0이다.
실제 source425의4unit/fresh 성공과 이 기록 수정 후의 **GHA UNRUN**을 분리한다.
