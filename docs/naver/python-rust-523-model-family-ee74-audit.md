# #523 ee74 fullsix 실제 성공의 inert 감사

작성: 2026-10-26. 감사 source: `ee74a75ffa4762397e3498d597e78135ef015d90`.
상태: **driver32/startup16/USB12 및 negative 실제 성공과 원시 packet 감사 완료. #476 원인 해결·six-source retirement credit·전체 전환·실기 수용은 별도 미완료**.

[run40232732815/job121287724267](https://github.com/bin9208/openpilot-rust/actions/runs/40232732815/job/121287724267)은
`workflow_dispatch`, attempt1, `Python Rust 523 exact native model-family caller`의
`rust native model-family retirement` job이며 `completed/success`다.
실제 runner는 `GitHub Actions 1000029628`, `ubuntu-24.04`/GitHub-hosted/Linux/X64다.
시각은 `2026-10-25T20:51:36Z`~`21:07:07Z`, job15분31초다.
원래 exact selector는 **1 passed/0 failed/0 ignored/11 filtered out,417.04s**다.
시간은 해당 VM의 전체 검사 증거이며 성능 개선·기기 frame 수용 측정치가 아니다.

## 감사 방법과 archive 인증

API의 source/size/digest를 먼저 확인했다. 각 archive가1GiB 미만임을 확인한 뒤
`.analysis/scratch/2026-10-25-523-completion/ci/<artifact-id>.zip`에만 보존했다.
예상 추가량511369089B, 사전 실제 가용75788713984B로 admission했다.
초기에는 작은 HTTP Range로 summary/receipt를 읽었고 최종적으로 아래 ZIP 전체 SHA와
모든 member CRC를 확인했다. 큰 bytes나 ZIP 전체 내용을 도구 출력으로 내보내지 않았다.
각 ZIP의 경로 중복·절대경로·빈/`.`/`..` component·backslash·NUL·symlink/특수 파일도 거절했다.

| artifact | bytes | members / 비압축 bytes | SHA256 |
| --- | ---: | --- | --- |
| `12614396134` fullsix | 479098729 | 2915 / 685392047 | `ff107c40b032569cc67a24bd806b7dfd4fe7bd3242eb151778e7eff83e78e670` |
| `12613868561` helper/provider 증거 | 11427866 | 23 / 14253499 | `1009cbba9cb0275eec2dbceecbf2e90536c38614e17fe3d7bb3c661febb07a24` |
| `12614191470` private source 증거 | 291686 | 12 / 2353842 | `2e431ac1711657f3282fb7ace01984533b69f2da04ee86ed773019b9e52e19ff` |
| `12393290469` 과거 object witness | 20550808 | 258 / 37492774 | `a318335767594bfe8bcd08878f15e4f6a03bb0d8d13541d56f3b83243b19f461` |

raw bytes·JSON·Cap'n Proto segment/list pointer·pickle opcode·ELF section/relocation의 데이터만 읽었다.
`pickle.load/loads`, Tinygrad import, 원본/project helper 실행, executable-memory mapping,
FFI/ELF/PROGRAM/모델/컴파일러 실행은0이다. ELF 비교는 section 배치와 PC32 재배치의 바이트
산술만 수행했다. 로컬 Cargo/test/Clippy, GHA trigger/cancel, source/WF/모델/expected/CPU 선택 변경도0이다.
추가한 추적 파일은 본 문서 하나다. 이전 buffer frozen 작업이나 부모 WIP를 이번 source의 증거로 섞지 않았다.

## 실제 full60 및 negative 범위

sourceee74의 caller job을 YAML 객체로 읽어 main `model-family-retirement`와 비교했다.
`Require reviewed maintained model-family source` 한 step을 제외한 job 전체가 같다.
기존 jobs2/debug0/120분·command1200초·stream1MiB·admission·실패 전파·최종 봉인은 유지됐다.
전체 source gate를 fresh4만의 결과로 바꾸지 않았다.

| 실제 성공 범위 | 한 root | 두 root 합계 |
| --- | ---: | ---: |
| driver: collector/no-collector × ordinary/raw-path alias × 해상도2 × raw prediction on/off | 16 | 32 |
| startup: driving/driver × 해상도2 × raw on/off | 8 | 16 |
| USB: 해상도2 × disconnect/retry/cancel | 6 | 12 |
| 합계 | 30 | 60 |

`grouped-native/summary.json`은 `passed=true`, roots2, source paths6/root,
`after_traffic=true`, reappearance24, accepted report final verification12를 기록한다.
12개 report의 실제 bytes/SHA를 summary seal과 대조했고 각각의 stdout JSONL도 report와
전체 행·순서까지 일치했다. USB stdout은 기존 CLI 계약대로 camera/mode/passed3필드이며
report의 전체 packet/worker 기록은 별도로 유지된다.

원시 invocation55개를 모두 읽었다. native CLI53개와 source metadata용 Git2개다.
status는0이17개(성공 report12+help3+Git2),1이32개,2가6개이며 runner error는 전부null이다.

- 원본6경로 각각 regular file 및 broken symlink 복원 ×2root = **24 reappearance 거절**, rc1.
- startup/USB unknown source-policy 및 foreign-output ×2root = **8 거절**, rc1.
- native CLI3개 각각 missing/unknown = **6 거절**, rc2. help3개는 rc0.
- 거절38개 전부 stdout0B. foreign report sentinel4개는 원문 `foreign-owned-report`로 그대로 남아 있다.
- 모든 invocation은 `PATH=/nonexistent`, `PYTHONPATH/PYTHONHOME` 제거를 기록한다.
  raw-path alias는 실제 argv/report 경로 bytes의255와 canonical ASCII target을 보존하며,
  두 alias의 소유 해제·부재 검사는 selector가 traffic 뒤 수행한다.

9개 같은 native ELF·C++ peer2·catalog receipt2·독립 원본/모델 등 **138개 held 항목**이
summary에 있다. source의 `Held`는 열린 FD와 dev/inode/길이/mtime/ctime 및 full SHA를
traffic 전후 다시 확인하고, 마지막 accepted report12개도 FD를 유지한 채 검증한다.
단위 로그의 `collector_pair_keeps_full_helper_catalog_and_final_fd_rejection` 및
늦은 finalization 실패 때 성공 report 제거 검사가 실제 PASS다.

실행한 일반 검사는 support8(ignored4), model-family lib20, driver lib27(ignored1), USB lib3,
정책44/2/42와 strict Clippy다. driver의 별도 forged/omitted publisher/native-source issuer
검사는 이 lib 호출에서 ignored1이므로 그 별도 음성 실행까지 끝났다고 확대하지 않는다.
full selector 자체는 명시한 ignored selector를 정확 하나 실행해 위60+negative를 완료했다.

USB12행 전부 `workers_reaped_and_shared_removed=true`이고 daemon/peer receipt24개는
exit0/signal 없음이다. 기존 driver의 연결 중SIGINT, receive 중SIGTERM, malformed-calibration
기대 exit1, duplicate/no-frame 거절과 collector lifecycle도 각 case의 원래 assertions에 포함된다.
마지막 owned source/alias/target 및 common temporary cleanup step은 API에서 모두 success다.
이것은 소유 자원의 gate 수용이며 시스템 전체 FD/process census나 syscall exec trace는 아니다.
summary가 직접 `exec_trace_collected=false`, `interpreter_execution=not_measured`를 기록한다.

## source6 정확 경로와 baseline

baseline은 계획의 stable dev `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`다.
아래6개는 모두 이 baseline Git tree 및3580 inventory에 포함된다. basename이 비슷한 다른
원본·production Python 파일의 제거 수용을 포함하지 않는다. sourceee74의 inventory는
여섯 항목 모두pending이다. 실제 두 부재 root는 checkout과 그 소유 private copy다.
ZIP에서 파일이 안 보인다는 이유가 아니라 selector의 복원/부재/후속 source 검사를 근거로 한다.

| 정확 경로 | baseline Git blob | baseline 포함 / ee74 inventory |
| --- | --- | --- |
| `rust/tools/check_driver_daemon.py` | `76894648a334de8c71205b6c605b5916914c81db` | yes / pending |
| `rust/tools/check_model_startup.py` | `357b5bb00757fb859be33908b53c949d6a8fd2db` | yes / pending |
| `rust/tools/check_usbgpu_daemon.py` | `0d68ce7a0c68097bd15ab0b4ce3ce51081ee2a1c` | yes / pending |
| `rust/tools/model_logging_capture.py` | `e8a79de628b0c10f87916c86633dde4789b8128e` | yes / pending |
| `rust/tools/model_logging_reference.py` | `692cc4d4843281a4888c9c3a6bb0dab7577e0fbf` | yes / pending |
| `rust/tools/model_startup_reference.py` | `0b21bf360e880d42ff8ba2ab7a0da49db5b7c1ff` | yes / pending |

[#476](https://github.com/bin9208/openpilot-rust/issues/476)의 명시 조건은 six-source credit을
보류하고 cause-specific repair와 전체 원본/actual CLI/두 부재 root gate를 요구한다.
이번 결과는 그중 **정확 source·CPU/provider 조건의 full60·수치·정리·복원 gate 성공**을
새로 입증한다. 원래 실패의 원인과 그 원인에 대한 repair를 분리해 입증하지 않았으므로
**여섯 baseline 항목에 retirement credit을 추가하거나 #476을 닫는 판정은 하지 않는다**.

## driver 원본과 최종 packet

두 reference는 독립 manifest/source로 계속 구분된다.

| dataset | 원본 source | manifest SHA256 |
| --- | --- | --- |
| collector-current | `2ecf3f6813f601ea52e7a61ccf75219952daefa0` | `532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795` |
| no-collector | `481ae6fec39ecc68786c4c2813b1205fb4567141` | `1b8e6cea511e9f212cd7737688b3e2ec0a165b9ff985b492f0e98c4639f18380` |

고정 compact tar8254920B의 SHA는
`0b4cbeb4b8c6201c150ccc85c916e5ff979b66da433c217a2f198ad4bd8c1210`다.
그 원본 packet을 직접 읽었다. native의 **112개 image input**은 독립 manifest의 길이/SHA와
모두 맞고 calibration48개도 맞는다. compact tar에서 생략한 image를 로컬에서 다시 만들지 않았다.

native warmup32+최종 frame80 = **112 packet**을 읽었다. Cap'n Proto Data pointer를 데이터로
읽어 raw-on의 **80쌍 ×2212B =176960B 전체**를 각 dataset의 고정 expected와 직접 비교했다.
전부 byte-identical이다. raw-off32개에는2212B Data가 없고 원본도 같다. 최종 packet80개는
각 report의 packet 길이/full-packet SHA와도 맞는다. warmup은frameId10, 이후 frame0..3은11..14다.

raw-on 두 해상도는 warmup+4frame, raw-off는 warmup+1frame이다. 각 packet의 기존47개
field 비교, warmup47·raw-on188/raw-off47의 후속 field 수가 유지됐다. 총5264 field-check
단위이며 이 수를 float 개수로 해석하지 않는다. collector16case는각5 logMessage/5 disk record,
error0이며 current sourceee74의 logging source-call 비교가 PASS다. no-collector16case는 logging=null이다.

parsed driver float 정책은 **abs(a-b) <= 1e-6 * max(1,abs(a),abs(b))** 그대로다.
공통 comparator의 Coefficients2e-5 branch는 DriverStateV2에 해당 field가 없어 적용되지 않는다.
raw Data는 hex 문자열의 정확 일치로 비교하므로 float tolerance가 적용되지 않는다.
clock과 model/gpu timing3필드는 원문 수치 equality 대신 기존 send/receive 구간·순서·
`0 < gpu <= model < 30` 계약을 검사한다. 이 세 변동 필드 때문에 전체 Cereal packet의 SHA를
원본 packet SHA와 같다고 요구하거나 같았다고 주장하지 않는다.

아래는 각2212B raw의 SHA다. root/collector/alias8조합과 두 원본 dataset에서 같은 값임을
확인했다. parsed float 수용은 위47-field gate 증거이며 이 inert 감사에서 모든 Cereal field의
독립 decoder를 실행한 것은 아니다.

| 해상도 | packet | raw2212 SHA256 |
| --- | --- | --- |
| 1344x760 | warmup | `ace8ee6c6ff7cf8aa6f1881dd9eae6dc9cba15bb75bcc7bd70a5af03ddef500e` |
| 1344x760 | frame0 | `99b8b2fe7aa17098a366d1bb5e40a2b5a40561f0186ab4dcd33fb29a6dc358fc` |
| 1344x760 | frame1 | `72a54d1e976ac4511770131c7c86445279169d4b8ce6a5ec1eb14aa2adb16d6e` |
| 1344x760 | frame2 | `cf3235b49d3210718416ac42c34eef0f1c2eeaa365aa1e6335911806c34ff244` |
| 1344x760 | frame3 | `1f82fbeabc2ea479122740cf0274c8af8403312fd09f0a2f9dce3af4a52fd8f8` |
| 1928x1208 | warmup | `2c02a83c9a9ca11911fba3777c45cc92f2e0a94cde488e1820509425b024e033` |
| 1928x1208 | frame0 | `e467ac488bd98c8f7fc73505f0a254aa5ff60d75cc3d0f1d4b3780f565b83281` |
| 1928x1208 | frame1 | `4a0836bcc98e464ee8abff88a13dff9220c0d690e10063c0b54ceeb5963f94f0` |
| 1928x1208 | frame2 | `8217489bac691816a2c8310b06eebc74d1d2f1e3f64ea162feab18d5b481d76c` |
| 1928x1208 | frame3 | `2d2d3420e527c41c97e66bb589427629201f2fe4dea6092c383bc51a49947c84` |

#476 최초 문제였던1344 frame0의 raw 선두3개 float는
`(-0.08113645017147064, 0.23940423130989075, -0.047499075531959534)`다.
이 sample의 frozen expected와 동일하며 native 값을 새로운 expected로 저장하지 않았다.

## startup/USB가 실제로 의미하는 범위

startup16행은 모두 native가 CarParams 이전·frame 없는 상태에서 model을 load하고
`frames_sent_to_native=0`, native_exit0이다. 독립 source initialization 및
source_first_frame **metadata**를 비교한다. driving road-image의 실제 numerical inference를
실행한 검사로 확대하지 않는다.

USB12행은 독립 원본의 controller/packet/state 결과와 native protocol fixture를 비교한다.
mode는 disconnect4/retry4/cancel4다. actual camera/worker lifecycle·packet·최종 Params를
검사하고 cancel은 기존대로 packet=[]를 요구한다. genuine asset admission은45개 asset·
3개 license roster, 원본 release `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`를 유지한다.
원시 artifact에서44개 asset 및3개 license role의 길이/SHA를 직접 재검증했다.
776634338B의 `model.pkl` 한 asset은 원래 upload exclusion으로 ZIP에 없으며 native held seal과
admission으로 확인된다. 그 파일을 이번 로컬 감사에서 다시 읽었다고 주장하지 않는다.
release executable 추출 및 model pickle 실행은 admission에서false다.

USB 원본 packet에는 drivingModelData/modelV2 등이 있지만 이것을 이번 actual driving-model
road-image numerical 수용으로 해석하지 않는다. #482 driving first-frame/signed-zero와
장치/주행 수용도 이 fullsix 성공으로 대신하지 않는다.

## CPU/provider 및 compiled catalog

| 비교 실행 | 실제 CPU / LLVM target | 확인된 결과 |
| --- | --- | --- |
| source126, run40209691630/job121222236470 | Xeon Platinum8573C / sapphirerapids | fullgate 첫1344 frame0가1e-6 밖에서 실패 |
| source425, run40227415638/job121271641246 | EPYC7763 / znver3 | 동일 collector/ordinary fresh4 성공 |
| sourceee74, 이번 run | EPYC7763 / znver3 | full60+negative 성공 |

126의 PR head는 `126fdbea36598566d1040be0a35a365c909be4e4`, actual checkout은
`63d9c91be30b555e3bf8e08ba3ef143d004fb19e`였다. 이전 감사의 두 tree 동일 확인과 구분한다.
126의 첫 faceOrientation[0]은 expected=-0.08113645017147064,
observed=-0.08065624535083771, abs difference0.00048020482063293457이었다.

이번 driver/family receipt의 실제 CPU/target이 같고, compiler-target before/after,
helper/publisher before/after도 같다. EPYC는 AVX2/FMA/F16C+, AVX512-이며126은 AVX512/AMX+였다.
세 실행의 모델 LLVM18 DSO는 canonical `/usr/lib/llvm-18/lib/libLLVM.so.1`, SHA
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`로 같다.
같은 LLVM 파일이라는 사실을 모든 provider/SDK closure가 같다는 뜻으로 확대하지 않는다.
CPU_COUNT2와 sha2의 dev/test opt-level3도 원래 main/fresh 준비에 공통이다.
CPU/ISA를 강제하거나 unsupported fallback을 도입한 source 변경은 하지 않았다.

| catalog | generation | files / 모델 수 | 실제 file seal 검증 |
| --- | --- | --- | --- |
| driver | `d96f38f80dd006583de54eef802870c1ca693fe8905b02f8a6c636ab2295e369` | 378 / 4 | 28252371B, source txt94·object90 |
| family | `563da472e989c7c85d2b5fd7ddebb209bc482156abada36d9ce6df2f2165acec` | 1122 / 5 | 168533701B, source txt280·object272 |

합계1500개 catalog file의 길이/SHA를 artifact 원문으로 직접 확인했다.
receipt의 source는ee74이고 publisher는 same-source native ELF로 봉인됐다.
두 receipt의 `stored_jit_executed=false`, `original_jit_execution_admitted=false`다.
각 current catalog는 별도로 준비됐으며 four/five model을 바꿔 끼우지 않았다.

driver 두 kernels.so는 각각368184B다. SHA는1344
`80f9523ffe89d7fca963bed89642dcaa71d4f8be3886e13950763e60f0e7a247`,1928
`cdc0bdd22afc294926ff8961a659e01def560088df1866e64b4d04692f3768d6`로 이전425 감사와 같다.
이번 guarded-add ELF는1388712B/SHA
`912852534a433b5a0e66994a0c1c9b2848418adfc62e489d271914de57586ec1`다.
425와 helper build feature/ELF가 다른 사실도 유지한다.

126→425에는 guarded prestate API 외 전역 serde_json `float_roundtrip` 변화도 있다.
prestate=None의 ordinary 실행 순서는 원래대로지만 이 전체 source/build 차이와
host codegen 차이를 통제해 각각의 영향을 입증하지 않았다. 이번 성공은 Intel 실패 조건의
cause-specific repair 검증이 아니며 CPU 자체가 원인이라고도 단정하지 않는다.

## 고정 PKL4와 retained44/warp의 inert 분류

두 driver dataset과 이번 native catalog의 모델4 seal은 동일하다.
고정 tar 안의4 PKL을 `pickletools.genops`로만 읽었고 어떠한 pickle 객체도 복원·실행하지 않았다.

| 고정 PKL | bytes | SHA256 | SOURCE/BINARY literal 쌍 |
| --- | ---: | --- | ---: |
| `dmonitoring_model_tinygrad.pkl` | 13056699 | `33a8ae999035383fd33954483a2e7292a7193fad0baf7126b14a4061ce3e405c` | 44 |
| `dmonitoring_model_metadata.pkl` | 818 | `8139895b0f810e351b1e3e9fd649ffcc80d7fe26583e8482f58f341c7d2900d5` | 0 |
| `dm_warp_1344x760_tinygrad.pkl` | 1413261 | `a860848b8e91f9739496b21b6cb1b7155212831aca288dd84963d05a3624df2d` | 1 |
| `dm_warp_1928x1208_tinygrad.pkl` | 1413224 | `11315cf0b50c3177ea0601391e3a5965b4feb02144d130d31e66050b1e349b1c` | 1 |

즉4파일 각각에44개가 있는 것이 아니다. **공통 모델 PKL 하나에44개**, metadata에0개,
각 해상도 warp PKL에 별도1개씩 있다. 모델44개 image pool은 두 해상도가 공유하며
warp2개는 서로 다르다. 전체 unique SOURCE/BINARY pair는46개다.
각 pipeline의 source roster는44 model PROGRAM+warp PROGRAM1+COPY2개이며,
`kernel-0.txt=copy:36`, `kernel-2.txt=copy:12`를 PROGRAM으로 세지 않았다.

| warp | SOURCE SHA256 | BINARY bytes / SHA256 |
| --- | --- | --- |
| 1344x760 | `2aa1e77caf772d772598811ebcdbfed9feb18e8a24d84967b2fc2141d1ee421b` | 2336 / `a7a7dac5e141306898b326c5cb122fc83e082257e767dc9996717925806d2156` |
| 1928x1208 | `10d1c7ad6eb74c1a70621edc00a794cf89484011c9f519f5b1e96fea15a17e3f` | 2288 / `d840d99739e4027a8be465c9b308dfb50e4591e33ad2f27d601df2e3d5e50034` |

과거 object witness12393290469의 두 해상도object90개를 section/PC32 데이터로 읽어
PKL의 SOURCE별 full BINARY와 비교했다. **90/90 일치**다. 모델44개가각2복사와 warp2개다.
이것은 표준 ELF의 바이트 비교이며 current native decoder/ABI/host ISA 실행 수용이 아니다.
PKL 최초 BINARY를 만든 compiler/CPU/cache 이력도 여전히 미확정이다.

witness의 API workflow head는 `0e522bf2a968082195f08ffa4692a3f2ecf6d775`,
내부 catalog receipt의 actual source는 `a29832fd0d046253465621a05f02a91e9cfdb1d1`다.
ee74가 새로 만든 독립 expected라고 잘못 연결하지 않는다. 원문 object/source prefix는
`driver-catalog/catalog/1dc28498bdbd261b337f9872f30d68fa9361ab79ebef816737d38a42e897a191/`이며
두 `driver-1344x760`/`driver-1928x1208` 안의 `kernel-1` 및 `kernel-3..46`을 연결한다.

추가로 이번 fresh object90개를 같은 독립 PKL image와 inert 비교했다.
88/90은 같고, 두 해상도의 `kernel-34`만 다르다. 두 경우 SOURCE는 같지만 image는 다음과 같다.

| `kernel-34` image | bytes | SHA256 |
| --- | ---: | --- |
| 고정 PKL/과거 witness | 2176 | `dbed0e579807a73b852cd74aab5a4c49cb46be36d0b8fe804e1edc62b9291847` |
| 이번 fresh, 양 해상도 공통 | 2192 | `9732008d854cdf34838b655a30ed37a84da53b96da12a59c793f7b440eec3058` |

따라서 이번 full raw 성공으로 fresh image를 retained expected에 덮어쓸 수 없다.
SOURCE 같은44개라는 이유로 whole44 BINARY 일치나 actual cached execution을 선언할 수도 없다.

### Nov5 witness의 향후 필요성

[artifact12393290469](https://github.com/bin9208/openpilot-rust/actions/runs/39700512883/artifacts/12393290469)는
API 조회 시 expired=false, 만료 `2026-11-05T00:41:04Z`다.
이는 고정 PKL image를 원래 object로 독립 연결하는 기존 retained/prestate 준비 경로의 입력이다.
이번 fullsix는 fresh compiled catalog를 썼으므로 그 의존성을 대체하거나 실행해 없애지 않았다.
향후 strict retained/prestate/native ISA/ABI 진단 또는 compiler image 비교를 계속하려면
인증된 archive/object/source pin을 만료 전에 장기 보존하고 부모가 실행 가능한 배포 경로를
연결해야 한다. 로컬 캐시는 인증돼 있지만 GHA가 자동으로 읽을 수 있는 보관소는 아니다.
current fresh object로 자동 대체하거나 SOURCE를 재컴파일해 기대 image를 바꾸면 안 된다.

이번 fullsix/helper artifact는Nov1, private source artifact는Nov8 만료다.
보존·재게시·새 GHA 실행 권한을 추정해 행사하지 않았다. 성공한 불변 검사의 반복도 하지 않았다.

## source/provider/SDK 봉인의 실제 범위

private proof의 SOURCE_COMMIT과 ownership/workflow SHA가ee74다.
index before/after는 bytes 동일하고19557개 path가 sourceee74 Git tree와 맞는다.
`grouped-native/source-copy.json`도19557개를 기록하고 selector가 실제/private root의
원본6 부재 및 copied source를 traffic 뒤 재검증한다.
전체 tracked source tar의 전후 digest는
`7e38d5bb2258ca756d5d9a06bed3ee6c3813d21a629b46160160029f486d23f2`로 같다.
이 tar digest는 실제 GHA가 계산한 증거이며 로컬에서 전체 tar를 새로 만들지 않았다.

bridge binding은116행/106 unique path다. identities.before/after bytes가 같고
bindings.after는 전부OK, final-guards.status=0이다. bound 외부 파일은 LLVM18 DSO와
canonical Python3.12.15 실행 파일이고 나머지는 owned native helper/publisher 및 source다.
Python package freeze before/after도 같다. bounded sidecar2018파일/52670709B, compilation
receipt6개가 보존됐고 archive SHA는
`29fc95eb04c68cd766e9592e0ba3f6763f94863f43b794be8ca9e425475c71e0`다.

Rust SDK version은 rustc1.94.0 commit `4a4ef493e3a1488c6e321570238084b38948f6db`,
cargo1.94.0, Clippy0.1.94, SDK LLVM21.1.8이다. 모델 provider LLVM18과 구분한다.
**이 exact-main job artifact에는 SDK 전체 file/roster의 before/after 봉인이 없다.**
Rust tool version/provisioning과116행 helper/provider/source 봉인은 확인했지만,
buffer100의 prepared SDK294파일 봉인과 같은 whole-SDK proof가 있다고 주장하지 않는다.
이는 원래 main copy의 증거 범위를 기술한 것이며 이번 audit에서 기준을 완화하거나
source를 고쳐 다른 성공을 만든 것은 아니다.

원본 driver manifest/packet과 모델4·family 원본 manifest/packet·모델5는 native gate의
held 검증을 통과했고 actual stored Data도 위와 같이 직접 맞았다. 전체 Python
stdlib/import/DSO closure나 interpreter exec 부재를 이 자료만으로 추가 수용하지 않는다.

## 12개 최종 report seal

아래 report는 각 원문 길이/SHA와 stdout 계약을 실제 대조했다.

| report label | bytes | SHA256 |
| --- | ---: | --- |
| `root-0-driver-collector-ordinary` | 205713 | `5bdc6055b48fe9faf26ffeb803455026a7b25145ca7177520624c3889e78bc4a` |
| `root-0-driver-collector-raw` | 205719 | `23b50929697d86a59c33448247b67caa571bfa94d18a03734e514a818b84d87b` |
| `root-0-driver-no-collector-ordinary` | 202040 | `d3477744c08aebf52858b6fd47f2a80fa38560922d43a12c3c5de4efb9ede875` |
| `root-0-driver-no-collector-raw` | 202040 | `7dce5088574a48b389698c68fe61f3c7015b4081cd39927275be66bbee20ab56` |
| `root-0-startup` | 133160 | `973853ce22d6bb6b59f4633a9d18cc39a307a7f53db860842331db540100c142` |
| `root-0-usbgpu` | 239914 | `fb23771a531ba793be69907dbf5164a0f96f6b3025ef78edc48a3af570b832df` |
| `root-1-driver-collector-ordinary` | 202771 | `6ed39f862efb07f03f5edd1b05d51c86c97dea2c6b6863f781e87ec2a0a2ed18` |
| `root-1-driver-collector-raw` | 202775 | `0d2a0c57869e16422704e0b883c90430bb7388ac367e6464f464d277fec594db` |
| `root-1-driver-no-collector-ordinary` | 199167 | `7882f066b1b87d3ce3c3563cc1f4e6074b1b14069f0bf3afd897ce5d9d4f4146` |
| `root-1-driver-no-collector-raw` | 199160 | `4a1958e66de8446ce3fbe3f510be61b6cc99d180f5e2db0944275a726d4a2047` |
| `root-1-startup` | 133176 | `7b96aacd91fba68642df64e4a26095e404366917bb9f5b26a6e77a9024f3e416` |
| `root-1-usbgpu` | 239916 | `217345cc85ea10d6097e9dd2a9bfb46c76054a571cabc6748de8c7faab16a2ff` |

이번 감사의 완료 범위는 위 exact-source full60+negative·원본 raw·compiled catalog·소유 정리와
보존 증거의 확인이다. 원인 repair/다른 CPU·cached ABI/ISA·실제 prestate·driving road-image·
전체 Python 제거/최종 PR·전달·장치 수용은 각각 남아 있다. parent가 이슈/ledger에 후속 기록한다.
