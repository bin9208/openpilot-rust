# #513 이미지 PR 인자 교정의 최소 원본 관측 후보

2026-10-26 사용자 승인으로 새 WF, 이 probe, 이 문서만 작성했다. production Python/native/
기존 Rust test/Cargo/공유 pin/원장은 수정하지 않았다. **실제 두 genuine 호출과 native 근거
API admission은 GHA UNRUN**이다. #510–#512의 정상 PR500/dev 통합·post-merge 조건은 보존하며,
이 작업으로 해당 세 이슈나 #523 전체 전환을 완료 처리하지 않는다.

## 원문과 의도한 차이

- repository: `bin9208/openpilot-rust`
- 원본 path: `openpilot/tools/lib/github_utils.py`
- pre-fix commit: `a364b0b037b3a90ba4c66936013014aa88835ab5`
- immutable Git blob: `6a443b4155bc7d864bd4749935e9bd842367d5ed`
- pre-fix SHA256: `05d53b2bfa9cf73b9165b534fc1c81fa88171f9f25f1a15fc9d9009d3ca51cbc`
- 현재 corrected SHA256: `f642a6652ab7238fc2b613ca2f7722b2a4756ef0363169210564d42290aeb063`
- 기존 교정 commit: `5a26b3ffda5062039f15c9cc2f649dc33b98a81d`

원본을 다시 쓰거나 expected native preparer로 생성하지 않는다. 인증된 GitHub HTTPS contents
GET의 ref/path/type/size/blob을 고정하고, 내려받은 bytes의 SHA256과 Git blob SHA1을 모두
확인한다. 원문·API 응답을 그대로 업로드한다. 현재 파일의 AST는 원본
`comment_on_pr(table, commenter, pr_branch)`의 인자1·2를 교환한 AST와 완전히 같아야 한다.
실제로 실행할 bytes는 원문 그대로이며 AST를 실행용으로 수정하지 않는다. 명시적인
overwrite=false 유지, 기존 raw HTML/JSON·업로드 및 오류 순서를 보존한다. 원래 licensing/
provenance를 바꾸거나 원본 파일을 삭제하지 않는다.

## 두 실제 호출 후보와 네 acceptance 조건

1. private0700 proof 안의 실제 PNG 파일을0600으로 생성한다. 원래 class를 원문 compile/exec로
   정의한 뒤 `comment_images_on_pr`를 정확히 한 번 호출한다. 고정 fake requests는 bucket 조회,
   image 조회404, 실제 파일 bytes의 PUT을 받아들인 다음
   `commits/commenter-513/pulls`에404를 반환한다. 원래 class가 발생시킨 정확한 Exception과
   stderr/업로드 prefix를 기대한다. commenter와 requested branch는 서로 다른 고정 문자열이다.
2. 현재 corrected 원문도 정확히 한 번 호출한다. 동일 image·동일 raw upload prefix 뒤
   `commits/requested-branch-513/pulls` GET과 `issues/513/comments` POST를 확인한다.
   기존 odd-row HTML과 raw POST data도 literal과 비교한다. comments 조회나 PATCH는 허용하지
   않아 기존 overwrite=false 기본을 유지한다.
3. 기존 native 수용은 [40080413343/job120832600964](https://github.com/bin9208/openpilot-rust/actions/runs/40080413343/job/120832600964)의
   source `9e7a3b39ebaa2d215e2cadbae0618cc8720f3649`를 재사용한다. 그 source의 native 구현과
   test bytes를 인증 GET해 현재 두 파일과 byte 비교한다. run/job/artifact12554902305의
   고정 ID·source·attempt·digest API와 원본 job log의 실제 GithubUtils26/영향 image5 PASS를
   대조한다. **둘러싼 run/job의 failure를 success로 바꾸지 않는다.** native upload/error
   순서5그룹은 이 불변 실제 수용으로 재사용하며 Cargo/test/Clippy를 새로 실행하지 않는다.
4. 원본/현재 AST delta와 raw transcript·결과·image/현재 source 전후 identity/SHA를 함께
   남긴다. 의도적 원본 차이는 위 인자 교환 하나다. 과거 original5/image-only18 계획의 숫자를
   새 의무로 추가하지 않는다. 전체 PR나 실제 외부 댓글·계정 동작을 수용 조건으로 요구하지 않는다.

## 전송·실행·소유 범위

인증은 `prepare` 단계의 고정 repository read-only GET에만 사용한다. 준비 GET은 원본1개,
불변 native source2개, 기존 run/job/artifact metadata 및 기존 job log에 한정된다. contents
응답256KiB/decoded source64KiB, metadata 각각1MiB, log4MiB cap이다. proxy를 사용하지 않고
contents/metadata redirect는 거부한다. GitHub job-log API가 반환한 HTTPS Azure/Actions log
redirect 한 번만 허용하며 그 요청에는 Authorization을 전달하지 않는다. archive 다운로드는 없다.

`compare`는 `/usr/bin/env -i`의 고정 환경과 `/usr/bin/python3 -I -B -S`로 실행한다.
token/auth/proxy 환경은 없어야 하며 class의 token도 빈 문자열이다. 유일한 `requests`는
고정 fake module이고 headers는 정확히 빈 객체여야 한다. Python audit hook은 socket/
urllib 실제 요청·자식 프로세스 실행을 거부한다. 두 관측의 HTTP URL은 문자열로만 다루며
GitHub 서버에 POST/PUT/PATCH하지 않는다. 추가 project worker·모델 실행이나 Rust SDK/apt/pip/
build는 없다.

새 WF는 manual-only, 고정 branch의 GitHub-hosted Linux X64, job10분이다. private checkout을
사용하며 source2GiB+proof32MiB+finish reserve64MiB의 작업별 admission을 checkout 전에
실제 source/job filesystem에 적용한다. build/registry/SDK 증가량을 꾸며 넣지 않는다.
준비 step5분/비교 step1분, 파일별 cap 및 source/proof allocated·apparent 상한을 둔다.
실측 peak/fit는 GHA에서 확인해야 한다.

tracked source/index, 실제 Python 실행 파일, source-growth와 proof-growth 전후 증거를
업로드한다. Python stdlib/전체 시스템 SDK의 봉인을 주장하지 않는다. 원문·private image·
두 case의 requests.raw.jsonl(method/URL/headers/data UTF-8와 hex, 실제 fake 응답 status),
result/stdout/stderr, AST/native reuse/identity 및 phase status가 같은 artifact에 들어간다.
fake transport이므로 이를 실제 HTTP wire packet이라고 부르지 않는다.

실패를 성공으로 바꾸지 않으며 항상 source/provider 최종 봉인과 artifact 업로드를 시도한다.
초기 source admission 실패도 private proof를 남긴다. 업로드 ID/digest와 source-final 성공을
확인한 뒤에만 고정 prefix·실제 uid/dev/inode·owner marker가 맞는 private source를 회수하고,
그 성공 뒤 기존 job-temp action이 자기 디렉터리를 회수한다. 소유권 불명·업로드 실패·봉인
실패 시 cleanup은 진행하지 않는다. 회수 후 경로 관측은 실제 GHA 감사로 구분한다.

## 부모 인계와 실행 방법

WF: `.github/workflows/python-rust-523-github-images.yml`, job `github-images-513`.
같은 helper를 별도 step에서 다음 순서로 호출한다.

```text
/usr/bin/python3 -I -B -S tools/ci/github_images_513.py prepare
/usr/bin/python3 -I -B -S tools/ci/github_images_513.py compare
```

두 번째 호출은 WF에 적힌 token 없는 env whitelist를 반드시 사용한다. helper SHA를 WF의
두 호출 앞에서 고정 검증한다. 업로드 이름은 `github-images-513-<run_id>-<attempt>`다.
부모가 **새 임시 original probe execution edge의 pending 등록**, 새 WF의 source policy/
Focus·전체 source pins·원장·발행을 맡는다. source 등록 전 retirement credit은0이다.
실제 두 호출, API/log admission, 업로드된 원문/결과와 owned cleanup의 수용 감사도 부모 소유다.

로컬에서는 helper AST, 고정 Git 원문/현재 AST의 단일 교환, YAML parse·embedded Bash 구문과
diff만 확인했다. probe import/실행·원본 fetch·genuine 함수/Cargo/test/Clippy/모델·CI dispatch·
stage/commit/push는0이다. 기존 native26과 성공한 다른 검사를 재실행하지 않았다.


부모 통합에서 새 workflow는 #523 브랜치의 자기 파일/helper 변경에 한정한 push와
workflow_dispatch를 모두 허용한다. source/repository/ref 및 hosted 조건은 같다.
이 첫 실행은 push로 배정하며, 실제 결과를 관측하기 전 수용 credit은0이다.

## source0283 실제 성공·원본 감사와 #513 네 조건 판정

2026-10-26 source `0283e6345fc8c8e60cd3537424f1c2df383d9088`의
[run40250301939/job121340512293](https://github.com/bin9208/openpilot-rust/actions/runs/40250301939/job/121340512293)
attempt1 원본 artifact와 job log를 읽기 전용 감사했다. job과 모든 step은 success이며,
실제 runner는 GitHub Actions1000029788/github-hosted/Linux/X64다. 앞 절의 UNRUN은 후보
작성 당시 기록이고, 이번 실행으로 아래 두 genuine 호출과 고정 native 근거 admission을 수용했다.

원본 artifact는 [github-images-513-40250301939-1](https://github.com/bin9208/openpilot-rust/actions/runs/40250301939/artifacts/12620525221),
ID `12620525221`, ZIP370,804B다. 50개 unique/safe member의 전체 CRC가 정상이고,
expanded2,661,935B다. ZIP SHA256은 API·업로드 로그와 다음 값으로 일치한다.

```text
276f4494716db01d0afcb482e9a66c249d0d85ee478690538692a20ab3d4fd24
```

| #513 상세 acceptance | 판정과 실제 증거 |
| --- | --- |
| private image/fake transport로 기존 wrong-branch prefix 재현 | PASS. pre-fix 원문 genuine1은 bucket GET200→image GET404→PUT201 뒤 `commits/commenter-513/pulls` GET404에서 원래 Exception을 발생시켰다. 정확한 traceback·stderr와4개 raw request를 보존했다 |
| 유지되는 구현의 branch/commenter 교정 및 overwrite 결정 | PASS. corrected 원문 genuine1은 같은 업로드 뒤 `commits/requested-branch-513/pulls` GET200→`issues/513/comments` POST201을 남겼다. commenter와 branch는 서로 다르며 overwrite=false를 유지한다 |
| Actions offline transcript와 upload/error 순서 검증 | PASS. 두 실행의 처음3개 raw request가 동일하고 PUT의 base64는 실제 private PNG68B와 byte 동일하다. PNG1×1의 signature·chunk CRC도 정상이다. corrected stderr는0B이고9개 요청 모두 fake/빈 headers다. 비교 단계의 실제 network/process 시도는0이다. 기존 native26/영향 image5의 실제 수용은 아래와 같이 재사용했다 |
| 고정 원본과의 의도적 차이·licensing/provenance·다른 동작 보존 | PASS. immutable commit/blob/SHA와 업로드 원문이 일치한다. 모듈 AST 차이는 인자 교환 한 곳뿐이며 raw POST/기존 odd-row HTML을 보존한다. native 구현/test는 실제 합격 source와 byte 동일하고 source/index 전후 봉인도 동일하다. 원본 licensing/provenance 변경·삭제는 없다 |

원래 native 근거는 source `9e7a3b39ebaa2d215e2cadbae0618cc8720f3649`의
[40080413343/job120832600964](https://github.com/bin9208/openpilot-rust/actions/runs/40080413343/job/120832600964)이다.
API의 run/job/artifact12554902305 결합, raw log의 실제 GithubUtils26 PASS/영향 image5 PASS,
현재 native 구현·test와 내려받은 불변 source의 byte 동일성을 대조했다. **둘러싼 과거 job의
failure는 그대로 보존**하며 전체 job success로 올리지 않는다. 새 Cargo/test/Clippy 실행이나
과거 archive 재다운로드는 없다. native raw log SHA256은
`1d6a428e332bec1d5fc61cd798972f7d7bede1c75fd342ef01e1e302e63599f2`다.

관측 함수는 token 없는 별도 환경과 fake requests에서만 실행됐다. 인증 GET은 원문·기존
근거 준비 단계에 한정되며, 실제 GitHub POST/PUT/PATCH나 계정·댓글 작업은 하지 않았다.
`comparison.json`은 genuine_calls pre_fix1/corrected1, raw_upload_prefix_equal=true,
overwrite_default=false, network_attempts=[], native_executed=false, passed=true다.
해당 JSON SHA256은 `d2877b0ef39dca90390342572691bae195e47be7a5ec539c3cac2d3d1ea822dc`다.

source/index와 Python 실행 파일의 SHA·stat는 전후 동일하다. source tar seal은
`af0351ce6737531abaa749bc2b67e19a6d70a14d53e0b5751816698fd3b3d24f`이며,
Python3.12.3의 `/usr/bin/python3.12` digest는
`e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f`다.
준비·비교 status는 모두0이다. proof-growth 관측은 allocated2,805,760B/apparent2,661,721B로
32MiB cap 이내이고, 최종 ZIP의 전체 expanded bytes도 그 cap 이내다. 전체 시스템/Python
stdlib 봉인이나 다른 실행의 peak 보장으로 확대하지 않는다.

이번 업로드 ID/digest 확인 뒤 source cleanup과 common cleanup(step11/12)이 실제 success다.
원본 log에서 고정 private prefix·uid/dev/inode·owner marker 검사, source 회수와 기존
job-temp 회수 호출을 확인했다. cleanup의 CI_JOB_TEMP/CI_RUNNER_TEMP도 실제 값으로 존재한다.
ZIP은 회수 전 증거이며, 삭제 후 경로를 별도 probe로 다시 관측했다고 주장하지 않는다.

따라서 **#513의 명시된 네 조건은 모두 충족하며 좁은 이슈 종결이 가능하다.** 과거 계획의
original5/image-only18이나 전체 PR를 새 gate로 추가하지 않는다. 이슈 close는 부모가 수행한다.
#510–#512의 PR500/dev 통합·post-merge 조건, #523 전체 전환·원본 퇴역·기기 수용은 별도다.
새 임시 probe의 retirement credit도0이다.

원본 ZIP·job log와 한국어 판정 JSON은 private
`.analysis/scratch/2026-10-25-523-completion/ci/`의 `12620525221.zip`,
`github-images0283-121340512293.log`, `github-images0283-audit.json`에 보존했다.
job log SHA256은 `2163759a1b058a190b78af156c829e927573350df86169bcd3987c5766fe868b`다.
이 문서 추가를 위해 새 다운로드·기능/CI 실행은 하지 않았다. production source/WF·이슈 상태
수정 및 stage/commit/push는0이다.

## #513 수용 완료 뒤 임시 진단 두 경계 퇴역 — 2026-10-26

사용자 승인으로 `tools/ci/github_images_513.py`와
`.github/workflows/python-rust-523-github-images.yml`만 삭제했다. 앞 절의 실행 방법은
source0283 당시의 역사 기록이다. #513의 네 조건은 source
`0283e6345fc8c8e60cd3537424f1c2df383d9088`의 실제 두 genuine 호출과
[40250301939/job121340512293](https://github.com/bin9208/openpilot-rust/actions/runs/40250301939/job/121340512293),
artifact `12620525221`의 50-member 증거 및 기존 native26/영향5 수용으로 충족됐다.
임시 관측을 영구 기능으로 다시 포팅하거나 같은 성공 검사를 반복하지 않는다.

영구 기능과 회귀는 그대로 남겼다. `github-utils-native` feature, lib module과
`github_utils` Cargo test 등록, corrected Python 원본은 수정하지 않았다. 현재 구현과
26개 native test 함수는 수용 당시 bytes와 같으며 다음 SHA256으로 재사용 근거를 고정한다.

| 보존 파일 | SHA256 |
| --- | --- |
| `rust/crates/dev-tools/src/github_utils.rs` | `6010fb32078700cdb054303b8b5bdcbbc9c70eaf74487c6325940ec8bd6db37c` |
| `rust/crates/dev-tools/tests/github_utils.rs` | `80e0c07885d517cccca0a1c103bad4deded532c921df972be535da22c1a9e492` |
| `openpilot/tools/lib/github_utils.py` | `f642a6652ab7238fc2b613ca2f7722b2a4756ef0363169210564d42290aeb063` |

보존한 영향5개 테스트 이름은 다음과 같다.

```text
images_513_upload_prefix_requested_branch_odd_rows_and_default_post
images_513_missing_private_file_stops_before_pr_lookup
images_513_pr_lookup_failures_retain_complete_upload_prefix
images_513_put_and_comment_failures_keep_existing_effect_order
images_513_empty_commenter_or_branch_keep_false_overwrite_default
```

기존 private 캐시 `.analysis/scratch/2026-10-25-523-completion/ci/12620525221.zip`은
Git 밖의 현재 필요한 원본 증거로 보존했다. 크기는370,804B, SHA256은
`276f4494716db01d0afcb482e9a66c249d0d85ee478690538692a20ab3d4fd24`로 동일하다.
이 기록에서는 크기/SHA만 확인했고 ZIP CRC 재감사·재다운로드는 하지 않았다.
기존 원격 업로드의 retention 설정은14일이며 WF 삭제가 이를 연장하지 않는다.
따라서 원격 artifact의 계속된 다운로드 가능성을 가정하지 않고 이 캐시와 immutable
source0283·원본 job log·위 수용 기록을 함께 유지한다.

Focus의 pin2개·변경 허용 항목 및 전후 working/index absence guard는 부모가 정리한다.
원장도 기존 두 행을 지우지 않고 `removed`로 갱신하는 것이 부모 범위다. 집계 예정은
추가170개=`33 replaced / 131 pending / 1 existing / 2 text / 3 removed`이며,
**baseline credit0·Rust 교체 승격0**이다. 영구 GithubUtils나 #523 전체 전환 완료로 세지 않는다.

이번 수정은 두 파일 삭제와 이 문서 append뿐이다. source/diff·해시와 등록 보존만 확인했으며
기능/Cargo/CI 실행·현재 job 조작·새 자동화·stage/commit/push는0이다. 부모의 Focus/원장
후속 등록과 새 소스의 실제 CI 수용은 이 기록에서 완료로 주장하지 않는다.
