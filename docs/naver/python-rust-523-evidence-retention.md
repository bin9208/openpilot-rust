# #523 Nov5 driver object witness의90일 보존 설계

작성: 2026-10-26. 검토 HEAD `7a928cefe22e3c81bcb941f6ed20eaa80eeabc59`.
상태: **부모 승인 후 기존 capacity WF의 고정 보존 mode 구현 완료, 실행은 UNRUN**.
수정은 `.github/workflows/python-rust-523-capacity.yml`과 이 문서뿐이다.
consumer/공유 policy/ledger/pin 변경·GHA dispatch·업로드·commit/push는0이다.
기존 #476/six-source credit pending을 유지한다. 이 보존 sidecar는 primary 기능 경로가 아니며
모델 재실행·원본 capture·compiler replay·release/장치 게시를 수행하지 않는다.

## 정확한 보존 대상과 origin

| 항목 | 고정값 |
| --- | --- |
| repository | `bin9208/openpilot-rust` |
| artifact / name | `12393290469` / `model-driver-cpu-context-296-0` |
| origin run / attempt | `39700512883` / 1 |
| API workflow head | `0e522bf2a968082195f08ffa4692a3f2ecf6d775` |
| 내부 actual checkout | `a29832fd0d046253465621a05f02a91e9cfdb1d1` |
| 원문 ZIP bytes | 20550808 |
| 원문 ZIP SHA256 | `a318335767594bfe8bcd08878f15e4f6a03bb0d8d13541d56f3b83243b19f461` |
| 만료 | `2026-11-05T00:41:04Z` |
| ZIP roster | 258 regular files / 37492774B expanded; 전체 SHA·CRC·canonical/no-symlink는 기존 inert 감사 완료 |

API 재조회 시 expired=false이고 origin run은 completed/success, push다. origin repository와
head repository가 모두 같은 저장소다. 인용 경로는
`driver-catalog/catalog/1dc28498bdbd261b337f9872f30d68fa9361ab79ebef816737d38a42e897a191/`다.
두 해상도object90개는 공통 모델44개×2와 각 warp1이며 독립 PKL full image와90/90 일치했다.
current fresh object는 두 해상도kernel34가 달라 원래 ZIP을 새 compiled output으로 재생성할 수 없다.
상세 byte/CPU 한계는 `python-rust-523-model-family-ee74-audit.md`를 따른다.

API `GET /repos/bin9208/openpilot-rust/actions/permissions/artifact-and-log-retention`은
`days=90`, `maximum_allowed_days=90`을 반환했다. 저장소는 public이다.
그러나 기존 artifact의14일 만료는 그대로다. 보존 설정은 새 객체에 적용되고 기존 객체에
소급하지 않는다는 [GitHub 보존 문서](https://docs.github.com/en/organizations/managing-organization-settings/configuring-the-retention-period-for-github-actions-artifacts-and-logs-in-your-organization)와도 맞는다.
기존 artifact의 만료를 바꾸는 API 호출은 하지 않았다.

## 정적 consumer와 다른 artifact 구분

| consumer / 자료 | 현재 계약 |
| --- | --- |
| `tools/ci/driver_retained_program_476.py:RETAINED_OBJECTS` | 위 artifact/run/workflow head/checkout/bytes/SHA/bundle을 고정하는 유일한 직접 code constant |
| `.github/workflows/python-rust-523-driver-retained.yml` object-auth/object-run-auth/object-download | helper constant로 API를 읽고 exact ID·run·head·size·digest·expired=false 및 origin repo를 검증; `driver476-frozen/artifact.zip`을 exact bytes/SHA로 hold |
| 같은 WF의 독립 request 준비 | 원래 model44/warp1 SOURCE에 해당하는 object를 선택. expected image는 독립 PKL에서만 읽음 |
| `model_family_retirement.rs::cached_driver_uses_actual_model_run_and_daemon_before_full_retirement` 및 cached ELF/ISA/prestate API | 준비된 request/object/image를 소비함. artifact ID를 직접 선택하지 않음 |
| 현재 fullsix caller | 위 witness를 소비하지 않음. 현재 source의 fresh compiled catalog를 사용 |

현재 driver WF는 먼저 fresh 실패를 요구하고, fresh exit0이면 object 다운로드 전에 중단한다.
업로드 roster에도 **`driver476-frozen/artifact.zip` 자체는 없다**. 따라서 현재 진단을 단순
재실행하거나 `retention-days`만90으로 바꾸는 것은 원문 ZIP 보존 경로가 아니다.

최근 보조 자료도 원래 witness의 복제본으로 간주하지 않는다.

- source425 `driver-retained-476-40227415638-1`: artifact`12612441493`,42099934B,
  SHA`cbc56ca968048ffe799ca6939334ff895d193f887970e74163bbe0fdc672276a`.
- source7a82 `driver-retained-476-40230515045-1`: artifact`12613945216`,42162581B,
  SHA`12ded513cbe634e852bd437a891b18c052aeea23dcca94e711e6c0d03545158b`.
  이번 Range 읽기에서는297멤버 중 ZIP/object-N.o가0개이며 outcome은fresh gate0·failure observed=false다.
- 원본 fixture provenance donor는 collector`12360417493`(models unavailable),
  no-collector`12369678295`, collector-current`12377006222`다. 현재 고정 fixture가 필요한 모델4와
  원본 bytes를 보유하므로 witness 보존을 위해 donor를 다시 capture/download할 필요는 없다.
- ee74 fullsix artifact`12614396134`는 fresh 성공 증거이며 원래 witness/retained image의 대체품이 아니다.

## 기존 입력으로 바로 가능한지

**구현 전 유지 중인 WF 입력 그대로는 불가능했다.** 위 검토 HEAD의 WF50개를 파싱했다.
실제 dispatch input이 있는 것은 native-units의 `crate`와 sync의 source_repository/target_branch뿐이다.
ZIP/로컬 path/보존 artifact를 지정하는 dispatch input은 없다. sync는 이 목적에 사용하지 않는다.

- 구현 전 capacity WF는 `workflow_dispatch: {}`이며 기존14일 cache inventory upload만 있었다.
- driver-retained 등 진단 WF도 현재 raw ZIP 보존 input이 없고 본래 workload를 실행한다.
- origin의 `model-driver-cpu-context-296-once.yml`은 push 전용이며 matrix sample0/1의 원래
  진단을 수행한다. 기존 input으로 data-only 재업로드하는 경로가 아니다.
- Actions registry의334개 항목에는 현재 tree에 없는 과거 one-off가 포함된다.
  registered/active라는 표시만으로 현재 branch에서 사용할 보존 경로라고 판정하지 않는다.

따라서 새 WF 파일이나 과거 WF 복원 대신 **기존 capacity WF 한 파일의 고정 choice
mode**를 제안했고 부모가 승인했다. 이번 변경 자체는 WF 개수를 늘리지 않으며 shared policy의
77→79 같은 수치 확대·새 일반 controller·새 Python helper를 요구하지 않는다.

## 승인된 최소 설계: 기존 capacity의 고정 data-only mode

수정한 실행 경로는 `.github/workflows/python-rust-523-capacity.yml` 하나다.
기존 capacity mode/실제 bin-actions guard/10분/concurrency cancel=false를 유지한다.

1. `workflow_dispatch.inputs.operation`을 choice `[inventory, preserve-driver476-witness]`,
   기본값inventory로 추가한다. 보존 mode는 명시 dispatch·해당 저장소/검토 branch에서만 허용한다.
   artifact ID/URL/local path를 받는 자유 입력은 만들지 않는다.
2. 보존 mode에서는 두 기존 cache inventory step을 실행하지 않는다. shared cache·SDK·
   model/proc census를 다시 읽지 않는다. 첫 소유 임시 report 생성/실제 runner guard를 재사용하고
   새 branch에서 GITHUB_ACTIONS/repository/ref/실제Linux-X64 및 canonical/uid/dev-inode를 확인한다.
3. 새 Bash step이 기존 `/usr/bin/curl`, `jq`, `sha256sum`, `unzip -t`로 고정 origin API와
   ZIP을 읽는다. 새 Python 파일/모델 실행/Cargo/SDK 설치는 없다. `permissions`는
   contents:read와 cross-run download용 actions:read만 필요하다.
4. 원문 API metadata·origin run metadata를 먼저 봉인하고 위 표의 ID/run/head/repo/size/SHA/
   expired=false를 exact 검증한다. 기존 파일은 덮어쓰지 않고 소유 report 아래 고정 이름으로
   ZIP을120초·20550808+1B cap 안에서 받는다. 길이·SHA·CRC258을 확인하며 전체 풀기/실행은 하지
   않는다. 인증된 고정41B source metadata 두 개만 `unzip -p`로 읽는다.
5. literal 파일 `driver476-object-witness-12393290469.zip` 한 개를90일 **as-is** 업로드한다.
   별도 proof 하위에는 원문 origin API JSON, origin run JSON, preservation source/run/owner,
   payload bytes/SHA와 upload metadata를 jq로 기록해 기존 artifact upload 방식으로90일 보존한다.
6. 새 artifact의 ID/run/head/size/digest/expires_at을 API로 읽고 **기존 consumer와 같은 `/zip`
   다운로드를 한 번** 수행해 원문20550808B/SHA와 다시 대조한다. 그 뒤에만 보존/호환 성공을 기록한다.
   owner/dev-inode·두 upload receipt와 roundtrip 성공을 확인한 뒤 해당 소유 report만 회수한다.

payload+roundtrip+소량 proof의 disk 예상 증가량은64MiB이며 구현이 실제 FS admission을 수행한다.
curl/jq 중심 비기능 보존 작업의 memory512MiB는 **미측정 예상치(forecast)**일 뿐이다.
memory/cgroup admission이나 사용량 계측을 구현하지 않았으므로512MiB 이내 실행·peak를 보장하거나
검증했다고 주장하지 않는다. disk 예상치 역시 peak 측정값이 아니며 동일 FS의 증가량을 합산한다.
기존 inventory/모델 gate의 admission을 줄이는 제안이 아니다. 자동 retry/다른 archive fallback은 없다.

### 재압축하지 않는 기존 action 입력

현재 repo의 controls-diagnostic도 이미 `actions/upload-artifact@v7`을 사용한다.
공식 v7의 `archive:false`는 single file을 그대로 올리며 artifact name은 파일 basename을 따른다.
[공식 v7 action 입력](https://raw.githubusercontent.com/actions/upload-artifact/v7/action.yml),
[기능 공지](https://github.blog/changelog/2026-02-26-github-actions-now-supports-uploading-and-downloading-non-zipped-artifacts/)를 확인했다.
조회한 v7 tag commit은 `cf430e030ddbb5b0abf93d22962f4752f3646cd9`다.

아래 upload 계약을 이번 WF에 적용했고 실행은 하지 않았다. metadata proof는 별도 경로에 두어
single-file upload에 섞지 않는다. 기존 trusted-condition prefix를 그대로 포함한다.

```yaml
- name: Preserve authenticated driver476 witness bytes
  id: witness_zip
  if: ${{ (github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository) && (github.event_name == 'workflow_dispatch' && inputs.operation == 'preserve-driver476-witness' && steps.witness_auth.outcome == 'success') }}
  uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9
  with:
    path: ${{ env.CAPACITY_REPORT }}/payload/driver476-object-witness-12393290469.zip
    archive: false
    retention-days: 90
    if-no-files-found: error
    overwrite: false
```

향후 실제 preservation GHA가 size/digest/90일 기한과 HTTP roundtrip을 확정해야 한다.
공식 REST download 형식은 여전히 `zip`이며 이 repo에서 v7 as-is와 같은 endpoint의
호환성을 실측한 증거는 아직 없다. [Artifact REST API](https://docs.github.com/en/rest/actions/artifacts)의
다운로드 계약에 따라,
추측으로 성공 처리하지 않고 위 data-only roundtrip1회로 확인한다.

### 새 archive와 기존 consumer의 호환성

- v7 as-is가 계약대로 동작하면 payload size/SHA와 내부258member/path는 원래와 같다.
  반면 **artifact ID/run/workflow head/생성일/기한/name은 새 보관 실행의 값**이 된다.
- 현재 consumer는 원래 ID/run/head/expired=false를 exact 요구하므로 bytes가 같아도
  새ID를 환경변수로 넘기는 것만으로 사용할 수 없다. origin pin을 새source로 덮어쓰지 않는다.
- 향후 consumer 연결 시 원래 `RETAINED_OBJECTS`는 origin으로 유지하고, 부모가 실제로 얻은
  새artifact ID/run/reviewed preservation source/metadata proof를 별도 고정 transport binding으로
  인증한다. 마지막 `artifact.zip`20550808B/SHA/bundle/checkout source/PKL/ABI/ISA/수치 계약은 불변이다.
  Rust/model/prestate API 변경은 필요 없으며 WF의 auth/download 경계가 주 연결 후보가 된다.
- 원래 API가 만료된 뒤에도 expired=false로 취급하지 않는다. 승인된 preservation run이
  생존 시점에 검증한 origin metadata와 동일 bytes의 연결을 인증한다. 새 transport 값은
  UNRUN이라 미정이며 새 artifact ID/source SHA를 여기서 만들어내지 않는다.
- v6/archive:true로 감싸면 외부ZIP SHA가 달라진다. 원래 ZIP을 고정 내부member로 저장할 수는
  있으나 현재 consumer에 별도 엄격한 unwrap 경계가 필요하다. 투명한 대체나 자동 fallback은 아니다.

## 정적 policy 경계와 남은 작업

`ci_policy.rs`는 upload action을 `actions/upload-artifact@` prefix로 인식하므로 위 pin도
source상 인식 범위에 든다. capacity job에는 `CI296_ALIAS_BASENAME`이 없어 private-source
layout validator가 해당 job에서 조기에 반환한다. 기존 fork/trusted-condition/runner/concurrency
검사는 유지한다. 새 step 조건은 trusted prefix 괄호 안에 두고 기존 default mode를 보존한다.
새 WF·Rust policy·기존 테스트 수 변경이 필요하다는 근거는 발견하지 못했다. 변경 후 WF의
GHA policy 통과를 이번에 확인한 것은 아니며 부모의 정규 source pin 통합은 별도로 필요하다.

초기 brief는 source/API/공식 문서 읽기와 설계 문서만으로 완료했고, 아래 구현은 이후 부모의
명시 승인에 따른 별도 수정이다. 기존 artifact 삭제·기한 변경, 임의 archive 공개,
consumer/ledger/pin 편집, release/device publication, capture/model/oracle/전체 six 재실행은0이다.

## 승인 후 구현과 정확한 부모 실행 계약 — 2026-10-26

구현 시작 HEAD는 `51fd430f76dd6bd28776ac1fbd04c16955bd977e`다. 해당 capacity 원문의 SHA는
`47df95c7bee23624b65722bcf6f4633429dbae55697d24d32c1a2e6aa9dfc850`이었다.
최종 리뷰를 반영한 WF는33936B/SHA256 `be4377d90d34a32444084b92fdb75555e062ba9a89de9a85107dc539e588ea39`다.
새 파일/새 job/새 source alias/새 helper/Python/controller는 추가하지 않았다.
HOST 감사 문서는 수정하지 않았으며 SHA
`ea99682ee974433cc934b3fcff4062e1ac936003cd1a43e4f7549ec62d3136e1`로 동결했다.

### 유지한 기본 경로와 분리

`jobs.inventory`와 runner `[self-hosted, Linux, X64, dev-md-local]`, timeout10분,
concurrency group/cancel=false/queue=max를 유지했다. `operation`은 required choice
`inventory`(default)/`preserve-driver476-witness`뿐이다. push와 input 생략은 inventory로 간다.
알 수 없는 값은 첫 guard에서 exit64로 거부한다. 두 기존 inventory step의 run scalar bytes와
기존 upload의 name/path/14일/if-no-files-found 입력은 수정 전과 같다. inventory의 cleanup도
새로 추가하지 않았다. cross-run 인증 GET에 필요한 `actions: read`만 권한에 추가했다.

보존 경로는 명시 `workflow_dispatch`, 저장소와 branch `codex/feat-523-python-rust`, 해당 WF ref,
실제 bin-actions/self-hosted/Linux/X64·uname x86_64를 첫 guard에서 요구한다.
보존 mode에서는 inventory 두 step과 기존 inventory upload가 모두 false다. 따라서 SDK/cache
파일·프로세스 census·모델/컴파일러/Python 실행에 진입하지 않는다. 공통 첫 step은 원래와 같은
mktemp report와 run:attempt:job owner만 만든다. 보존 시에만 umask077/noclobber를 적용하고
report dev:inode:uid:mode를 기록한다. 모든 후속 조건은 trusted prefix 뒤의 **전체 나머지 식을
괄호 안에** 넣었다. `trusted && (...) || always()` 형태는 없다.

### 단계와 실패 경계

1. `witness_auth`: report/owner의 canonical/no-symlink/uid/0700·0600/dev-inode를 검증하고
   해당 FS에64MiB 예상 증가량을 admission한다. 고정 artifact와 원래 run API의 ID/name/head/
   repo IDs·names/size/digest/expired=false/status=completed/conclusion=success/attempt1/push/
   producer WF path를 먼저 검증한다. 현재 preservation run도 exact source/run/attempt/repo/ref/WF
   identity로 인증한다. 원본 binary GET은 이 검증 뒤에만 발생한다.
2. 인증 GET은 절대 경로 `/usr/bin/curl`로 한다. JSON마다30초/1MiB max-filesize와 커널
   file-size limit, create_new 성격의 noclobber FD를 적용했다. API의302/HTTPS redirect만 받아
   별도의 **토큰 없는** HTTPS GET으로 payload를 만든다. signed URL/header를 파일이나 log에
   저장하지 않는다. curl/jq/unzip 등 필요 실행물이 없으면 설치하지 않고 실패한다.
3. 각 binary transfer는 timeout120초(TERM 뒤5초 KILL), curl110초, max-filesize20550808B,
   head20550809B streaming cap·pipefail로 제한한다. 원문20,550,808B/SHA를 확인한 후
   CRC258와 `SOURCE_COMMIT=a298...`, `WORKFLOW_COMMIT=0e522...`의 실제41B member를 읽는다.
   PKL/object/model은 실행하거나 source로 다시 컴파일하지 않는다.
   ZIP 내부 directory/path·symlink·특수파일 안전성은 이번 WF에서 일반 검사기를 새로 실행하는
   대신, 기존 inert 감사에서 확인한 exact ZIP의20550808B/SHA에 결합한다. CRC/258 roster 검사만으로
   그 안전성을 새로 증명했다는 의미가 아니며 SHA가 다르면 ZIP member를 읽기 전에 실패한다.
4. `witness_zip`: 검토 pin `cf430e030ddbb5b0abf93d22962f4752f3646cd9`의 v7 `archive:false`,
   90일/overwrite=false로 고정 ZIP 한 개만 올린다. 새 ID와 digest는 action output에서만 받는다.
   같은 run에서 같은 basename이 이미 있으면 덮어쓰지 않고 실패한다.
5. `witness_roundtrip`: report/하위 경로 및 held payload의 dev/inode/uid/mode/size/mtime/ctime/SHA를
   재확인한다. 남은 roundtrip+metadata의20550809+4MiB를 같은 FS에서 다시 admission한다.
   새 metadata가 action ID·현재 run/head/repo·정확한 basename·20550808B·원문 digest와 맞아야
   한다. API 만료는 run created_at+90일 이상, job10분 이내 차이로 검증한다. 원본과 **같은
   `/actions/artifacts/<new-id>/zip` endpoint**를 받아 길이·SHA·cmp를 통과한 뒤에만
   `ROUNDTRIP_VERIFIED`를 만든다. 외부 ZIP이 생기거나 v7 endpoint가 투명하지 않으면 실패하며
   unwrap/retry/fallback은 없다.
6. `witness_proof`: 인증 원본·현재 run JSON/source/owner/growth/CRC/member roster와
   roundtrip 결과를 별도90일 provenance artifact로 올린다. 인증 이후 실패하더라도 이 upload는
   실행해 진단을 남길 수 있으나, 실패한 이전 step을 성공으로 바꾸지 않는다. roundtrip 실패 시
   성공 receipt와 cleanup은 없다.
7. `witness_complete`: 두 action output ID가 서로 다르고 digest 형식/값이 맞는지 확인한다.
   두 artifact API를 다시 읽어 exact ID/name/current run/head/repo/digest/90일 기한을 검증한다.
   raw size는 원문 크기, 별도 proof ZIP은0 초과1MiB 이하를 요구한다. stored roundtrip의
   source/run/원문 bytes/SHA·no-unwrap도 다시 검증한다. held payload와 readback SHA·report owner/
   dev-inode 확인 뒤 해당 report 하나만 `rm --one-file-system`으로 정리한다. 이 뒤에만
   `PRESERVATION_VERIFIED`·두 ID/digest를 log/step summary와 최종 output에 기록한다.

provenance archive는 자기 자신의 ID/digest를 내부에 담을 수 없으므로, payload ID/digest와
roundtrip까지 내부에 담고 provenance ID/digest의 최종 API 인증은 job summary/log로 결합한다.
별도 proof가 성공했더라도 raw roundtrip을 통과하지 못하면 최종 성공 상태는 생기지 않는다.
초기64MiB는 두20.55MB ZIP·최대6MiB JSON·소량 CRC/proof ZIP의 예상 증가량이며 측정 peak가 아니다.
기존 inventory/모델 gate 예산이나 job 기한을 줄이거나 늘리지 않았다.

### 부모가 마지막으로 실행할 입력과 남은 실제 증거

부모가 검토 source를 publish한 후 기존 WF에서 아래 하나의 입력으로 dispatch한다. 이 명령은
인계용 원문이며 이번 작업에서 실행하지 않았다.

```sh
gh workflow run python-rust-523-capacity.yml \
  --repo bin9208/openpilot-rust \
  --ref codex/feat-523-python-rust \
  -f operation=preserve-driver476-witness
```

새 preservation ID/run/source는 아직 없다. parent의 실제 단 한 번의 data-only 실행에서
v7의 새 artifact API size/digest,90일 만료, same `/zip` readback 원문 SHA와 두 upload receipt,
정상 owned cleanup을 확인해야 한다. 이에 실패하면 원래 witness가 살아 있는 동안 실패 원인을
검토하며 임의 감싸기·consumer pin 바꾸기·진단/모델 재실행으로 대신하지 않는다.
consumer의 `RETAINED_OBJECTS`와 driver helper/WF는 건드리지 않았다. 원래 fresh-failure sticky와
#476/six-source credit pending도 그대로다. 실제 보존 ID가 생긴 뒤에만 별도 transport binding을
부모가 검토할 수 있다.

로컬 확인은 YAML parse, 각 Bash run의 `bash -n`, 기존 inventory run/upload 입력의 source 동등
대조, trust 식의 괄호와 diff/whitespace 검토뿐이다. Bash run은6개이며 후속 trust 조건은8개다.
프로젝트 policy/Cargo/기능 테스트나 새 shell
본문·curl/jq/download/upload/cleanup을 로컬에서 실행하지 않았다. CI policy/Focus/source pin 및
GHA 결과 수용은 부모 소유다. 기존 workflow 개수·alias 등록 수를 바꾸는 shared77→79 수정은0이다.
