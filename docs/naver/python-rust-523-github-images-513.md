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
