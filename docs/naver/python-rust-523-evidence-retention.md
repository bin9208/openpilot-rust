# #523 Nov5 driver object witness의90일 보존 설계

작성: 2026-10-26. 검토 HEAD `7a928cefe22e3c81bcb941f6ed20eaa80eeabc59`.
상태: **source972468 보존 actual 성공·원문 byte 감사 완료. 후속 expiry 보정은 실제 API와의 불일치 발견**.
수정은 `.github/workflows/python-rust-523-capacity.yml`과 이 문서뿐이다.
agent의 consumer/공유 policy/ledger/pin 변경·GHA dispatch·업로드·commit/push는0이다.
부모가 실행한 기존 run의 실제 artifact/roundtrip/cleanup과 만료 시각 한계는 마지막 절에 기록한다.
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
첫 게시본 WF는33936B/SHA256 `be4377d90d34a32444084b92fdb75555e062ba9a89de9a85107dc539e588ea39`다.
이후 queue/expiry source 보정은 아래 실제 run 관측 절에 별도로 기록한다.
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
   한다. API 만료는 artifact 자체 created_at+90일 이상/+600초 이내를 요구하고,
   run.created_at≤artifact.created_at 및 현재 run/head/repo 결합도 확인한다. queue 시간은
   job10분이나 보존 기한에 포함하지 않는다. 원본과 **같은
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

최초 인계 시 새 preservation ID/run/source는 없었다. 이후 dispatch 관측은 아래 절에 기록한다.
parent의 실제 단 한 번의 data-only 실행에서
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

## source972468 dispatch의 관측과 queue/expiry 보정 — 2026-10-26

부모가 게시한 source는 `972468fab5279a9897d2d2bda736938c89a81a9a`다.
기존 [dispatch40241602401](https://github.com/bin9208/openpilot-rust/actions/runs/40241602401)은
workflow_dispatch/attempt1/같은 저장소와 source로 확인했고 API created_at은
`2026-10-25T22:44:29Z`다. 이번 source 수정 직전 API 상태는 **pending/job0/artifact0**이며,
아직 원본 인증·업로드·roundtrip·두 receipt·cleanup의 actual 성공이나 최초 실패는 없다.
등록 runner22 bin-actions는 online/busy이고, 같은 source의 자동 inventory
run40241573554도 runner 배정을 기다렸다. 이 대기 상태를 기능 실패로 판정하지 않는다.
후속 API에서 inventory run40241573554는 `2026-10-25T22:49:21Z`에 SUCCESS로 완료됐고,
보존 run은 `22:49:20Z`부터 queued/job`121315471388`로 진행했다. 당시 조회에서도
해당 job은 runner_name 빈값/steps0/queued이며 artifact0이었다. 기본 inventory의 성공을
아직 시작하지 않은 보존 mode의 성공으로 세지 않는다.

부모의 정적 리뷰에서 첫 게시본의 두 expiry 검사가
`run.created_at+90일 ≤ expires_at ≤ run.created_at+90일+600초`인 결합을 찾았다.
run.created_at은 queue 이전 시각이므로10분보다 긴 queue 뒤 생성된 정상90일 artifact도
상한을 넘을 수 있다. 이는 **source로 확인한 잠재 거부 원인**이며 pending run의 관측된
실패 원인으로 단정하지 않는다.

WF의 roundtrip 검사와 최종 payload/proof receipt 검사 두 곳만 다음 시각 관계로 보정했다.

```text
run.created_at <= artifact.created_at
artifact.created_at + 90*86400 <= artifact.expires_at
artifact.expires_at <= artifact.created_at + 90*86400 + 600
```

artifact별 실제 created_at을 사용하며, 기존 API의 현재 run ID/head/repository 결합을 그대로
요구한다.90일 하한과 기존600초 timestamp window의 크기는 유지하고 기준 시각만 분리했다.
원문 artifact/run/source/checkout/bytes/SHA, v7 archive:false/90일, same `/zip` 원문 bytes,
두 upload receipt/소유 cleanup, no-unwrap/no-fallback/no-new-consumer는 바꾸지 않았다.
보정 WF는34293B/SHA256 `b7b83241b04f3e3688791e72a2579a84862c925b472ace3fbd9c1569825de1fb`다.
YAML/Bash 문법과 diff를 확인했고, 두 expiry 경계를 역투영하면 게시본 WF SHA로 돌아간다.
이는 source 검사이며 새 expiry 조건의 기능 실행 결과는 아니다.

이 수정은 당시 로컬 source에만 있었고 부모가 이후 `39800e064e1e7bae7fd01adcb1952505f1db41f7`에 게시했다.
이미 dispatch된972468 run의 실행 source를 바꾸거나
취소·재실행하지 않았다. 새 dispatch/업로드·consumer pin 변경도0이다. 기존 run은 원래 source의
actual 결과로 계속 구분해서 관측해야 하며, 새 expiry 보정의 actual 수용은 부모가 별도로 소유한다.

## 기존 source972468의 actual 보존 감사 완료 — 2026-10-26

기존 run40241602401/attempt1/source`972468fab5279a9897d2d2bda736938c89a81a9a`는
`2026-10-25T22:59:57Z`에 completed/SUCCESS다. job121315471388은 실제 runner22/bin-actions/
Default에서 `22:59:23Z`부터 실행됐고 setup 뒤 first guard·원본 인증·as-is upload·same `/zip`
roundtrip·별도 proof upload·최종 두 receipt 인증/owned cleanup이 모두 SUCCESS다.
기존 inventory 두 step과 inventory upload는 모두 skipped여서 cache/SDK/process census에
진입하지 않았다. 새 자동화를 만들기 전에 완료를 확인했으므로 후속 자동화도 생성하지 않았다.

### 실제 archive와 두 upload receipt

| 역할 | artifact / bytes / SHA256 | created_at / expires_at |
| --- | --- | --- |
| 원문 as-is payload | `12617590637`, `driver476-object-witness-12393290469.zip`,20550808B / `a318335767594bfe8bcd08878f15e4f6a03bb0d8d13541d56f3b83243b19f461` | `2026-10-25T22:59:41Z` / `2027-01-23T22:44:30Z` |
| 별도 provenance ZIP | `12618005602`, `driver476-witness-provenance-40241602401-1`,9261B / `f8d72c81b3bdb266e1e98b6d04fbfe242a9db62213c1e4a15ef727bfa337ff57` | `2026-10-25T22:59:50Z` / `2027-01-23T22:44:30Z` |

두 API metadata 모두 expired=false, workflow_run40241602401/head972468 전체 SHA,
repository_id=head_repository_id=1395501963 및 검토한 정확한 이름/길이/digest와 일치했다.
실제 upload action 두 개 모두 검토 pin `cf430e030ddbb5b0abf93d22962f4752f3646cd9`를 사용했다.
payload는 archive:false/90일, proof는 별도 archive/90일이며 overwrite=false다.

이번 독립 감사는 API metadata의 크기/digest를 먼저 확인한 뒤 합32MiB 미만의 inert download로
두 ZIP을 같은 `/actions/artifacts/<id>/zip` endpoint에서 다시 받았다. cache는 기존
`.analysis/scratch/2026-10-25-523-completion/ci/{12617590637,12618005602}.zip`이다.
payload20550808B/258멤버/37492774B expanded, proof9261B/10멤버/85039B expanded의 전체
SHA·CRC·unique relative safe path/no-symlink·special-file 거부를 확인했다. ZIP은 풀거나 실행하지 않았다.
새 payload 전체 bytes를 기존 cached origin12393290469 ZIP과 chunk별 대조해 byte 동일함을 확인했다.
이 플랫폼/실제 run에서는 v7 as-is 업로드가 `/zip`에 추가 wrapper 없이 원문 ZIP을 반환함을 입증한다.

provenance에는 원문 JSON/source metadata/CRC/roster를 그대로 담은10개 파일이 있다.

| member | bytes | SHA256 |
| --- | ---: | --- |
| SOURCE_COMMIT | 41 | `09787d2b7fabb5991bfee84828d6f4efd35fe71e3009e0a52d31fb965b443a04` |
| WORKFLOW_COMMIT | 41 | `dddc639f43883639775aec1cdf1837dfe347f53f19abe9f4924a95d2b1cdbc87` |
| origin-artifact.json | 820 | `26e571196f2cb8ba1800186dbbcc60108951607b3d06a7b65b5214a4d796ce45` |
| origin-binding.json | 710 | `08e0499957cb8b64c92b3c907f2257b26db834cf895fe2b1deb830be19b51c39` |
| origin-crc.txt | 29647 | `3efa8758bd152cb380917f4870ac7332ca26553e38c3861f8f73c5fb79458117` |
| origin-members.txt | 24581 | `6a3a8ee5ceaeb791f0aeba3578c924527704dae1f9fbe602023dc53e426c9ace` |
| origin-run.json | 13985 | `e0d04b4368cef26fb8c29aacf7dcd032f2647fefdb3a9c7249df31fb553bb8fe` |
| preservation-run.json | 13938 | `b1852247250a08be5b541fa983da0d8cc729a28f6b32ac6ee8d1f67f3c5ccbcc` |
| preserved-artifact.json | 819 | `bb764ca3d2290ff00226b62bb0fe448b4c028327ef3eed93363b62fc4bad136f` |
| roundtrip.json | 457 | `3ec36ce2a52e2a90ea93e8ac97386c7bb6e839df6f2b5bab02c0df4fb160901e` |

JSON은 duplicate key를 거부하는 inert reader로 읽었다. origin-artifact/run은 원래
12393290469/run39700512883/attempt1/head0e522 전체 SHA/같은 repo IDs·names/20550808B/
원문 digest/expired=false 및 completed/success를 보존한다. SOURCE_COMMIT은a298 전체 SHA,
WORKFLOW_COMMIT은0e522 전체 SHA의 원래41B member와 같다. origin-members258개는 새 payload의
실제 전체 member roster와 같고 CRC log는 `No errors detected`다.

origin-binding은 owner=`40241602401:1:inventory`, report identity=`64521:289460:1000:41c0`,
실제 FS free before=33592328192B, admitted growth=67108864B를 기록한다.
replay_executed=false/consumer_binding_changed=false이며 모델·원본·컴파일러 실행은 없다.
roundtrip.json은 artifact12617590637/run40241602401/source972468,20550808B/원문SHA,
status=ROUNDTRIP_VERIFIED, requested_retention_days=90, endpoint=/zip,
unwrapped=false/original_bytes_equal=true와 expires_epoch1800744270을 기록한다.
이 JSON을 새 expected 생성이나 full-model 수용 증거로 사용하지 않는다.

### 실제 cleanup과 수용 범위

job 원문 log는38040B/404행/SHA256
`7d3ce3628b01c69e616f22e614da1cd260b6542805c721b69b8510d5f04e8915`다.
원문 SHA OK·각 upload ID/digest·roundtrip SHA OK·최종 두 archive SHA OK를 확인했다.
최종 step10은 `22:59:51Z`부터 `22:59:52Z`까지 SUCCESS이며 실제 stdout의
PRESERVATION_VERIFIED receipt가 두 ID와 위 두 digest/source/run을 정확히 결합한다.
이 receipt는 원래 source가 두 API receipt/owner/dev-inode/held SHA를 확인하고 해당 report
`/home/actions/actions-runner/_work/_temp/python-rust-523-capacity.BByoAp` 하나를 정리한 뒤에만
출력한다. 별도 remote FS census를 실행한 것이 아니며 그 이상 회수 범위를 주장하지 않는다.

따라서 **원본 인증·20.55MB 원문 bytes·투명 as-is API roundtrip·두 receipt·owned cleanup**은
이 실제 run/source 범위에서 수용할 수 있다. 새 primary consumer ID는 자동 적용하지 않았다.
현 helper가 요구하는 원래 ID/run/head 인증은 그대로라, 향후 보존 archive 연결에는 별도 검토된
transport binding이 필요하다. #476 원인 해결·fullsix credit·모델/ABI/기기 수용은 여전히 별개다.

### 실제 expiry 관측으로 드러난 후속 source blocker

run.created_at은22:44:29Z, 실제 payload created_at은22:59:41Z, proof는22:59:50Z다.
**두 artifact의 expires_at은 모두2027-01-23T22:44:30Z로 run.created_at+90일+1초다.**
따라서 queue가10분을 넘은 이번 실제 run에서도 첫 게시본의 run-anchor guard는 통과했다.
이 metadata 관측은 앞선 잠재 거부 가정을 이 run에서 입증하지 않으며, 반대로 후속
artifact-created guard와의 불일치를 드러낸다.

| 자료 | created_at→expires_at 실제 초 | artifact 생성 후 정확한90일 하한보다 부족 |
| --- | ---: | ---: |
| payload | 7775089 | 911초(15분11초) |
| proof | 7775080 | 920초(15분20초) |

후속 source39800의 WF SHA`b7b83241...`는 이 실제 metadata를 받으면 두 새 하한 중 첫 payload
검사부터 거부한다. 이것은 oldrun의 실패가 아니라 **새 guard와 이미 관측한 API 값의 source 불일치**다.
API에90일 configured retention이 기록되고 upload 입력도90일인 사실과, 각 artifact 생성 후
정확히90×86400초를 보장하는 주장은 구별한다. 실제 만료일은 위 표의 값을 그대로 사용한다.

부모 리뷰용 최소 수정 후보는, 이번 실제 서비스가 반환한 run-anchor 보존 계약을 채택할 경우,
두 expiry 검사에서 원래 `run.created_at+90일 ≤ expires_at ≤ run.created_at+90일+600초`를
유지하면서 `run.created_at≤artifact.created_at`와 현재 run/head/repo/ID 결합을 추가하는 것이다.
artifact 생성 자체를 run+10분 안에 묶는 조건은 두지 않는다. 이는 configured90일의 API 계약을
관측과 맞추는 후보이며, 사용자가 artifact 생성 후 온전한90일을 별도 요구한다면 이 archive는
그 하한을 충족하지 않는다. 그 경우 source 조건을 완화해 충족했다고 선언할 수 없다.

이번 감사는 **WF를 추가 수정하거나 위 후보를 적용하지 않았다**. 부모가 게시한39800과
old972468의 구분을 유지하며, 추가 dispatch/upload·재생성·consumer pin 변경·정책/ledger 수정은0이다.
실제 run이 이미 끝나 있어 예정했던 heartbeat/재poll 자동화도 만들지 않았다.


## 실제 API에 따른 부모의 expiry 기준 정정

부모가 제안해 게시한39800의 artifact 생성 기준 하한은 잘못된 가정이었다. 실제 서비스는
이번 실행에서 run.created_at을 기준으로90일 만료를 정했으며, 원본972 run의 보존·roundtrip·
cleanup은 성공했다. 새 source는 **원래 run 기준90일 window**로 되돌리고
`run.created_at <= artifact.created_at <= expires_at`과 현재 run/head/repo/ID 결합을 추가한다.
artifact가 run+10분 안에 생성돼야 한다는 조건은 원래도 요구하지 않았고 새로 넣지 않는다.

이 정정은 실제 configured90일 계약과 만료일 `2027-01-23T22:44:30Z`를 보존한다.
각 artifact 생성 후 온전한90×86400초를 보장한다고 쓰지 않는다. 원문 ZIP bytes·90일 action
입력·same-/zip SHA·두 upload receipt·owned cleanup은 원래 실제972 run의 증거로 유지한다.
성공한 보존을 새 source로 반복 dispatch하거나 archive를 재업로드하지 않는다.

## 승인된 consumer transport 연결과 data-only 진입점 구현 — 2026-10-26

사용자 승인 뒤 기존 helper/driver WF와 새 전용 WF를 구현했다. 기준 HEAD는
`47fb84019a078f3d1c1de9c432cfe475f5b92be0`이다. 상태는 **source 구현·구문/역투영 검토 완료,
최종 helper pin 결합·execution-edge 등록·새 exact-source GHA는 부모 pending**이다.
이번 작업에서 dispatch/재실행·stage/commit/push·원본/모델/Cargo 실행·archive 재업로드는0이다.

### origin과 transport의 별도 고정

`RETAINED_OBJECTS` 원문 bytes는 바꾸지 않았다. artifact12393290469/run39700512883,
workflow source0e522와 checkout a298,20,550,808B/원문 SHA 및 bundle은 역사 origin이다.
별도 `RETAINED_TRANSPORT`는 payload12617590637/run40241602401/attempt1/source972468,
proof12618005602/9,261B/SHA
`f8d72c81b3bdb266e1e98b6d04fbfe242a9db62213c1e4a15ef727bfa337ff57`만 허용한다.
임의 ID·URL·path 입력, 최신 artifact 탐색, 원래 API로의 fallback은 없다.

`authenticate_retained_transport` 한 함수를 두 경로가 공유한다. 현재 run API의 completed/
success, source/attempt/event/workflow path/branch/repository IDs와 payload/proof 양쪽의 현재
ID/name/size/digest/expired=false를 확인한다. configured90일은 실제 run-anchor window와 현재
생존으로 검사한다. 현재 만료를 원래 artifact의14일 기한이나 artifact-created90일로 바꾸지 않는다.

proof ZIP 전체 SHA를 먼저 확인한 뒤 정확한10 member/85,039B expanded/regular/unique/
canonical/no-encryption/CRC를 요구한다. 원본 ZIP도 정확한258 member/37,492,774B expanded와
전체 SHA/CRC를 검증한다. 원본90 object의 존재와 기존 receipt의 checkout/model SHA도 확인한다.
90개 image와 object의 과거 독립 일치 증거는 위 실제 audit로 유지한다. data-only에서 native
image 재구성·ISA/ABI admission·수치 비교를 새로 실행했다는 주장은 하지 않는다.

proof의 `SOURCE_COMMIT=a298…`, `WORKFLOW_COMMIT=0e522…`는 payload의 원래41B member와
byte 동일해야 한다. origin API/run/binding/roster/roundtrip을 원문 값과 대조하며 source972나
현재 GITHUB_SHA로 덮어쓰지 않는다. **`preservation-run.json`은 실제 in_progress/null snapshot**을
그대로 요구한다. 완료 판정은 현재 API의 completed/success에서 얻는다. 이 snapshot은 원본 보존
run에서 최종 upload/cleanup 전에 만들어진 자료이며 완료 상태를 사후 기록한 원문이 아니다.

새 인증 receipt의 현재 `source_commit`과 nested origin/transport는 구별한다. 원본 JSON을
current source의 기록으로 다시 쓰지 않는다. raw snapshot의 권위는 고정 proof ZIP SHA다.
current API JSON은 별도 owned 파일에 그대로 보존하고 FD로 전후 결합한다.

### 기존 failure-only 경계 유지

driver WF의 fresh-status 검사 **뒤**에만 공통 함수를 호출한다. fresh exit0은 기존과 같이
다운로드 전 abort한다. fresh failure status의 sticky 반환, 원문9개 ELF, original collection44+
warp1, 모델/ABI/ISA/수학·prestate·원본 실행 본문을 바꾸지 않았다.
`frozen-authentication.json`의 기존 artifact/run/witness는 여전히 origin이며 새 `transport`만
추가했다. 기존 helper의 두 worker를 포함한 모든 기존 top-level 함수와 Candidate class의
전체 source body가 기준 HEAD와 byte 동일함을 AST 구간 대조로 확인했다.

driver WF의 변경은 auth/download block, additive receipt, 작은 transport metadata upload뿐이다.
나머지 step/job/조건과 fresh→postmortem→collection→native admission 본문은 source 역투영으로
동일함을 확인했다. 기존 push trigger도 유지했으므로 helper를 publish하면 기존 driver 진단 WF의
자동 실행 조건은 계속 성립한다. 이번 작업에서 그 모델 CI를 dispatch한 것은 아니다.

### 전용 data-only Actions 계약

새 `.github/workflows/python-rust-523-driver-transport.yml`의 job ID는 `transport`다.
push는 검토 branch의 새 WF/helper path만, dispatch는 입력 없이 허용한다. 실제 GitHub-hosted
Ubuntu24/X64/repository/ref/workflow-ref를 확인한다. 자유 ID나 모델 선택 인자는 없다.
기존 job-temp pin과 private owned-source pattern을 사용하며 SDK 설치/venv/pip/APT/Cargo/
모델 준비 단계는 없다. 전용 helper 진입점은 인자 없는 `--verify-retained-transport`다.

source checkout은 기존2GiB admission과 사후 allocated cap을 쓴다. 별도 transport/command/
proof 합계는 **128MiB**, reserve16MiB이며 같은 FS이면 checkout 증가량과 합산 admission한다.
helper가 실행 중 apparent/allocated 양쪽을 보수적으로 합산하고 마지막 WF가 job 전체의 두
크기도 검사한다. 이는 disk/file cap이며128MiB RSS 보장이 아니다. memory peak는 미측정이다.
Python 기록에는128MiB soft file limit, 기존 공통 command에는 원래512MiB OS backstop과
stdout/stderr 각32MiB 검사가 남는다. 실제 두 archive 다운로드는 각각 고정 길이의1024B 단위
올림 file limit와 exact bytes/SHA를 별도로 요구하므로 과대 응답은 거부한다.

job25분, job 시작부터 command deadline22분, 독립 finalizer90초를 둔다. metadata API3회는
각60초, payload/proof 다운로드2회는 각120초이며 기존 pidfd/subreaper/descendant 회수 함수를
재사용한다. 데이터 검증 성공은 nonzero command·EOF/시간초과·source/FD 변경·미회수 child를
덮지 않는다. 원래 helper Candidate.command body도 변경하지 않았다.

source HEAD/index/tar digest와 tracked-source stat/hash snapshot을 전후 비교한다. helper/common/
Python alias·실제 ELF/gh/bash/git/index/owner/API metadata/payload/proof는 held FD로 결합한다.
전체 tracked source는 각 read의 FD/stat/hash snapshot이며19,596개 FD를 동시에 잡고 있다고
주장하지 않는다. SDK/stdlib/DSO 전체 closure를 새로 입증했다는 주장도 없다.

`transport-authentication.json`과 data-only `outcome.json`은 transport_only=true, 실행 수0,
full_six_source_acceptance=false/credit0을 기록한다. independent final step은 source/FD seals,
child recovery, 실패 없는 finalizer와 실행 수0을 확인한다. 인증 자체가 실패하면 job은 실패하며,
안전한 회수와 봉인만 완료됐다면 실패 증거 upload 후 owned cleanup은 가능하다.

새 검증 artifact는14일 관측 자료다. 원문 payload와 proof ZIP은 **업로드하지 않는다**.
API metadata·새 receipt/FD seals/command logs만 올리고 upload ID/digest·source/index/FD/child
seals가 모두 있어야 source를 회수한다. 이어서 같은 owner-token의 job-temp를 회수한다.
기존90일 archive를 다시 보존 실행하거나 재업로드하는 경로는 추가하지 않았다.

최종 source review에서 독립 verifier의 성공 결합도 명시했다. helper step이 success이면
outcome.transport_authenticated=true/error=null과 인증 receipt의 held SHA, 현재 source,
transport-only/원문258 CRC/90 object/metadata 불변·credit0을 모두 요구한다. helper step이
failure이면 인증=false와 오류 문자열을 요구한다. skipped/cancelled/unknown을 성공으로
흡수하지 않는다. 합법적인 인증 실패의 안전한 seal/cleanup과 인증 성공 판정을 구별한다.

### 부모 최종 결합과 검증

두 WF의 `DRIVER476_HELPER_SHA256`은 부모 소유의 기존 literal
`1b69287de0a8f5045da82f008788bf79dceac2587d9f79f2a5810212ef4ee1cf`를 그대로 남겼다.
아래 최종 helper SHA로 **두 곳을 함께 교체하기 전에는 pin 검사에서 fail-closed**한다.
새 Python execution edge1(`--verify-retained-transport`), 새 WF/job1, 새 SDK/ARM/alias0의 전역
등록과 source pins/ledger는 부모가 통합한다. 새 WF의 독립 final verifier에는 stdlib-only inline
Python1도 있으며 helper 실행과 별도로 source에 나타난다. 아직 실제 수용은0이다.

부모 GHA는 공통 함수의 transport authentication, 현재 API와 고정 origin의 구분, 원본 member
bytes, 실행 수0,128MiB growth, FD/source/index 전후, upload/owned cleanup을 확인해야 한다.
잘못된 ID/head/digest/proof/origin/만료·truncated ZIP/unknown entry는 성공 skip 없이 거부해야 한다.
data-only 성공은 기존 fresh-failure/44+warp1 실행을 대체하지 않으며, 그 경로가 fresh0으로
abort했다면 retained diagnostic는 계속 미도달로 기록한다.

이번 로컬 검토는 helper AST, 두 WF YAML, Bash run12+7개의 `bash -n`, embedded Python3+1의
AST, 기존 body/원본 상수/모델 구간 byte 대조와 diff뿐이다. 공통 함수/API 다운로드/ZIP decoder/
소스 봉인 본문·프로젝트 CLI·원본/모델·GHA는 실행하지 않았다.

| 파일 | source SHA256 (부모 pin 결합 전) |
| --- | --- |
| `tools/ci/driver_retained_program_476.py` | `89c48139534f8f2566de73aee20a46403ae3b0be1b2f95674d88dad6d9f4f4fd` |
| `.github/workflows/python-rust-523-driver-retained.yml` | `44540f73dd48989075043a8880d023fafc87ac3d9a8da4d322aecd5c60039e3c` |
| `.github/workflows/python-rust-523-driver-transport.yml` | `21c5676f9fc2ac2f4822047b1ea4d3dd2fea1031753f799805db29d62a96d1bb` |

## sourceec43598 transport-only actual 감사 완료 — 2026-10-26

부모는 `ec43598c9900eae3e3d7ee28428659aba6f7e63c`에 helper/두 WF와 최종 helper pin을
게시했다. 부모 인계상 Focus354·추가 edge166 등록도 완료됐다. 본 감사에서는 해당 원장이나
source/CI를 수정하지 않고 이 절만 append했다.

[run40248425907/job121334855271](https://github.com/bin9208/openpilot-rust/actions/runs/40248425907/job/121334855271)은
attempt1/push/sourceec43598, 실제 GitHub-hosted ubuntu-24.04/X64 runner1000029782에서
completed/SUCCESS다. job 시작00:15:05Z, 완료00:15:41Z이며 인증 step7은00:15:23–00:15:32Z에
SUCCESS다. source/index after·독립 FD/child verifier·upload·source cleanup·job-temp cleanup도
각각 SUCCESS다. 원문 model/SDK/Cargo를 실행하는 step은 없다.

### 새 validation artifact와 원문 운송 분리

| 항목 | 실제 값 |
| --- | --- |
| validation artifact | `12619771888` / `driver476-transport-only-40248425907-1` |
| ZIP bytes / SHA256 | 2,480,516 / `901b2a218441ed53b9f805c23f26b8975dd99e15cea055cbce631b25a11cd55a` |
| inert ZIP 감사 | 32 safe/unique/regular/no-encryption member, expanded14,864,886B, CRC 정상 |
| job 원문 log | 56,239B / SHA `d5388aa5d3652c8896f0fb2a636d457f97f39cabffdcdb4c70d40cdbd2c7722a` |
| 인증 receipt | 33,941B / SHA `2b5ba7ec22cdbaeedb29aed9094925e49d46175690c097f53a60fb4d262b54a0` |

이번 로컬 다운로드는 위 **새 validation ZIP과 job log뿐**이다. 원래 carrier payload/proof
두 ZIP은 다시 받지 않았다. 비교에 필요한 기존 cached proof의 origin/run/roundtrip JSON만
inert하게 읽었다. cache는 `.analysis/scratch/2026-10-25-523-completion/ci/12619771888.zip`과
`40248425907-job121334855271.log`다. ZIP을 풀거나 내부 source/프로그램을 실행하지 않았다.

validation에는 원문 carrier/proof ZIP member가0개다. 두 download command의 stdout/stderr도
모두0B로, archive가 command log를 통해 재업로드되지 않았다. 관측 metadata와 새 receipt,
source/FD/children 증거만 있다.

### 실제 API·원본 metadata·held bytes 결합

원문 API JSON은 command stdout과 frozen metadata 파일 간 byte 동일하다. payload12617590637과
proof12618005602 모두 같은 preservation run40241602401/source972468/repository IDs1395501963,
각각20,550,808B/원문 SHA와9,261B/proof SHA, expired=false를 기록한다. 현재 run API는
completed/success/attempt1이고, expires_at은 양쪽 모두 `2027-01-23T22:44:30Z`다.
실제 run-anchor 보존 길이는 양쪽 모두7,776,001초(90일+1초)로 승인된 window에 든다.

다음은 새 인증 receipt와 held FD record를 독립적으로 대조한 값이다.

- payload:20,550,808B / `a318335767594bfe8bcd08878f15e4f6a03bb0d8d13541d56f3b83243b19f461`.
- proof:9,261B / `f8d72c81b3bdb266e1e98b6d04fbfe242a9db62213c1e4a15ef727bfa337ff57`.
- 두 파일 모두 regular/nlink1이며 새 인증 receipt 자체의 held SHA도 업로드 원문과 일치한다.
- `whole_zip_verified=true`, `all258_CRC_verified=true`, `original_objects=90`,
  `original_metadata_unchanged=true`다. 이 전체 원문 ZIP CRC 실행은 GHA에서 수행됐다.

receipt의 origin_witness는 원래12393290469/run39700512883/workflow0e522/checkout a298/
bytes/SHA/bundle 그대로다. origin_artifact/origin_run/preservation_roundtrip 값은 기존 cached
고정 proof JSON과 정확히 같다. proof의 preservation-run은 여전히 in_progress/null 원문이며
current completed/success를 덮어쓰지 않았다. 즉 원본41B source metadata와 역사 record를
source972나 현재sourceec로 승격하지 않았다.

### source·FD·children·자원·최종 outcome

index-before/after1,178,723B는 byte 동일하고 historical sourceec tree19,615개 entry와 일치한다.
source-before/after digest 파일도 동일하다. tracked-source FD/stat/hash snapshot은 root 포함
19,616행/각6,216,608B이며 전후 **원문 bytes 동일**이다. critical source의 snapshot SHA는
sourceec Git blob과 각각 일치한다.

| source | 실제 SHA256 |
| --- | --- |
| helper | `89c48139534f8f2566de73aee20a46403ae3b0be1b2f95674d88dad6d9f4f4fd` |
| common controller | `8a5da6caaf97dc00a0672e271130e0ce0ead8c055bee9883f7e503b9904b85a7` |
| transport WF (부모 pin 결합 후) | `b25b607e92cd8fb0152e2fbc410bfbd72bdd0698d0feb07c89137ba7c252ea8d` |
| driver WF (부모 pin 결합 후) | `bc93829f01f6f09adc841cd304955347b2d416465fcfc994b817dba235d114c9` |

held FD record17개, `source_fd_seals_passed=true`, `all_owned_children_reaped=true`다.
실제 command는 source-head, current preservation run API, payload/proof API, 두 `/zip`
download의6개뿐이며 모두 exit0/빈 stderr다. child journal12행은6개 observed PID/start와
6개 reaped PID/start가 정확히 짝을 이루고 returncode도 모두0이다. 모델/원본/컴파일러/
Cargo/SDK 설치·실행 command는 없으며 SDK 전체 봉인이나 DSO 전체 closure를 주장하지 않는다.

preflight는 source/job 동일 device66305에서 available91,728,310,272B를 확인하고 source2GiB+
transport128MiB+reserve16MiB를 합산했다. source 사후 allocated1,194,967,040B는2GiB 이하다.
transport 사후 기록은 allocated35,540,992B/apparent35,425,290B로128MiB cap 이하다.
이는 기록 시점의 disk 자료이며 RSS나 실행 전체 peak 실측으로 확대하지 않는다.

outcome은 `scope=transport-only-no-model-no-sdk`, `transport_only=true`,
`transport_authenticated=true`, `error=null`, `finalizer_failures=[]`다.
models/original/compiler/Cargo/reuploaded_archives는 모두0,
full_six_source_acceptance=false/retirement_credit0을 유지한다. 독립 verifier는 실제
AUTH_OUTCOME=success와 이 outcome/receipt held SHA를 확인한 뒤 SUCCESS다.

upload ID/digest가 원문 log/API/다운로드 ZIP SHA와 일치하고, 뒤의 두 cleanup step이 성공했다.
회수 대상은 확인한 `.ci296-source-40248425907-1.*` owner root와 `/tmp/openpilot-ci.HiK6hT`뿐이다.
다른 host/cache를 재조사하거나 전체 FS의 부재를 별도 입증한 것은 아니다.

따라서 **새 data-only transport의 실제 API/원문 bytes·provenance/CRC/source·FD·child/
disk/upload/owned cleanup 경계는 sourceec에서 수용 가능**하다. 별도 driver full 진단의
fresh-failure/44+warp1/ABI/ISA/수치 성공이나 #476 repair/fullsix/file credit는 아니다.

full retained run40248425959/job121334855336은 이번에 phase API만 한 번 읽었다. 당시 qualify
step12가 in_progress이며 first_failed는 없었다. 그 원본 log/artifact·fresh/retained 분기는
감사하지 않았다. genuine ef attempt2는 부모가 이미 감사한 결과로 유지하며 재다운로드/감사0이다.
이번 작업에서 source/CI 수정·stage/commit/push·dispatch/cancel/rerun·프로젝트/원본/모델 실행은0이다.

## Planner IPC capture2 보존 actual 감사와 stager 연결 제안 — 2026-10-26

[capacity run40250326019/job121342010745](https://github.com/bin9208/openpilot-rust/actions/runs/40250326019/job/121342010745)은
workflow_dispatch/attempt1/source`0283e6345fc8c8e60cd3537424f1c2df383d9088`에서 SUCCESS다.
실제 runner22/bin-actions/self-hosted/Linux/X64/Default에서00:44:20–00:44:56Z 실행했다.
mode는 preserve-planner-ipc-witnesses이며 inventory census/upload와 driver476 보존 단계는
모두 skipped다. Planner auth→HOST/ARM as-is upload→same-/zip readback→별도 proof upload→
세 receipt 인증/owned cleanup은 모두 SUCCESS다.
실행 capacity WF 파일의 SHA256은
`f16d6c1d2fd87177a1177c006b63f7bdd439c781154c995d8b6c1d837af3916c`다.

### 실제 fixed transport metadata

공통 repository는 bin9208/openpilot-rust, repository_id/head_repository_id는1395501963,
head_branch는 codex/feat-523-python-rust다. 새 capture2와 proof의 run/source는 위 preservation
run40250326019/source0283 전체 SHA이며 attempt1이다.

| 역할 | 새 API artifact / 정확한 name | bytes / SHA256 |
| --- | --- | --- |
| HOST private capture | `12620219669` / `planner-ipc-aa79-host-capture-12615278417.zip` | 1,241,625 / `3df5894ffe4a0ba003a670dee9978981d57656644dc16f83fb3da953db823a8d` |
| ARM private capture | `12620435668` / `planner-ipc-aa79-arm-capture-12615287431.zip` | 1,239,760 / `efa5a289799a4e1fbcd63cc8baae5f3379df20240f6c48ecad6dc68b15c5682b` |
| 공통 provenance proof | `12621030512` / `planner-ipc-witness-provenance-40250326019-1` | 57,919 / `97d499b95cee38a7472b8dc9009aeb9f1ba8513f2767b2d2a1827ebcf3213bfa` |

새3개 모두 actual API에서 expired=false이고 만료는 **2027-01-24T00:38:20Z**다.
preservation run.created_at은2026-10-26T00:38:18Z로, expires-run.created_at은7,776,002초
(90일+2초)다. HOST/ARM/proof artifact.created_at은 각각00:44:36Z/00:44:41Z/00:44:50Z다.
created_at 순서와 run-anchor90일 window는 모두 맞으며 artifact 생성 후 온전한90일을
보장한다는 의미가 아니다.

functional receipt2개는 **아래 원래 API를 그대로 유지**한다. proof 안 metadata와 별도 현재
GET API의 ID/name/bytes/digest/expired=false/run/head/repository/만료가 일치한다.

| 역할 | 기존 artifact / run / 정확한 name | bytes / SHA256 |
| --- | --- | --- |
| HOST functional | `12615802090` / `40237263524` / `rust-planner-local-x64-evidence` | 55,187,534 / `c583db165f1b5c0226dc25d2a25fb1cbba7a1fad0e282d25506f327f151cf828` |
| ARM functional | `12614614983` / `40237263421` / `rust-planner-hosted-arm64-evidence` | 44,843,072 / `2726bee5baa231c56c1856cea7bc0096b549bd6bb600cd262e04836551311798` |

두 functional의 원래 run.created_at은2026-10-25T21:49:09Z, 만료는
**2027-01-23T21:49:10Z**로7,776,001초(90일+1초)다. 그 raw ZIP은 GHA/로컬 모두 이번 보존을
위해 다시 다운로드하거나 재업로드하지 않았다. 따라서 현재 전체 input/output cohort의
소비 가능 기간은 functional 쪽 만료가 먼저이며 새 private capture 만료까지 연장된 것이 아니다.

### 원본 bytes·CRC·원문 metadata와 cleanup

기존 HOST private12615278417/run40237263524 및 ARM private12615287431/run40237263421의
source는 `aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf`, attempt1이다. 두 원본 run snapshot은
completed/success이고 origin API의 ID/name/length/digest/source/repository가 고정 plan과 같다.
원문 origins.psv는661B/SHA
`29b13c7e8da80fe11ccdbd151d5f86ca90c20f0f6f1fe303f861bc843a012742`로 변경되지 않았다.

이번 로컬 다운로드는 **새 capture2/proof와 두 job log뿐**이다. 원본4 ZIP은 재다운로드0,
CRC 반복0이다. 새 capture bytes/SHA를 확인하고 각각 cached 원본 private ZIP과 전체 byte
동일함을 확인했다. capture CRC는 로컬에서 다시 실행하지 않았다. GHA의 원문 CRC 로그에는
HOST2378/ARM2371 member의 testing 행과 No errors detected가 있고, member roster는 새 ZIP의
central-directory 이름 순서와 정확히 일치한다. 기존 전체 안전/CRC 감사의 권위는 동일 bytes로 유지된다.

새 proof만 별도로 full SHA/CRC/19 safe unique regular/no-encryption member를 감사했다.
expanded는725,259B이며 origin4 API, 원래 run2, preservation-run, plan/admission,
capture별 CRC/member list/held identity/new API/roundtrip JSON을 포함한다.
preservation-run.json은 **in_progress/null인 완료 전 snapshot**이며 사후 success로 수정하지 않는다.
현재 run의 completed/success는 실제 API에서 별도로 확인했다.

HOST/ARM roundtrip은 각 새ID·원래ID·역할·source0283/run40250326019·정확 bytes/SHA와
status=ROUNDTRIP_VERIFIED, endpoint=/zip, unwrapped=false, original_bytes_equal=true,
requested_retention_days90을 정확히 기록한다. 별도 wrapper나 source/audit record 재생성이 없다.

admission은 owner40250326019:1:inventory/report identity64521:289586:1000:41c0,
available33,586,786,304B/growth67,108,864B를 기록한다. 모델/원본 replay 및 consumer binding 변경은
false, functional metadata_only2/raw_downloads0/raw_uploads0, memory_admission=false다.
HOST/ARM payload는 uid1000/regular0600/정확 size와 inode·mtime·ctime identity를 갖는다.
메모리 peak나 전체 disk peak를 계측한 자료로 확대하지 않는다.

job 원문 log는47,163B/SHA
`80179e87209f10e09f30c687605c25012cabe1063b6b1b581bf94420562209bf`다.
마지막 step16은00:44:50–00:44:51Z SUCCESS이며 세 서로 다른 API upload receipt와 held
payload/readback SHA를 확인했다. owner·dev/inode·권한이 결합된
`/home/actions/actions-runner/_work/_temp/python-rust-523-capacity.F2NWRn`만 회수한 **뒤** 실제
PLANNER_CAPTURE_PRESERVATION_VERIFIED receipt를 출력했다. 그 receipt는 새2ID/proofID/위 proof SHA,
원문동일=true/functional metadata_only2/raw다운·업로드0/consumer_binding_changed=false다.
다른 FS/cache 정리를 주장하지 않는다.

### 부모 승인 전 최소 stager 패치 제안 (아직 source 수정0)

대상은 기존 `tools/ci/stage_planner_ipc_original_523.sh`의 인증/운송 구간이다.
origin audit2/source9/original sourceaa79/run/id/bytes/SHA와 NativeCapture reader는 그대로 둔다.
audit의 capture ID를 새ID로 바꾸거나 audit/outputs metadata를 다시 생성하지 않는다.

1. 기존 origin-run-auth의 **live API** 조회와 completed/success/sourceaa79/attempt1/repository
   검사는 유지한다. functional receipt2도 원래 API를 direct 사용하고 expired=false와 원래
   run-anchor90일을 확인한다. 새 preservation anchor로 functional의 기한을 계산하지 않는다.
2. 고정 preservation run40250326019/source0283/attempt1/WF/ref/repository를 live 인증하고,
   proof12621030512의 exact name/57,919B/SHA/expired=false/90일을 인증한다.
   기존 HTTPS302→별도 token 없는 HTTPS GET 경계를 재사용해 fixed proof ZIP만 받는다.
3. proof 전체 SHA/CRC와 정확한19 member를 확인한 뒤 알려진 metadata만 bounded unzip-p로
   owned proof 경로에 읽는다. 원본 capture identity는 해당 host/arm-capture-api.json을 불변
   audit/plan의 원래 ID/run/source/length/SHA/name에 대조한다. proof의 원본 artifact가 보존 시점에
   유효했음을 확인하되 **old private artifact 현재 API의 expired=false에 영구 의존하지 않는다**.
   그 API 실패를 무조건 허용하는 fallback이나 origin-run live 검사를 생략하는 방식은 아니다.
4. capture role만 새 fixedID/name/run/source0283/repository/bytes/SHA/현재 생존·run-anchor90일을
   검사하고 proof의 preserved-api/roundtrip과 결합한다. proof snapshot의 in_progress/null은
   그대로 두며 완료 판정은 live preservation API로만 한다. `root/capture.zip`에는 여전히 원문과
   같은 bytes를 받는다. receipt role의 API/download 경로는 변경하지 않는다.
5. 기존 URL/token 분리, owned path/CRC, capture2349 파일·expansion, functional allowlist489
   (summary1+case4+raw484), 최종 member pins·identity·128MiB cap을 모두 유지한다.
   추가 proof/작은 API JSON도 같은 cap 안에 합산한다. 기존 phase/status/export 계약은 유지하고
   운송 provenance는 별도 proof 파일에 추가한다. 새 Python/helper/원본 실행·임의 ID/자동 fallback은 없다.

새 transport 소비자 source/GHA 검증과 실제 stager source SHA는 아직 없다. Locke가 진행 중인
WF stageHASH 동기화와 충돌하지 않도록, 구현 승인 뒤 마지막 stager SHA는 부모가 전달한다.
보존 actual PASS를 소비자 연결·모델/IPC 기능 또는 Python 퇴역 credit로 자동 승격하지 않는다.

### ad1e same-helper transport 추가 API/log 확인

[run40250641545/job121341555065](https://github.com/bin9208/openpilot-rust/actions/runs/40250641545/job/121341555065)은
source`ad1e35cf6813dd352f700907489f2edc31e1482e`/attempt1에서 SUCCESS다. API의 인증·source/index/
FD verifier·upload·두 cleanup step이 모두 SUCCESS이며 원문 log는56,224B/SHA
`3d320cb5b9e529c77da3a2bd3cc1ab8019c94e84436c59b4801734984c514f7a`다.
helper pin89c481 전체 SHA와 실제 source check OK, AUTH_OUTCOME=success를 확인했다.
이는 API/log 범위 확인이며 artifact나 carrier를 새로 다운로드해 재감사하지 않았다.

이번 작업은 본 문서 append와 inert API/새자료 감사뿐이다. 원본4 ZIP 재다운로드/CRC 반복,
기능/모델/원본 실행·CI 재실행·source/stager/WF/이슈 수정·stage/commit/push는0이다.

## Planner IPC stager fixed90d transport 연결 구현 — 2026-10-26

부모가 위 최소 제안1–4를 승인하여 `tools/ci/stage_planner_ipc_original_523.sh` 한 파일에
구현했다. 최종 stager SHA256은
`43e1eb7b2b5e27521f8f7ce4e4a36c35376515b4be944c6bd390a348978810b6`다.
새 Python/CLI/helper/테스트 파일, WF/profile/env/권한 변경은 없다.

고정 preservation run40250326019/source0283/attempt1/정확 capacity WF·branch·repository/head
identity·created_at을 live API에서 인증하고 completed/success를 요구한다. proof12621030512와
HOST12620219669/ARM12620435668의 ID/name/bytes/SHA/URL/head·repository identity를 literal에
결합했다. 새 transport의 실제 만료2027-01-24T00:38:20Z와 run-created90일 window/현재 생존을
동시에 요구한다. 입력 ID/URL 변경이나 fallback, 기한 연장은 없다.

proof ZIP의 고정 전체 SHA/CRC, 정확19개 canonical unique 이름, regular0600/unx 및
expanded725259B를 검사한다. 전체를 풀지 않고 admission/preservation-run/해당 origin-run/
해당 old capture API/new capture API/roundtrip/functional API의 **JSON7개만** 각각64KiB로
제한해 create_new0600/canonical/단일 link 경로로 읽는다. 고정 proof의 원래 capture metadata를
immutable audit의 original ID/run/source/bytes/SHA/name에 대조하며, 보존 시점에 원본이
유효했는지를 따로 검사한다. **old private artifact의 현재 API 조회는 하지 않는다.**

original run live API 인증 블록은 byte 그대로다. 원문 `capture-api.json`은 과거 metadata로
유지하고 현재 운송 API를 `capture-transport-api.json`에 분리한다. 완료 전
preservation-run snapshot의 in_progress/null도 그대로 인증하며 현재 run 완료 상태로
덮어쓰지 않는다. admission의 replay=false/consumer_binding_changed=false/functional raw0과
roundtrip의 old/new ID·역할·bytes/SHA/source/run/90일·/zip·unwrapped=false·bytes_equal=true를
모두 AND로 요구한다. 저장한 API identity는 새 live API와 대조한다.

functional HOST12615802090/ARM12614614983은 원래 ID/source/run으로 direct API/download한다.
기존 필드를 모두 보존하면서 proof snapshot과 exact identity를 대조하고, 원래 run-anchor90일과
실제 만료2027-01-23T21:49:10Z를 확인한다. 따라서 functional 쪽의 더 이른 소비 기한은 유지된다.
URL Authorization→302와 token 없는 HTTPS download 분리는 기존 코드 경계 그대로 재사용한다.
원래120s/110s/64MiB download 및 CRC 제한도 유지한다.

private2349 추출, functional allowlist489(summary1+case4+raw484), source9/audit2/output2,
기존 최종 pins/source/owner, allocated/apparent du128MiB, status/export 블록은 보존했다.
추가 proof와 JSON도 같은128MiB 입장·실제 du 합산에 포함한다. proof ZIP/선택 JSON의 hash를
최종 export 전 다시 확인한다. 별도 transport-authentication receipt는 운송 검증만 기록하며
native_revalidation_required=true/full_ipc_acceptance=false를 유지한다.

로컬 검증은 bash -n, JQ32개 program compile-only(미호출 함수 안에 식을 정의하여 policy 평가0),
소스 byte 역투영과 diff --check뿐이다. 기존 original-run 인증/private 추출·489 allowlist/
최종 pins·du·status·export의 byte 동일성을 확인했다(새 최종 proof 검사와 cap 주석 제외).
실제 stager/네트워크/프로젝트/원본 기능 실행, CI dispatch/rerun, stage/commit은0이다.
이 새 소비자 구현의 실제 GHA 성공 및 추가 기능·퇴역 credit는 아직 없다.

부모/Locke가 위 최종 SHA로 동기화할 소비자는 다음과 같다. 이 작업에서 해당 파일은 수정하지 않았다.

| 소유자 확인 대상 | stager pin 소비 위치 |
| --- | --- |
| 외부 WF2 | `.github/workflows/python-rust-523-planner-caller.yml`, `.github/workflows/python-rust-523-native-arm-more.yml`의 planner-runtime/planner_ipc_stage |
| canonical WF2 | `.github/workflows/rust.yml`, `.github/workflows/rust-native-arm.yml`의 같은 planner-runtime/planner_ipc_stage |
| canonical source contract | `rust/crates/dev-tools/tests/ci_rust_isolation.rs::planner_ipc_canonical_steps`의 literal SHA |
| focused source guard | `.github/workflows/python-rust-523.yml`의 focused/ssh_absence_admission stager SHA |

교체 전 literal은 `578ec6f73765ca4293174eeb7eb2a5df99a9af6ff726f262c7aa3f17f65c97bf`다.
부모의 다음 exact-source GHA에서 HOST/ARM 각각의 live origin+preservation API, proof19/JSON7,
운송/functional CRC, allowlist489/native 재검증, source·held·disk·owned cleanup을 확인해야 한다.

## source6c HOST/ARM 최초 실패 감사 — 2026-10-26

source `6c5d5a9be691d18ca850af6ef9b42e47b668d767`/attempt1의
[HOST40255341871/job121355876469](https://github.com/bin9208/openpilot-rust/actions/runs/40255341871/job/121355876469)과
[ARM40255341883/job121355876763](https://github.com/bin9208/openpilot-rust/actions/runs/40255341883/job/121355876763)은
각각 hosted ubuntu-24.04/X64 및 ubuntu-24.04-arm/ARM64에서 **실패**했다.
HOST job은01:40:15–01:43:51Z, ARM job은01:40:17–01:44:43Z다.
ARMmore run 전체에는 별도 WebRTC job이 있으므로 해당 run 전체 상태와 Planner job 실패를 혼동하지 않는다.

최초 실패는 HOST step17/ARM step15의 compile/check 안 `planner_ci_policy`다.
그 앞 `native_build_closure`7/`planner_ci_controller`14는 양쪽 모두 PASS다.
policy21은1 passed/20 failed이고 공통 실패 지점은
`rust/crates/dev-tools/tests/planner_ci_policy/contract.rs:137`의 helper SHA literal이다.

- source6c helper 실제 SHA:
  `cf510c5bb65b5fdd5bcad2356f988e026d64f8ad7d52c6117e49efcf082f1612`
- contract가 요구한 stale SHA:
  `df5eab6cbfecac8e0d5de52140fac9c8d3b006d1f5c2072db19bde77d238022c`

양쪽 원문 log의 left/right 및 git6c helper bytes의 SHA가 정확히 일치한다. 종료는101이다.
최소 수정 제안은 부모/Locke가 위 contract literal을 최종 검토 helper bytes에 동기화하는 것이다.
이 감사에서는 source/CI를 수정하거나 재실행하지 않았다.

그 결과 HOST step18–21/ARM step16–19의 stager fixed transport 새 소비자·runtime
비교/lifecycle collector·sanitizer·IPC qualification은 모두 **skipped**다.
transport3 인증/collector26 run·input234·Params30·source14·초기 상태/회복·송신 bytes·출력72·
64MiB/128+64MiB·추가 worker/valid-data 비교0·SDK 전후 봉인 성공으로 승격할 근거는 없다.
이 source의 유효 lifecycle raw index/manifest SHA·size는 관측하지 못했으며 다음 nativeLife용
HOST/ARM 외부 audit manifest2는 생성하지 않는다. 앞선 source의 자료를 이 capture로 대체하지 않는다.

실패 뒤 functional/private upload2, source/index 사후 seal, 소유 source alias/target cleanup 및
common job-temp cleanup은 두 job API에서 모두 SUCCESS다. source seal 명령은 index-before/after와
source-before/after의 cmp를 실행했다. helper runtime 자체가 미도달하므로 그 SDK before/after
봉인을 실행한 것으로 해석하지 않는다.

아래는 현재 API와 원문 upload log가 일치하는 **실패 자료 carrier metadata**다.
공통 head_sha는 source6c, repository_id/head_repository_id는1395501963,
head_branch는 codex/feat-523-python-rust, expired=false다. 두 run.created_at은
2026-10-26T01:40:11Z이고 네 artifact의 expires_at은2027-01-24T01:40:11Z로 정확 run-anchor90일이다.

| 역할 | ID / 정확한 name | bytes / API·upload SHA256 |
| --- | --- | --- |
| HOST functional | `12622606189` / `rust-planner-local-x64-evidence` | 4693521 / `2d70c37b1fe245db545bf8723ba5738c745680b001f8b69e8da5cf471e43764a` |
| HOST private | `12622491387` / `private-rust-planner-runtime-0-40255341871-1` | 292776 / `1bf3be114fbbce94c833480462efad6b8751a945cd6e4de8aa5878a75552ae71` |
| ARM functional | `12622680819` / `rust-planner-hosted-arm64-evidence` | 3860472 / `a7b5e73cb6cc77c3d0ea69ff36f684190152bb00b57bd8daed7a483b16dc3c9f` |
| ARM private | `12622651044` / `private-rust-planner-runtime-0-40255341883-1` | 292739 / `44814ff8cc7e53a4efc8e900512810c6324c5ab11322117064d99daccd73cf1c` |

네 ZIP의 유일 downloader/전체 SHA·CRC 담당은 Bacon이다. 이 감사자는 다운로드/CRC 반복0이며
위 digest는 로컬 ZIP 재계산 결과로 표시하지 않는다. 원문 job log만 읽었고 HOST212873B/SHA
`23518855b09586f18710dfe7206f1a4549558f222bc4a82aec84dc70f7dc53d8`, ARM201598B/SHA
`e55148ab4641a7b61644e94b118ccf89da6107ffd2138d7e683f2297ba59e18c`다.
stager/collector actual 수용·새 원본 관측·nativeLife closure·퇴역 credit는 모두0으로 유지한다.

## HOST57/ARM69 consumer 미도달 확인 — 2026-10-26

HOST source`57e1761c5cac9ddee282550d8c079083e6fa9a5a`의
[run40256938654/job121360705093](https://github.com/bin9208/openpilot-rust/actions/runs/40256938654/job/121360705093)은
step17 compile/check 실패이며 stager18/runtime19/qualification21은 skipped다.
이 job 최초 오류 상세는 Bacon 담당으로 두고 이 감사에서는 중복 조사하지 않았다.

ARM source`69d5489463e22a9ef4245cc941a5788f795a8b99`의
[run40257284064/job121361745718](https://github.com/bin9208/openpilot-rust/actions/runs/40257284064/job/121361745718)은
hosted ARM64에서02:04:33–02:09:20Z 실행 후 step15 compile/check 실패다.
build-closure7/controller14 PASS 뒤 policy21은1 passed/20 failed다. 실제 최초 공통 오류는
`planner_ci_policy/contract.rs:353`의 planner-runtime functional count(actual7/expected6)다.
같은 source의 canonical HOST/ARM YAML을 정적으로 읽으면 원래6개에 native IPC qualification이
추가되어 functional step7개가 정확히 일치한다. 최소 제안은 FAMILIES의 planner-runtime count6만7로
동기화하며 기존 전체 step object 동일성·SDK/order·negative 검사를 유지하는 것이다. 수정하지 않았다.

ARM stager16/runtime17/qualification19는 모두 skipped다. collector2/stager/helper의 bytes는
git57=git69임을 확인했지만 이 동일성을 actual 실행 증거로 사용하지 않는다. source6c/57/69의
capture 데이터·성공·nativeLife 외부 audit manifest를 채우지 않으며 새 credit도0이다.

ARM의 upload/source-index 사후 seal/owned source cleanup/common temp cleanup은 SUCCESS다.
아래 실패 carrier2는 API/upload log identity가 일치하고 expired=false다. repository/head ID는
1395501963, head source는 위69 전체 SHA다. run-created2026-10-26T02:04:28Z에서
expires2027-01-24T02:04:28Z까지 정확90일이다.

| 역할 | artifact ID / name | bytes / API·upload SHA256 |
| --- | --- | --- |
| ARM functional | `12623480113` / `rust-planner-hosted-arm64-evidence` |3860473 / `1d6a185137904362cce55ea6dbae83804a4cebd48aa81bc275e6c0a9f7318a42` |
| ARM private | `12622638687` / `private-rust-planner-runtime-0-40257284064-1` |292739 / `880ce37016717251c9b8f37ffe9321cc6f3c3a202230236f6dfc59df9678ddb8` |

ARM job 원문 log196822B/SHA
`4cc4f2e59dcaf7fd874abbe27bdb3dea7d41770b791bf720b9aa0a34aa6ead67`를 읽었다.
artifact ZIP 다운로드·CRC 반복0, source/CI 수정·rerun0이며 Bacon의 유일 downloader 역할을 유지한다.

## source841 lifecycle actual 감사와 독립 stager 준비 — 2026-10-26

source `841ee8a4e1357e7a0db4193cf7944e23c31ee0fb`/push/attempt1의
[HOST40258459503/job121365332760](https://github.com/bin9208/openpilot-rust/actions/runs/40258459503/job/121365332760)과
[ARM40258459472/job121365332586](https://github.com/bin9208/openpilot-rust/actions/runs/40258459472/job/121365332586)은
전체 SUCCESS다. HOST는02:18:37–02:27:46Z, ARM은02:18:38–02:28:18Z의 hosted native 실행이다.
stager→runtime/lifecycle→IPC qualification은 모두 실제 PASS다. sanitizer는 원래 조건대로
HOST에서 PASS/ARM에서 skipped이며 ARM sanitizer 성공으로 확대하지 않는다.
6c/57/69의 collector 미도달 자료를 채우거나 재분류하지 않는다.

### 실제 archive4와 원문 관측

Bacon의 `ci/planner841-artifact-handoff.json` ready/whole_crc_pass=true를 재사용했다.
네 ZIP의 유일 다운로드·전체 SHA/CRC/안전성 감사는 Bacon이며 이 감사자는 전체 CRC를 반복하지 않았다.
아래 digest는 API/upload/handoff와 일치한다. 공통 repository/head ID1395501963,
head branch codex/feat-523-python-rust, source841, expired=false다.
두 run-created2026-10-26T02:18:33Z에서 expires2027-01-24T02:18:33Z까지 정확90일이다.

| 역할 | ID / 정확한 name | ZIP bytes / SHA256 | Bacon member/expanded |
| --- | --- | --- | --- |
| HOST functional |12623801541 / rust-planner-local-x64-evidence |58488480 / `cb4b4b562fa5b143c32cb3b88d8e0c8879e2fc7066646a957cd01747b841bd3b` |41687 /174338679 |
| HOST private |12624056023 / private-rust-planner-runtime-0-40258459503-1 |2217291 / `0ca520bbecfdc7ea35b09b914d042ee7ad6006efe6b33f7d9c9fbb59871bee5f` |85 /11878869 |
| ARM functional |12623646303 / rust-planner-hosted-arm64-evidence |48144861 / `5288aea003bdf02852307217587617aadcba90f017a1f1b581fec70deae4db81` |37084 /145301622 |
| ARM private |12623531667 / private-rust-planner-runtime-0-40258459472-1 |1891965 / `9228ad7acd660cf47a5e63498aa401fe6aba3cb21b9a5550cfcabb6778f69056` |74 /10519278 |

각 functional에서 index→run manifest→raw의 exact bytes/SHA와 완전 파일 roster를 inert하게
대조했다. 양쪽 모두 **26 run/input234/Params30/output72/source14/capture 파일5791**이며
capture tree에 빠지거나 추가된 파일이 없다. capture ZIP member는 regular0600이다.
source14 SHA는 git841 blob과 전부 일치하고 receipt/cases/각 run receipt도 manifest에 결합된다.

13 case의 source/rust 순서와 종료 상태는 원래대로다. wait-missing/dir/empty의 prelaunch 상태,
recover-directory의 prelaunch 및 recovery-before dir→recovery-after nonempty file,
settings-directories4, fatal 숫자 literal `invalid`, malformed CP `x`를 모두 확인했다.
정상4case×2mode×3frame의 output72는 원래 frame/service/순서와 receipt에 맞는다.
source wait/SIGINT=-2·Rust0, ready/recover SIGTERM=-15, 숫자 오류 source=-6·Rust1,
malformed 양쪽1을 명시적으로 유지한다. valid/data 값에 새 합격 조건을 붙이지 않았다.

input234는 frame/service별 원문 SHA와9개 송신 순서에 결합된다. 실제 producer/PubMaster source
SHA를 통해 기존 `to_bytes()` 한 번의 bytes를 저장하고 같은 bytes를 send하는 경로를 확인했다.
추가 original/model/worker 호출은 없고 기존 lifecycle26 안의 관측이다. 12개 controller command가
모두0이며 lifecycle leaf는 한 번이다. 원본 packet을 native 계산으로 다시 생성하거나 보완하지 않았다.

| 실제 기록 | HOST | ARM |
| --- | --- | --- |
| capture index bytes/SHA |14119 / `280539324781d356a4f5e03bd19582128e1a8c22090b65a80156f7074e592f23` |14127 / `f1d02d1c17623dc527db57ff4c5fde2279ef05d16018ed0999c9c860d6d495dc` |
| index 전 payload/apparent/allocated |2109325 /2695053 /26189824 |2109368 /2695096 /26189824 |
| index 포함 capture payload |2123444 |2123495 |
| 선택5891 파일 payload 합계 |2174197 |2174248 |
| collector free admission bytes |86247878656 |110156296192 |

collector는 기존128MiB+capture64MiB를 입장시켰고 위 사용량은64MiB 이하다.
COMPLETE index 생성 후 다시 실제 member identity/hash/du를 검사하는 producer 경로가 성공했다.
표의 du는 index 전 기록이며 전체 실행 peak/RSS 실측으로 확대하지 않는다.

HOST root는 `tmp/openpilot-ci.afCEBf/planner-comparison/lifecycle-original-inputs`,
ARM root는 `tmp/openpilot-ci.s6xgRN/planner-comparison/lifecycle-original-inputs`다.
각 root의 `index.json` 및 sibling `lifecycle/{receipt,cases}.json`, run receipt26/원문 output72의
실제 path/bytes/SHA만 외부 audit에 넣었다. audit는 strict `lifecycle_corpus.rs::Audit` 필드에 맞고
artifact.archive는 각각 `12623801541.zip`/`12623646303.zip`, extracted는 `functional`이다.
이는 새 stager bundle의 명시적 배치이며 과거 원본 경로를 바꾼 metadata가 아니다.

private의 source/index before/after가 byte 동일하다. SDK manifest159개(HOST)/158개(ARM)의
runtime/IPC qualification 전후 SHA·roster·stable roster가 일치하며 HOST memory도 같다.
capture의 SDK proof hashes가 private 실제 member와 일치한다. 보존된 실제 binary/solver2/manifest도
index identity7의 해당 hash와 맞는다. 보존하지 않은 Python/Params/msgq 바이너리 bytes 자체의
재감사를 주장하지 않고 producer 전후 pin 자료를 유지한다. 모든 owned cleanup step은 SUCCESS다.

기존 IPC fixed transport도 실제 수용했다. 원래 aa79 ID/run/source와0283 보존 carrier/proof는
분리되어 있고 원래 capture API snapshot과 현재 carrier API, in_progress/null 보존 snapshot과
live success를 각각 유지한다. source841 transport-authentication은 roundtrip/bytes_equal=true다.
원래 IPC 입장128MiB 안에서 HOST allocated80478208/apparent69672575B,
ARM68882432/58074125B를 기록했다. 새 LC를 그 root에 합치거나 cap을 늘리지 않는다.

### 부모 승인된 새 data-only stager

새 `tools/ci/stage_planner_lifecycle_original_523.sh`는
`$CI_JOB_TEMP/planner-lifecycle-original`과 `$CI_JOB_TEMP/proof/planner-lifecycle-stage`만 만든다.
독립128MiB 입장/allocated·apparent 합산, 원래10분 stage 안의300초 deadline,
fixed841 run/source/ID/name/ZIP bytes/SHA/실제 SUCCESS/run-anchor90일을 요구한다.
원래 IPC stager/그128MiB cap과 WF/helper/기능 step7은 수정하지 않았다.

API Authorization과 token 없는 blob download를 분리하며 redirect 후 token env도 제거한다.
payload 요청 없이 actual API Location header만 읽어 확인한
`productionresultssa5.blob.core.windows.net`/`productionresultssa10.blob.core.windows.net`만 허용하고
userinfo/port/fragment는 거부한다. ZIP whole SHA/CRC·전체 canonical unique nonsymlink roster를
확인한 뒤 index/run manifest의 인증된 hash 사슬로 exact5891 allowlist만 추출한다.
uid700/600·single-link·file count/size·각 SHA·source/owner 전후·실제 du를 확인한 뒤에만
`PLANNER_LIFECYCLE_CORPUS`와 `PLANNER_LIFECYCLE_CORPUS_AUDIT` 두 env를 export한다.
바이너리/SDK를 추출하거나 Python/원본/프로젝트 CLI를 실행하지 않는다.

| 부모/Ohm 고정 대상 | bytes | 최종 SHA256 |
| --- | --- | --- |
| tools/ci/stage_planner_lifecycle_original_523.sh |17552 |`258943202a9a19f64e8fd5e1188156206c7d2e5a998690a1a0540ab1a718d84e` |
| tests/fixtures/planner-lifecycle-original-audit/x86_64.json |36135 |`9ae47494dcf2e201fa1c48a70e6acd934e08fefc008001bd0c95c9ba502eb88e` |
| tests/fixtures/planner-lifecycle-original-audit/aarch64.json |36136 |`23bace6376cc1d87a1857bc8f69b64d6f0ddf5c526e1f69a133502620bd81f80` |

fixture 실제 root는 `rust/crates/dev-tools/tests/fixtures/`다. 위 audit bytes를 그대로 복사했으며
새 expected/native 계산값은 없다. Ohm은 reader의 stale producer6c를841로 바꾸고 이 두 pin을
결합해야 한다. 부모는 기존 stage step에서 oldIPC 다음 새 stager를 호출하고 timeout10분/step7을
유지한다. 새 stager 및 native lifecycle integration 실제 성공은 아직 없으며 퇴역 credit0이다.

로컬은 inert data/SHA 감사, Bash syntax, JQ13개 compile-only(식 평가0), 신규 diff 공백 검사뿐이다.
새 stager·원본/모델/프로젝트 기능 실행·GHA dispatch/rerun·stage/commit은0이다.

### 발행 전 빈 directory 운송 결함 수정 — 2026-10-26

부모가 실제 functional ZIP 두 개 모두 directory entry가0임을 발견했다. metadata-only 재확인에서
원래26 manifest는 inputs26/Params30을 요구하며, input이 없는16 run의 빈 directory는 파일5891개
선택 추출만으로 생기지 않는 것을 확인했다. 실제 원본 자료의 결함이나 native roster 검사 문제가
아니라 이전 stager2589432의 directory 복원 누락이다. 아직 새 stager 실제 GHA는 없었다.

`stage_planner_lifecycle_original_523.sh`만 수정했다. 기존 SHA 인증을 통과한 각 manifest의
run index/epoch/source·rust mode/ASCII case name에서 case path를 검증하고, Params label은
prelaunch 또는 recover의 prelaunch/recovery-before/recovery-after만 허용한다. 이 metadata에서
inputs26+Params30의 정확56개 subdirectory와 root1+case26+subdirectory56의 전체83개 roster를
만든다. 파일 추출 후 누락된 항목만 `mkdir -m 700`으로 만들며 wildcard/mkdir-p/새 raw 생성은 없다.
기존 directory도 canonical/owned UID/mode700/nosymlink를 요구하고 전체83개 exact roster를 대조한다.

selected 파일5891/capture 파일5791와 각 bytes/SHA, source841 audit2 bytes, 기존128MiB cap,
300초 deadline, core의 directory roster 검사는 그대로다. 추가 directory도 기존 최종 du 합산에
포함한다. 기존 IPC stager·다른 source/fixture/WF는 수정하지 않았다.

새 최종 stager는19577B/SHA256
`2b10c879782740c5662580febc84faac1806661f85d8512cb74691cbd9c74f48`다.
부모는 앞선2589432 pin을 이 값으로 동기화해야 한다. Bash syntax/JQ15개 compile-only·diff 검사는
PASS이며 식 평가·stager/프로젝트 기능·CRC 반복·원격 Actions·stage/commit 실행은0이다.
