# #559 UI recording pending-close 현재 검증 경계

2026-10-25. 실제 [c00 run40154418322/job121052614014](https://github.com/bin9208/openpilot-rust/actions/runs/40154418322/job/121052614014)은 failure다. #559의 구현 전체 수용, 원본 제거 또는 retirement credit은 아직 없다.

## c00의 실제 결과

source `c00c7a11fbee357cbfb39bee45125d73596bf56c`, helper SHA256 `10cb3d55ba9a655bd1239843c4d5ab6e414695d5fef91bf0fdb52cd39682381a`, workflow SHA256 `b60233dbcded60ae051e986b1bf76b9b3b73bf6670824952f6a89799ee95d397`. actual GitHub-hosted Ubuntu24.04 X64/private Rust1.94.0에서 decoder SDK 준비, fresh baseline RED1(의도한 정확 실패), candidate GREEN1, recording9, strict Clippy까지 성공했다.

최초 실패는 `five_original_modes_preserve_real_graphics_ticks_and_native_encoder`의 첫 profile case다. child exit1에 `application original source changed: openpilot/system/ui/lib/application.py`가 기록됐고 Rust test exit101/helper exit1로 실패했다. source admission 뒤 실제 graphics/PNG/frame/encoder 비교에는 도달하지 못했으며 나머지 네 mode도 실행되지 않았다. shared1200초 및 파일/성장 cap은 변경하지 않는다. 실패 메시지는 deadline/provider가 아니라 source pin 거부다.

[artifact12584138814](https://github.com/bin9208/openpilot-rust/actions/runs/40154418322/artifacts/12584138814)은40,897,338B/SHA256 `f3f7a1b25e5ceeb84beca5fb17f2655e8a462856ecd358f2b5861fe56aec6718`, 전체25,304 member CRC/SHA·expanded206,632,400B 감사 완료. 실패 finalizer0, held372paths identity/SHA 동일, source/SDK 및 baseline/index 전후 동일, watcher27개 모두0/direct leader wait 뒤 group 부재27개였다. 별도 SDK/source after·upload·owned cleanup은 실제 success다. adopted children 전체 wait/완전 FD·DSO closure는 주장하지 않으며 outcome의 관련 false 및 retirement_credit0을 유지한다.

helper 실행 step 시작부터 첫 실패까지 약1158.299초로1200초 소진이 아니며, timeout124/resource-stop이 없다. 실제1,101회 admission은 모두 통과했다. 기존 resource budget을 확대하거나 다음 수정의1200초 성공을 추론하지 않는다.

## 직접 원인과 최소 후속 범위

`ui_application_lifecycle_reference.rs::validate_source`는 provenance 및 source-closure 두 루프에서 각각 application.py의 원문38,593B/SHA256 `d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00`만 허용한다. #557의 현재 font staging source는42,048B/SHA256 `c659ce3574276546da11a7bd25e373321b62226411b1eaadda37401fe3b2110d`다. 두 manifest에서 이 파일만 현재와 달랐다(기존 명시된 retired Python2 absent 예외는 별개).

이미 `ui_application_logic_check.rs`에는 정확한 font3구간 길이/hash/출현1/순서와 메모리 역투영 후 old fullSHA를 검증하는 private matcher가 있다. c00 bytes의3구간을 source 데이터로 비교하면 실제 Git 원문 전체 bytes가 복원된다. 이것은 원본/현재 프로그램 실행이나 변경된 기대값 수용이 아니다.

최소 후속안은 기존 matcher를 crate-private로 재사용하고 lifecycle의 두 검사에서 정확한 application 경로 및 기존 d17a expected pin일 때만 같은 source 경계를 적용하는 것이다. 다른 source/font/absence/Params/dependency pin, fixture, 원문5 body, RED/GREEN 및 recording9는 유지해야 한다. 기존 integration target의 새 sibling 회귀에는 현재 source 통과와 font구간 변조·누락·중복·순서/비font영역 변조 거부가 필요하다. runtime mode는 현재 default source-font 범위이며 staged-font env 기능 수용을 의미하지 않는다.

초기 helper/WF/docs 범위에서 source 우회 patch는 하지 않았다. 이후 사용자 승인으로 아래 Rust4파일의 최소 연결 및 회귀를 구현했다. helper/WF의 시간·cap·source/baselineoverlay·원문 expected는 변경하지 않았다. 수정 코드와 신규 회귀의 기능 검증은 Actions 전 UNRUN이다.

## source-only 구현과 회귀 동결

기존 `application_source_matches`의 본문은 변경하지 않고 `pub(crate)`로만 공유했다. lifecycle 내부 `source_matches` closure는 정확 경로 `openpilot/system/ui/lib/application.py` **및** 기존 expected `d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00`가 모두 맞을 때만 그 matcher를 사용한다. provenance·closure 두 루프가 동일 closure를 호출한다. 다른 경로/expected에는 원래 SHA 비교를 적용한다. 새 public API, env override, 새 feature/crate는 없다.

기존 matcher의 original38,593B/fullSHA 허용 및 current42,048B의 정확3구간 길이/hash/출현1/순서/최종 old fullSHA 확인은 전부 불변이다. current c659 fullSHA는 새 회귀 입력 출처 검증에만 있으며 production의 blanket 허용이 아니다. 현재 source/fixture를 검사할 뿐 원본 source를 복구·실행하거나 expected를 재캡처하지 않는다.

기존 integration target `ui_application_lifecycle_reference`에 새 sibling `source_binding` module을 연결했다. 기존 파일은 module 등록2줄 외 전체 byte 동일하므로 원문 five-modes와 기존13개 나머지 body를 유지한다. 새4tests는 실제 public `validate_source`와 private 임시 source copy를 사용하며 프로그램 spawn/skip/fixture expected 재생성이 없다.

- `source_binding::current_font_source_passes_both_frozen_pin_boundaries`: 한쪽 연결만 있으면 전체 validator가 실패한다.
- `source_binding::font_blocks_reject_mutation_missing_duplicate_and_reordering`: 구간별 변조/길이 유지 제거/중복 및 같은 길이 순서 변경을 거부한다.
- `source_binding::non_font_application_drift_is_rejected`: FPS/stack/close 영역의 같은 길이 변조를 거부한다.
- `source_binding::projection_does_not_admit_other_paths_fonts_or_params_providers`: 유효 application bytes를 widget/provenance 또는 font/closure-only 경로에 넣어도 각각 정확한 첫/둘째 루프 오류로 거부한다. 새로운 source-font fallback 및 Params provider도 거부한다. 각 임시 변조를 복구한 뒤 validator의 성공을 다시 확인한다.

## 검증·부모 CI 연결

로컬은 Rust1.94 rustfmt/parse·diff·inert source/JSON/TOML 검토만 수행했다. matcher는 visibility 외 전체 byte 동일, 기존 integration 파일은 module 등록 외 전체 byte 동일, validator 밖 runtime 및 기존 fixture/helper/WF 불변을 확인했다. package lock의3개 graphics dependency record/기존 fallback 부재/Params layout은 고정 자료와 일치한다. 신규4tests와 수정 후 original5·RED/GREEN/recording9·Clippy는 로컬 실행하지 않았고 current-source CI 성공을 아직 주장하지 않는다.

부모가 현재 fresh candidate-reference ELF의 기존 owned wrapper 안에 아래 한 검사를 등록하면 된다. 기존 five-modes/RED/GREEN/recording9/Clippy 실행은 보존한다. expected는4passed/0failed/0ignored 및 summary1개이며 0선택을 성공으로 인정하지 않는다. 시간은 같은 shared1200초에서 차감하고 caps/원문 expected를 바꾸지 않는다. helper/WF/pins/원장/Cargo는 부모 소유이며 여기서 편집하지 않았다.

```bash
"$proof/elfs/candidate-reference" source_binding:: --nocapture --test-threads=1 --color=never
```

해당 ELF의 기존 build target은 `cargo test --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-dev-tools --features ui-application-lifecycle-native --test ui_application_lifecycle_reference --no-run --message-format=json` 그대로다. 필요 시 기존 `transitive_sources_fonts_absence_and_dependency_records_reject_drift` exact selector로 전체32 source/font/fallback/dependency 경계도 검증할 수 있다. 이 추가 검사는 원문5개를 대체하지 않는다.

| 변경 파일 | SHA256 |
| --- | --- |
| `rust/crates/dev-tools/src/ui_application_logic_check.rs` | `672942f302fa7538cbc57d20cb61ba146aeedc4e8a92397061891e933542b9e6` |
| `rust/crates/dev-tools/src/ui_application_lifecycle_reference.rs` | `910d83f44909352ace57d1bf1c59f7e3da00a1878106448eee4a7fa712b2c8ae` |
| `rust/crates/dev-tools/tests/ui_application_lifecycle_reference.rs` | `cd6233cbe31283b47f3d8a4d6dac9c4a11b60739ec5fa54d25cd23630473c458` |
| `rust/crates/dev-tools/tests/ui_application_lifecycle_reference/source_binding.rs` | `9aa47cb3cc2403e24d13d4faf1ae0b9155d980a8b6696f72931432d321a60ce6` |

실제 수용은 새 exact-source Actions 결과가 필요하다. c00의 부분 성공을 이 수정 source에 소급하지 않으며, old source pin/fixture/원본5·RED/GREEN9·1200초·성장 cap·retirement_credit0은 유지한다.

상세 inert 원문은 ignored `.analysis/scratch/2026-10-24-python-rust-523/hosted-c00-ui-12584138814-audit.md`와 동일 prefix `.audit/`에 있다. 감사 raw log/API/전수 member SHA/CRC/outcome/실패 stderr/시간·seal 비교를 함께 보존했다.


부모 CI 연결 후보는 같은 fresh candidate-reference ELF에서 `source_binding::`의4pass/0fail/0ignored를 명시 확인한다. outcome의source_binding4와기존pass조건,WF최종closure에도같이묶었다. 기존5원문mode/RED101/GREEN9/Clippy/1200초·cap은유지하며새controls도기존총예산에포함한다. 기능은아직UNRUN이다.


## 01f 실제 shared deadline과 독립 cache copy 후보

[01f run40157685695/job121062462927](https://github.com/bin9208/openpilot-rust/actions/runs/40157685695/job/121062462927), source `01fcc36e09b39172f4c88954a702fa8353b2d1e8`은 clippy phase의 바깥 timeout124로 실패했다. watcher0/finalizer0이며 source controls4, exact RED101/GREEN1, recording9는 실제 통과했다. original five modes는 미시작이다. Clippy stderr의 Finished와 Cargo exit0이 있어도 전체 command124를 성공으로 재기록하지 않는다. 이전 c00의 source pin 실패와 이번 shared1200 소진을 구분한다.

[artifact12585589103](https://github.com/bin9208/openpilot-rust/actions/runs/40157685695/artifacts/12585589103)은40,902,107B/SHA256 `89b99b56f4817e1318d16f56ca8f49792bc4a4f90bd7eeaeef1daa8e78e3dbc7`, 전수25,281CRC/expanded206,767,504B 정상이다. source/SDK/baseline/index 등55개 비교와 held372paths의 identity/SHA가 동일했다. 27phase watcher0/직접 wait 뒤 group 부재27개, source/SDK after·upload·cleanup도 실제 success였다. 이 부분 성공은 #559 완료·현재 Clippy·five-modes 수용이 아니다. 상세 원문/계획은 ignored `hosted-01f-ui-12585589103-audit.md`와 같은 prefix `.audit/`에 보존했다.

baseline/current recording/reference build는 각각264.515/251.917/251.796초였다. 같은 features인 첫 두 build에서 registry artifact records232개 중 fresh=false가232/223개였다. 이는 고유 package/실제 compiler 호출 수가 아니라 JSONL record 수다. reference는 dep:zmq도 켜므로 추가 의존 전체를 불필요한 중복으로 간주하지 않는다. Cargo/test/shared source를 축소하지 않고, 승인된 동일 job cache 복사만 후보로 추가한다.

### 복사와 입장 검증

기존 helper `tools/ci/ui_recording_559.sh`, WF의 helper literal pin, 이 기록3파일만 바꾼다. 기존 Cargo/prod/tests/fixture/expected/원문5 body/RED-GREEN/recording9/source4/Clippy 명령은 유지한다. 원래 baseline 취소 overlay1파일 및38,593B/d17a application source 경계에도 변경이 없다. 새로운 cache mode/옵션/외부 controller/워크플로우는 추가하지 않는다.

1. image disk guard와 standalone image SDK stager 두 준비 도구의 target만 같은 build8GiB 안 `bootstrap-target`으로 옮긴다. 따라서 baseline actual RED 및 source 봉인 후에도 `candidate-target`은 부재해야 한다. 다른 job/과거 artifact/cache restore는 쓰지 않는다.
2. baseline/current Cargo workspace·dev-tools manifest 및 Cargo.lock bytes를 비교·hold한다. actual SDK manifest를 hold/재검증하고, 두 build의 명시 env/flags를 NUL 원문으로 비교한다. CARGO_TARGET_DIR만 baseline/current로 각각 고정된다. 첫 current build 전후 registry source 전체 bytes/roster를 비교해 복사 재사용 구간의 input을 고정한다. 뒤의 reference가 새 features의 registry dependencies를 정상 준비하는 범위까지 과거 snapshot으로 동결했다고 주장하지 않는다.
3. baseline target은 별도 finalizer tree로 등록하여 copy·후속 build 실패에서도 전후 identity/roster/SHA를 검사한다. owned snapshot은 symlink/특수파일/다른 UID/공유 쓰기를 거부한다. absent candidate에 `cp -R --reflink=auto --preserve=mode,timestamps`로 실제 COPY한다. -a/--link/--preserve=links와 산출물 삭제·무효화·source 복원은 없다.
4. source target의 copy 전후 identity roster/SHA가 같아야 한다. candidate의 상대경로·type·mode·UID/GID·file size·mtime payload와 모든 regular-file SHA는 source와 같아야 하며, source/candidate의 전체 dev:inode 교집합은0, candidate regular-file nlink는 모두1이어야 한다. CoW/reflink는 inode가 독립일 때 허용한다. 공유 hardlink/외부 alias·symlink fallback은 없다.
5. 복사 직후 **첫 current recording-build 전체 Cargo JSONL**을 eager parse한다. EOF success1개와 compiler artifacts 존재를 확인하고 모든 non-registry artifact가 current rust root의 path package/manifest/src 및 fresh=false여야 한다. 하나의 baseline URI/unknown source 종류/fresh=true도 실패다. `cache-copy/current-workspace.json`은 `phase=recording-build`, `scope=first_current_cargo_after_copy`, 검증 record 수와 all_fresh_false를 명시한다.
6. 이후 reference/stager에서는 같은 job의 검증된 current workspace dependency cache 재사용을 허용한다. 첫 copy 입장 조건을 매 후속 build의 모든 dependency에 반복 적용하지 않는다. 대신 새로 **선택하는** reference test/CLI의 exact current package/manifest/path/kind/profile/features/non-null executable/unique record 및 fresh=false를 유지한다. 선택된 ELF는 정상 current Cargo 명령 이후 proof에 복사·hold한 것만 실행한다. copied baseline ELF를 helper가 직접 선택·실행하지 않는다.

### 실제 rustc 관측과 시간·실패 경계

현재 recording/reference/stager 세 compiler phase에만 기존 process trace와 함께 chdir/fchdir를 관측한다. 기능 phase의 trace는 그대로다. 첫 recording의 dev-tools lib/test, reference test, stager CLI 총4개에 대해 pinned SDK rustc의 정확한 crate/source 인자, 성공 exec, 그 시점의 명시 current source/rust cwd, current candidate-target/debug/deps out-dir와 link emit, 성공 exit_group0을 요구한다. fchdir/다른 chdir는 cwd 근거를 무효화하며, inherited cwd 추정·baseline cwd·누락·중복·truncated/resumed exec를 성공으로 만들지 않는다. 원문은 phase별 trace.PID와 `cache-copy/<phase>-<crate>.rustc`에서 연결하고 FD/hash를 hold한다. 이는 compiler source 관측이며 전체 syscall/FD/DSO 폐쇄 주장이 아니다.

복사/snapshot도 기존 env-i/owned supervisor/timeout/watcher/direct wait/tracees non-live/실패 proof 경계에서 실행한다. source 전후 및 copy snapshot은 각각60초, copy는120초 **이하**이고 전부 기존 shared1200의 remaining으로 더 줄어든다. 추가1200이나 budget 밖 준비를 만들지 않는다. copy 전후 build physical/apparent bytes를 기록하고 copy 진행에도 기존 build8GiB/proof1GiB/file512MiB 및 filesystem reserve/admission을 유지한다. reflink 미지원 실제 bytes 복사도 같은 cap에서 판정한다.

outcome의 `independent_cache_copy`, `seeded_current_workspace_fresh`는 초기 false이며 검증 후에만 true다. 기존 pass 조건에 두 값이 모두 추가된다. copy/input/freshness/관측 실패는 원래 source/SDK/baseline/provider 봉인·upload를 계속 거친 실패이며 cold fallback/retry/옛 수용 재사용으로 숨기지 않는다. WF는 helper literal SHA만 바꾸며 원래 qualification/source·SDK final/closure/upload/cleanup 조건은 그대로다.

### 검증 상태와 남은 실제 조건

로컬은 source/인자/diff, Bash·고정 snapshot/copy 본문의 문법 및 YAML만 검토한다. cache copy/Cargo freshness/새 cwd 관측/추가 proof·8GiB fit/1200초 절약량/최종 five modes는 전부 Actions UNRUN이다. baseline target에 symlink·특수파일이 있거나 Cargo가 copied path artifact를 fresh로 사용하거나, 필수 cwd/exec를 관측하지 못하면 실패한다. 이를 자동 완화하지 않는다. SDK·Cargo·features·selector·기대 bytes·cap/timeout을 바꾸거나 #559 해결을 선언하지 않는다. 부모 독립 source review와 exact source GHA가 필요하다.

## 772 실제 CXX alias6 거부와 제한된 seed 제외 후보

[772 run40160682062/job121071387390](https://github.com/bin9208/openpilot-rust/actions/runs/40160682062/job/121071387390), source `7720ce770a07cce7c39d29113d7eb88f2330164d`는 baseline build와 정확한 RED101 뒤 `cache-source-before`에서 exit1/watcher0으로 실패했다. 실제 helper는 `db6f3252ae29c497a568ec318a2b1b31f2b40c2dd9d443c8273337058b301438`였다. job log558행의 원인은 snapshot이 정상 CXX symlink6개와 그 mode777을 regular/dir 입장 조건으로 거부한 것이다. shared1200 deadline이나 current compiler freshness/cwd 실패가 아니다. copy와 current build에는 도달하지 않았다.

[artifact12586947032](https://github.com/bin9208/openpilot-rust/actions/runs/40160682062/artifacts/12586947032)는23,044,524B/SHA256 `66c99af66873385236aefff0f522344b17c42e30b3f6425d009030324ba0f10e`, 전수12,356CRC/expanded131,262,104B 정상이다. source/index/SDK/baseline/provider 비교38쌍과 held378paths의 identity/SHA가 같았다. finalizer0/immutable_seals=true/owned_processes_nonlive=true 및 source·SDK after/upload/cleanup success는 부분 증거이며 GREEN/recording9/source4/five-modes/Clippy 수용이 아니다. 상세 API/log/member SHA/봉인은 ignored `hosted-772-ui-12586947032-audit.md`와 `hosted-772-ui-40160682062/`에 있다.

### 실제 Cargo OUT_DIR와 고정 registry에 연결

원문 `ui559/logs/baseline-build.stdout`는334,249B/SHA256 `f8697c2156cf426db46107cd293863503041a4a33d77fdecf420d349b0fe84a4`,409 JSONL records다. exact baseline JPEG package의 custom-build artifact와 같은 package의 build-script-executed record가 `debug/build/openpilot-jpeg-6a49348a96bf129c/out`을 연결한다. registry cxx1.0.202의 lib artifact는 같은 job의 cargo-home 아래 manifest/src를 가리킨다. `profile.test=false`이며 target 자체의 test=true와 구분한다. 세 원문 record는 ignored `hosted-772-ui-40160682062/baseline-cxx-provenance.json`에 보존했다.

후속 helper는 각 baseline/current Cargo JSONL 전체와 EOF success1개를 읽고, JPEG custom-build package/manifest/src/kind/name/fresh=false 및 유일한 OUT_DIR, cxx1.0.202 lib record를 선택한다. header는 그 registry manifest 옆 `include/cxx.h`로만 정한다. baseline/current source의 JPEG crate와 target의 OUT_DIR, 같은 sealed registry가 아래6관계의 유일한 근거다. build hash leaf는 actual OUT_DIR에서 얻으며 임의의 링크를 glob으로 허용하지 않는다.

`T`는 해당 baseline 또는 candidate target, `J=T/debug/build/openpilot-jpeg-<actual16hex>/out/cxxbridge`, `S`는 해당 source의 `rust/crates/jpeg`, `H`는 same-job registry의 `cxx-1.0.202/include/cxx.h`다.

| 정확한 link | 정확한 raw absolute target |
| --- | --- |
| `T/cxxbridge/openpilot-jpeg/src/bridge.rs.cc` | `J/sources/openpilot-jpeg/src/bridge.rs.cc` |
| `T/cxxbridge/openpilot-jpeg/src/bridge.rs.h` | `J/include/openpilot-jpeg/src/bridge.rs.h` |
| `T/cxxbridge/rust/cxx.h` | `H` |
| `J/include/openpilot-jpeg/src/bridge.rs` | `J/include/openpilot-jpeg/src/bridge.rs.h` |
| `J/include/rust/cxx.h` | `H` |
| `J/crate/openpilot-jpeg` | `S` |

5개 file target과1개 directory target의 종류/소유 UID/쓰기 mode/canonical path를 확인한다. link 자체의 lstat identity·nlink1·raw readlink와 referent FD identity/SHA를 기록하며, parent chain의 alias·broken/cycle/outside target·추가/누락 link를 거부한다. source/header는 원래 source·registry 봉인과 연결하고 metadata JSON/header/source manifest/build.rs도 hold한다. generated current files는 후속 정상 Cargo 입력이므로 영구 불변 FD 파일로 거짓 분류하지 않는다.

### baseline 무변경과 독립 복사 범위

이6개는 baseline에서 지우거나 고치지 않는다. full baseline roster에는6개 link와 raw target을 계속 포함한다. copy 전후 full roster·regular SHA·alias 영수증·exact6 목록·seed 목록이 같아야 하고, 기존 finalizer도 baseline source/index/target 전체를 다시 검사한다. seed에는 검증한 regular/dir만 NUL 목록으로 넣는다. 위6개 link 외에 비교에서 제외하는 파일/디렉터리는 없다.

복사는 같은 env-i/owned run 안에서 GNU tar의 `--no-recursion --null --verbatim-files-from`으로 이 목록만 읽어 absent candidate root에 pipe로 푼다. `--hard-dereference`는 hardlink member 대신 각 regular bytes를 독립 저장하는 용도이며, soft symlink를 따라가는 `-h/--dereference`는 쓰지 않는다. `--format=posix`, `--same-owner --same-permissions --delay-directory-restore`로 파일과 디렉터리의 기존 mode/UID/GID/mtime 비교를 보존한다. 디렉터리 timestamp·권한의 추출 종료 시 복원은 [GNU tar 공식 설명](https://www.gnu.org/software/tar/manual/html_node/Directory-Modification-Times-and-Permissions.html), hardlink별 독립 member 의미는 [GNU tar hard links 설명](https://www.gnu.org/software/tar/manual/html_section/Portability.html)에 근거한다.

copy 직후 candidate에는 symlink0, regular nlink1, source/candidate regular·dir inode 교집합0이 필수다. 모든 seed 상대경로/type/mode/UID/GID/file size/mtime payload와 모든 regular SHA가 baseline과 같아야 한다. ctime/inode 자체는 독립 copy 때문에 달라지는 필드이며, baseline 쪽 전후에는 기존 identity/ctime까지 계속 비교한다. registry/header 및 source JPEG directory를 symlink dereference로 candidate에 복제하지 않는다.

첫 current recording-build의 전체 non-registry/path artifact fresh=false와 exact current test/CLI·actual rustc source/cwd 검사는 원래 그대로다. 그 검증 뒤 candidate OUT_DIR/current JPEG source/same registry에 정확6 link가 재생성됐는지 같은 worker로 검사한다. baseline source/target을 가리키는 잔존 link는 실패한다. 이후 같은 job의 current dependency cache 재사용은 기존 승인 범위이며, 첫 독립 copy의 nlink1 조건을 후속 Cargo가 만든 정상 내부 hardlink까지 무조건 확장하지 않는다.

### Actions 거부 통제와 미수용 조건

같은 snapshot worker에 private 구조 fixture를 주어 exact6 양성1개, extra-alias/wrong-target/broken-target/cycle-target/outside-target 거부5개를 실행하고 각 변조를 되돌린 뒤 다시 양성 입장을 요구한다. fixture는 Cargo/ELF/원문 expected 대체물이 아니다. 정확한 exit0/1과6개 markers를 요구하며, 다른 오류코드·원복 후 입장 실패·0선택을 수용하지 않는다. 새 `ln` provider도 기존 ELF/DSO FD/hash 봉인에 포함한다.

snapshot60초/copy120초/control30초/current alias60초 모두 기존 shared1200 remaining 안에서만 실행한다. build8GiB/proof1GiB/file512MiB·실제 FS admission·성장 감시·source/SDK final·upload/cleanup은 그대로다. tar COPY는 CoW 절감에 기대지 않으므로 physical/apparent fit은 실제 GHA에서 판정한다. baseline copied workspace를 Cargo가 fresh=true로 재사용하면 여전히 실패하며, 자동 fallback/산출물 삭제/Cargo clean/expected 재생성은 없다.

이 후보는 helper/WF helper literal pin/이 한국어 기록3파일만 수정한다. 로컬은 Bash·고정 본문 문법/YAML/소스·diff만 확인했으며 새 controls/copy/metadata worker/Cargo freshness/alias 재생성·fit·원문5/RED-GREEN9/source4/Clippy는 **UNRUN**이다. 기존 Cargo/prod/test/fixture/expected와1200·8GiB를 바꾸지 않았고 #559 또는 runtime 전체 수용을 주장하지 않는다.


## 54b actual jq 구문 실패와 최소 정정

[40164031999/job121081294345](https://github.com/bin9208/openpilot-rust/actions/runs/40164031999/job/121081294345),
source54b7b8d32의 최초 실패는 baseline RED 뒤 `cache_alias_metadata` jq compile이다.
`header:(...)+"/include/cxx.h"` object value의 합 전체를 괄호로 묶지 않아 unexpected '+'를 냈다.
baseline_exact_red=true이며 aliascontrols/cachecopy/currentfresh/sourcebinding/recording9/5mode는 미실행이다.
후보는 해당 식을 `header:((...)+"/include/cxx.h")`로만 정정하고 WF의 helper literal SHA를 갱신한다.
원문 expected/실제 alias6 대상·negative5/첫 current freshness/시간·cap·원본5mode는 그대로다.

artifact12587279330=22955599B/SHA256
`c8dc6729c54fb0863bc1c0b3ead14baf12d59aa6fae75562702370fe57a00236`,12348CRC/expanded131572606B를
부모가 감사했다. finalizer0/immutable_seals=true/owned_processes_nonlive=true와 source/index/SDK
봉인은 별도 성공이다. outcome phase는 baseline-red/command_exit3이며 pass 및 retirement_credit은false/0이다.
local Bash/YAML와 새 jq 식의 syntax compilation만 확인하며 다음 실제 positive·negative/cachecopy 수용은
Actions 대기다. compile문법 오류를 cache/source/timeout 수치 결함으로 오인하지 않는다.


## #568 첫 current workspace cache 지문 거부


[a483 UI40166180677/job121087586932](https://github.com/bin9208/openpilot-rust/actions/runs/40166180677/job/121087586932), source `a4838fc4e7d09499d1dbc50045ec049af995ffbc`.
artifact12588248598/27490467B/SHA256 `3ecee0c086f20557f3fed7e4207d6d85bc9e9e02c73b787f40e5af5d7341064d`의15798CRC/expanded163208313B를 부모가 감사했다.

baseline exact RED, CXX exact6 aliases·음성5와 독립 cache copy는 통과했다. current recording Cargo build도 exit0/완전한 build-finished:true였지만, non-registry144개 artifact 중12개가 fresh:true라 기존 first-current-workspace guard가 정확히 거부했다. vendor ureq/ureq-proto 및 Params/JPEG/image-codecs build scripts, runtime-core/log-format/http-transport 등이다. 기능 PASS나 현재 source 입증으로 바꾸지 않는다. baseline의12 non-registry package명과191 registry package명은 중복이 없다.

원인은 baseline target 전체를 독립 복사한 뒤 source 위치 변경만으로 모든 workspace 단위가 dirty가 될 것으로 가정한 데 있다. Cargo는 내용이 같은 일부 단위의 fingerprint를 재사용했다. guard를 완화하거나 fresh:true를 수용하지 않는다.

후보는 정확 baseline Cargo JSON으로 registry/non-registry package명을 분류·봉인하고, 독립 복사 검증을 마친 current 전용 target에서 non-registry의 fingerprint 디렉터리만 제거한다. root/canonical/UID/inode와 이름/해시를 확인하고 삭제 journal을 보존한다. 모호한 package명·registry 중복·누락·알 수 없는 fingerprint는 거부한다. baseline/registry/SDK는 수정하지 않으며 첫 current 전체 fresh:false와 기존 rustc cwd/선택 ELF 검증을 유지한다. build8GiB/proof1GiB·shared1200초·원문/기능은 그대로다.

완료 조건: 실제 첫 current의 전체 non-registry fresh:false 및 선택 source/ELF 경계, 독립 복사와 baseline/registry/source/SDK 봉인 확인. 이후 원문5mode·recording9/Clippy 등 전체 UI 수용은 #559에서 계속 별도로 추적한다. 현재 source syntax만 확인했으며 기능/기존 성공 승격/원본 삭제·credit0이다.

이 후속 결함은 https://github.com/bin9208/openpilot-rust/issues/568 에 연결한다.

## e856 #568 실제 CMake OUT_DIR 재사용 실패와 고정3 출력 정리 후보

[e856 run40168818381/job121095302862](https://github.com/bin9208/openpilot-rust/actions/runs/40168818381/job/121095302862)은
workspace fingerprint147개 정리 뒤 첫 current `recording-build`에서 Cargo101로 실패했다.
기존 [artifact12589147465](https://github.com/bin9208/openpilot-rust/actions/runs/40168818381/artifacts/12589147465)
ZIP25,464,633B/SHA256 `e27871c4423b9a406c9241233c0e2c2afdbe0ef9995d87ed0fda83cd0bf0cbc1`/15,192CRC는
부모 감사 자료를 재사용했다. 추가 다운로드·전체 CRC 반복 없이 `ui559/logs/recording-build.stderr`3515B와
baseline Cargo JSONL만 inert로 읽었다. 로컬 프로젝트/원본/컴파일/CMake 실행0이다.

직접 오류는 candidate의 JPEG `out/jpeg/CMakeCache.txt`가 생성 당시의 baseline target 경로를
기억하고 있고, source CMakeLists도 baseline checkout의 절대경로인 것이다. CMake는 현재
candidate-target/current source 경로와 두 값이 달라 `Re-run cmake with a different source directory`로
거부했다. fingerprint만 지워서 build script가 다시 실행돼도 기존 OUT_DIR 내용은 남는다.
이 실패를 current-fresh guard 완화, CMakeCache 문자열 치환이나 기능 성공으로 처리하지 않는다.

baseline JSONL의 non-registry `build-script-executed`는 정확3개이며 각각 같은 package의 유일한
`compiler-artifact/custom-build`와 짝을 이룬다. `B=$job/build/baseline-target`일 때 실제 값은 다음과 같다.

| 고정 path package / version | crate 경로 | 실제 baseline OUT_DIR |
| --- | --- | --- |
| `openpilot-params@0.1.0` | `rust/crates/params` | `B/debug/build/openpilot-params-963dcccc3d695b58/out` |
| `openpilot-image-codecs@0.1.0` | `rust/crates/image-codecs` | `B/debug/build/openpilot-image-codecs-12d1075497bdecd9/out` |
| `openpilot-jpeg@0.1.0` | `rust/crates/jpeg` | `B/debug/build/openpilot-jpeg-6a49348a96bf129c/out` |

이16hex는 위 actual 관측값이다. 새 source 후보는 임의 과거 hash를 고정하거나 glob 전체를 지우지 않고,
**이번 같은 job의 baseline 성공 JSONL**에서 exact path package ID/manifest/build.rs/custom-build/fresh=false/
유일 OUT_DIR3개를 받아 고정 package명과 `debug/build/<name>-16hex/out` suffix를 확인한다.
unknown/missing/duplicate/non-registry 추가 script는 실패다. registry 출력은 선택하지 않는다.

기존 `prune_seeded_workspace_fingerprints()`에 bounded `workspace-build-outputs.json` plan을 추가했다.
기존60초 owned prune 명령 안에서 workspace·registry package명 disjoint 확인과147 fingerprint 정리를
유지하고, baseline OUT_DIR에 대응하는 **candidate의 package parent3개만** 정리한다.
빌드 script 실행파일의 다른 hash parent나 `debug/build` 전체를 삭제하지 않는다.

각 source/target parent는 canonical/nonsymlink·현재 UID·같은 job device여야 한다.
baseline/candidate package parent의 inode는 달라야 하고, candidate dev/ino/UID는 입장 때 저장해 삭제 직전에
재확인한다. 내부 node는 owned/same-device regular nlink1 또는 directory만 허용하므로 alias/외부 link를
따라 지우지 않는다. plan64KiB, 후보 node metadata는 package당1MiB로 한정한다.
삭제 전 `planned`와 실제 부재 확인 후 `removed`를 `workspace-build-pruned.tsv`에 보존한다.
`workspace-build.before/expected/after.bin`은 정확3개만 제거된1-depth roster와 나머지 entry identity 불변을
대조한다. 이 자료와 plan/node metadata는 성공 후 기존 held 목록에 넣으며 실패 시에도 생성된 원문은 남긴다.

copy/seed 단계의 exact6 symlink 제외·regular bytes 일치·inode 독립·baseline 전체 전후 봉인은 그대로다.
새 정리는 **독립 copy 입장 검증 후의 명시적 current output 무효화**이며 seed를 몰래 더 제외한 것이 아니다.
registry build cache/source, baseline source/target, SDK에는 쓰지 않는다.
첫 current artifact 전체 fresh=false, 실제 rustc cwd/current source·선택 ELF 검증과 기존 CMake build가
이 후보의 regression gate다. 구현을 복사한 의미 없는 synthetic test는 추가하지 않는다.
원래 RED/GREEN/recording9/source4/원문5 modes/Clippy·shared1200초·8GiB build/1GiB proof/512MiB file도
바꾸지 않았다. 추가 작업의60초 fit과 실제 CMake/current 전체 성공은 **Actions UNRUN**이다.
shared WF helper literal은 부모가 갱신하며, logging frozen5 source는 편집하지 않는다.


## 3393의 current 재빌드 성공과 rustc emit 판정 정정 후보

[40171142693/job121102032148](https://github.com/bin9208/openpilot-rust/actions/runs/40171142693/job/121102032148),
source `3393c2ee9c8a923ca94f46951e8959bb382dac7b`는 current recording Cargo build를 마쳤다.
Cargo JSONL EOF는 success=true이고 non-registry144개 모두 fresh=false다. 이전 fingerprint 및
정확3개 OUT_DIR 정리 뒤 CMake 경로 오류는 나타나지 않았다. 그 다음 rustc cwd/exec 검사에서
멈춰 전체 GREEN/recording9/원본5/Clippy는 미수용이다. artifact12590786738은28,692,076B/
SHA256 `db108902133019543637f97e43fe5e3416f20a8bb56b38e6175f4a08d5a1ce9f`/18,107CRC,
전개164,756,398B이며 전체 CRC와 해당 실제 JSON/trace를 확인했다.

원문 `ui559/trace/recording-build/trace.90944:2`는 정확 current cwd와 SDK rustc로
`openpilot_dev_tools` library를 컴파일하며 `--emit=dep-info,metadata,link`를 사용하고 exit_group0이다.
기존 감사는 test/bin용 `--emit=dep-info,link`만 기대해서 정상 library 명령을 거부했다.
fixture의 trace.96187은 원래 기대대로 test/dep-info,link다.

후속은 알려진4개 crate/source 조합 중 library1개에만 metadata 포함 emit을 요구하고,
기존 test2/bin1은 원래 emit을 유지한다. 미등록 조합은 거부한다. current cwd·정확 compiler/소스·
출력 root·유일 exec·정상 종료와 기존 Cargo freshfalse/ELF 기준은 그대로다. 새로운 전체 기능
성공은 Actions에서 확인하며 source/SDK 최종 봉인·관측 process 부재 성공과 구분한다.


## 3c1 cache/source/ELF 수용과 남은 UI 검증

source `3c1e70b689b3d017a352c06f2873f066fe4f3907`의
[40172405889/job121105837855](https://github.com/bin9208/openpilot-rust/actions/runs/40172405889/job/121105837855)은
첫 current144개 모두 fresh=false, exact rustc receipt4개·선택 ELF, baseline RED/candidate GREEN/
recording9/source binding4를 통과했다. artifact12591287332는44,846,602B/
SHA256 `101fdbba04b5ad01b4b8ff9f4d6ca39f18d3c813ef06deec1596db8ab407552c`/24,982CRC,
전개233,003,619B다. source/index/SDK5쌍·baseline/registry·독립 copy/source copy 전후 등
raw20쌍과 비공유 inode·남은 build roster를 직접 대조해 동일함을 확인했다.
최종 immutable_seals/owned_processes_nonlive=true와 original/full pass=false를 구분한다.

#568의 cache/source/ELF·봉인 완료 조건은 충족해 개별 종료한다. 전체 job은 Clippy137/watcher0으로
실패했고 원본5mode는 미도달이다. Clippy는 전역20분의 남은66.8초만 사용하고 종료됐다.
원래600초 Clippy 명령과 원문5mode300초 자체는 바뀌지 않았다. 전체 #559 수용/퇴역 credit은0이다.


## 기능/원문 비교와 strict Clippy의 수명 분리 후보

3c1의 전체20분 예산이 Clippy를66.8초에서 자른 실제 결과에 따라 단계 수명을 분리한다.
기존 기능·원문 검사의 순서를 유지하며 원본5mode를 recording9 다음에 먼저 실행한다.
빌드·baseline/독립 cache/current/source4/GREEN/recording9/원본5 전체는 기존1200초 안에
완료해야 하고 그 실제 elapsed/완료 항목을 functional-window.json에 기록·봉인한다.

그 다음 기존과 같은4target/feature/명령·환경의 strict Clippy에 기존600초를 별도로 배정한다.
graphics용 환경 추가분은 저장한 이전 extra_env로 되돌려 Clippy 환경도 같게 유지한다.
같은512MiB 추가 build cap·8GiB build/1GiB proof·모든 watcher/회수·원문 기대값과 개별 기능 제한은
그대로다. Clippy-window에도 실제600초 이내 완료를 요구하고 기록·봉인한다.
전체 PASS는 두 단계와 기존 최종 봉인/프로세스 부재가 모두 성공해야만 된다.

전체 helper outer는25분에서35분으로 바꾸고 기존180초 kill-after와45분 job은 유지한다.
이는 기능 기준을 늘리는 변경이 아니라 준비·기능1200초와 별도 정적600초·후검사를 담는
운영 예산 변경이다. 기존 전체 shared1200초는 이 source부터 두 명시 수명으로 대체됐으며,
이전 실행을 새 경계의 PASS로 다시 표기하지 않는다. 실제 원본5·Clippy/최종 전체 수용은 GHA 대기다.


### #569 기존 partial-write RED에 현재 pending-close API가 섞인 컴파일 실패

[PR343 bacda40173790842/job121111148697](https://github.com/bin9208/openpilot-rust/actions/runs/40173790842/job/121111148697)의
UI/connectivity step33은 현재 test9 전체를 과거11f3dec adapter에 복사해 E0432
`pending_close_native::pending_close_control` unresolved import로 실패했다. 의도한 RED assertion에
도달하지 않았으므로 기존 partial-write 수용이나 #559 기능 실패로 세지 않는다. #569로 독립 추적한다.

기준 adapter11f3dec과 exact short-reading selector/assertion은 유지한다. test 입력만
`ca8fd1119ca16aaefe0f99676d5b3ed7d5874df2`의 기존8개를 immutable git show로 복사한다.
18569B/SHA `5b26f9e5ba3db68693b00532930fd7ee509122f18372a1c6d308086cc855cddc`를 확인하고
현재test의18569byte prefix와 동일함을 proof로 남긴다. 현재 test9 전체는 기존 후속 Cargo 명령에서
계속 실행한다. 과거 adapter의 독립 target·용량 입장·실제 RED 실패문구·현재 GREEN을 유지한다.
main workflow는 해당 run1곳만 바꾸고 다른43job/native8집계/원문5mode·pendingclose9 경계를 보존한다.
로컬 source/문법/diff만 확인했으며 새 RED/GREEN과 전체 UI는 Actions UNRUN이다.


### #559의 실제 수용 완료: c761 기능 전체와 Clippy 성공

[c76140175214685/job121114297363](https://github.com/bin9208/openpilot-rust/actions/runs/40175214685/job/121114297363)은
SUCCESS다. write ACK와 queue2를 고정한 과거 detach overlay는101/actual514560 vs expected1543680으로
RED, 현재 동일 selector는 GREEN이다. 기존 recording9/source-binding4/원본5모드의 실제9-render/3-frame
비교 및 기존4target strict Clippy가 같은 source에서 통과했다. public detach·30/10/5초·queue/FPS/
write_all·원문 기대값은 그대로다. 기능955/1200초와 별도 Clippy63/600초, finalizer0·immutable seals/
owned nonlive/pass=true를 확인했다. 앞 절의 운영 예산 변경은 명시적으로 유지하며 과거 실행을 재도장하지 않는다.

artifact12592830565는44755004B/SHA
`c22b619d73a8d58a9224379d92e54fb3a0987beda6005c19ec283819d9f16259`,25166CRC/전개233536665B다.
source/index/SDK·baseline/registry·image SDK/renderer/header/compiler raw36쌍, held421개 전체 identity/SHA
행(행 순서만 다름), 정확 current source6핀, 명령35개와 group35 nonlive를 대조했다. 첫 current의
non-registry144개는 모두 fresh:false다. upload/소유 cleanup도 성공해 #559를 개별 종료했다.
제품 UI 전체/#523/#296/필수 PR·실기·원본 제거 credit은 계속 미완료/0이다. adopted 자손 전체 wait나
closed FD/DSO는 주장하지 않는다. #569 partial-write RED의 별도 compile 연결 결함은 계속 열린다.
로컬 프로젝트·원본·모델·ELF/빌드/테스트 실행0이며 ignored ui-c761-559-acceptance-audit.json에 원문 근거를 둔다.


### #575 Forms validator에 빠진 #557 application source 경계 연결

PR343 `9ae67`/merge `75d0cfd49912b631946721e58979487935372828`,
[run40187340482](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482)의
UI/connectivity 원문에서 short-reading RED0/1은 의도대로였고 current recording9도 통과했다.
이어 `ui_forms_admission`은7PASS/3FAIL이었으며 실제 첫 오류는
`openpilot/system/ui/lib/application.py changed; independently recapture Forms`다.
기존 #569의 과거 adapter/test 연결 실패와 분리한 #575다. 새 다운로드·원본 capture 재생성은 없다.

Forms manifest는 원래 application SHA
`d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00`을 유지한다.
#557 current source `c659ce3574276546da11a7bd25e373321b62226411b1eaadda37401fe3b2110d`는
기존 lifecycle에서 사용한 `ui_application_logic_check::application_source_matches`의3구간
역투영 경계로 검사한다. Forms public `validate_source`도 **그 정확 path와 old expected pin의 쌍**에만
같은 함수를 호출하도록 연결했다. 다른 path/pin은 원래 full SHA 비교이며 manifest 전체 hash,
retired source·Params·font 부재·lock dependency 검사와 에러는 그대로다. shared matcher 본문과
production `application.py`는 수정하지 않았고 current whole SHA를 production 허용 목록에 추가하지 않았다.

`tests/ui_forms_admission.rs`에는 child module 등록2줄만 추가했다. 새 `source_binding`4개는
current 양성, font3구간 각각의 변조/같은길이 제거/중복과 순서 변경, 비font FPS/stack/encoder 변조,
같은 current bytes의 다른 path/expected pin·font fallback·Params provider 거부를 검사한다.
변조 전 양성과 원복 후 양성을 요구해 모든 입력을 거부하는 퇴행을 통제 성공으로 세지 않는다.
current whole SHA는 이 테스트 입력의 출처 pin으로만 사용한다.

기존 root6/support4 총10개 테스트 본문,65개 원본 case/44개 성공 raster의 상태·PNG·env 기대값과
manifest/원본 bytes는 그대로다. 새 child는 filesystem source admission만 검사하며 Python/graphics/
CLI/원본 프로그램을 실행하지 않는다. 로컬 rustfmt·source projection·diff만 확인했고,
새4·기존10/실제 Forms CLI·원본 비교와 source/SDK/provider 봉인은 **GHA UNRUN**이다.
이 source 연결만으로 #575 해결·staged-font 기능·#557/#523/원본 퇴역 수용을 주장하지 않는다.
WF/Focus/ledger 등록은 부모가 담당한다.

### #585 Network validator에도 같은 #557 application 경계 연결

source56f의 [PR Rust40191336989/UI121174299528](https://github.com/bin9208/openpilot-rust/actions/runs/40191336989/job/121174299528)
원문에서 Forms14개는 PASS한 뒤 Network는6PASS/2FAIL했고,57source manifest의
application old `d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00` pin이
#557 current font source를 거부했다. 부모가 확인한 이 실제 실패와 source 연결을 기록하며
새 artifact 다운로드나 original capture/expected 생성은 하지 않는다.

Network public `validate_source`의 **정확 application.py path+old expected pin의 쌍**에만
기존 `ui_application_logic_check::application_source_matches`를 연결했다. 다른 path/pin은 원래
fullSHA 비교다. manifest SHA `4967ad887a8cee21ae050ceb592ab1b432cceb98861c9d2df55105a82d559c5e`와
57pin·retired originals·font 부재·dependency lock·에러는 유지한다. 공통 matcher와 production
application.py를 수정하거나 current wholeSHA를 production 허용 목록에 추가하지 않았다.

기존 integration root에는 child module 등록2줄만 추가했다. 새 filesystem source 통제4개는
current 양성, font3구간 각각 변조/같은길이 제거/중복/순서 변경, 비font FPS/stack/encoder 변조,
같은 current bytes의 다른 path/expected pin·font fallback·dependency lock 거부를 검사한다.
음성 전 양성과 원복 후 양성도 요구한다. Network에는 Forms의 Params provider 검사 자체가 없으므로
그 검사를 새 production 조건으로 도입하지 않았다. current wholeSHA는 test 입력 출처 pin뿐이다.

기존 root4/support4 총8개 test body와 original state/PNG/env·타 source bytes는 그대로다.
새4개는 Python/graphics/CLI/원본을 실행하는 검사가 아니다. 로컬은 Rustfmt·source 역투영·diff만
확인했고 새4/기존8·실제 Network CLI/원본 비교·source/SDK/provider 봉인은 **GHA UNRUN**이다.
#585 해결·staged-font 기능·전체 UI/#523/원본 퇴역 credit은 주장하지 않는다.
WF/Focus/ledger/Cargo 등록은 부모가 담당한다.

## #575 Forms 실제14와 #585 후속 경계 — 2026-10-25

[PR343 run40191336989/job121174299528](https://github.com/bin9208/openpilot-rust/actions/runs/40191336989/job/121174299528)는 actual merge `f8c781ede4b574cb127c51c42730e66be2078206`이며 head56f와 tree `dfc5bc8efbede54369e02689015fd876a7050efc`가 같다. Forms admission14는0FAIL/0ignored/0filtered,132.45초로 성공했고 기존10과 신규source4를 모두 포함했다. `UI_FORMS_RAW_HELPER`가 실제 설정되어 PNG/state 원본 비교 test가 실행됐다. 뒤 Network6PASS/2FAIL로 전체job은실패했으므로 UI/Clippy 전체수용과 #575종료는보류한다.

부모는 artifact12600183865(15103335B/SHA `03b0ad05cf0babfa4ac00686196e7a16d755b245c49f1ca879ca3d1ebeafd646`/543safeunique+CRC), private12600558220(290333B/SHA `a9bff8f3b0c32187e1ec0badec39bee32ffc430804822f96ae6ca04dd6ec5cd1`/12safeunique+CRC)를 한 번 감사했다. private SOURCE_COMMIT/ownership은actual merge에묶이고source/index전후가byte동일하다. 성공PNG별도재보존·wholeSDKFD/DSO감사를새로입증했다고쓰지않는다. #585 새source4/full12의실제수용은다음정확source PR검사에서확인한다. 원본삭제/retirement credit0이다.


### #586 Render validator의 같은 application 경계 누락 — source 후보

#575/#585 후속 source inventory에서 Render도 기존 application old
`d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00` 전체 SHA만
허용하고 #557 font source 경계가 연결되지 않은 것을 확인했다. Render manifest37pin 중
현재 다른 live34개는 정확히 같고 app1만 c659로 달라졌으며 retired originals2개는 부재다.
PR56 UI는 Network에서 먼저 중단돼 Render의 실제 새 실패를 관측한 것은 아니다.

Render public `validate_source`도 **정확 application.py path+기존 d17a expected pin 쌍**에만
기존 `ui_application_logic_check::application_source_matches`를 호출한다. 다른 path/pin은
원래 full SHA 비교다. manifest SHA
`e2c03cc3d440ae44a81490ffd3153739e6c00accedc4ef0d4f5009738799cfe6`와37pin,
Params import·font 부재·dependency·retired2/에러는 유지한다. 공통 matcher/production
application.py를 수정하거나 current wholeSHA를 production 허용 목록에 추가하지 않았다.

기존 integration root에는 nested module 등록2줄만 추가했다. 새 filesystem source 통제4개는
current 양성, font3구간 각각 변조/같은길이 제거/중복/순서 변경, 비font FPS/stack/encoder 변조,
같은 current bytes의 다른 path/expected pin·Params 원문/provider·font fallback·dependency 거부다.
음성 전과 원복 후 양성도 요구하며 c659 wholeSHA는 테스트 입력의 출처 pin뿐이다.
기존 root5/support4 총9개 test body와 원문 state/PNG/env·타 source bytes는 그대로다.

로컬은 rustfmt·source 역투영·pin/보호파일 SHA·diff만 확인했다. 새4와 기존9, 실제 Render
CLI·원본 graphics 비교·Clippy/sourceSDK/provider 후검사는 **GHA UNRUN**이다. 원본 capture/
expected 재생성0, #586 해결·staged-font 기능·전체 UI/#523/원본 퇴역 credit0이다.
WF/Focus/원장/Cargo 등록과 실제 수용은 부모가 담당한다.
