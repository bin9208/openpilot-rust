# 검증된 native 입력의 발행과 독립 Git pin — #599

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[발행 실행 #613](https://github.com/bin9208/openpilot-rust/issues/613).
구현과 검증 경로를 정리한 개발 기록이며 현재 공개 입력/설치 후보가 준비됐다는 뜻은 아니다.

genuine producer는 실제 seed/SDK/15 provider의 publication 후보를 만든다. 큰 전체 증거
artifact는 보존하고,34개 입력과 필요한4개 증거만 별도 `native-input-payload-<SHA>-<attempt>`에
저장한다. 소유 regular 파일의 hash를 확인한 뒤 hard link로 모으므로 전체 debug tree나
모델 data를 재복사하지 않는다. 별도 `native-inputs-publication-<source SHA>-<attempt>`에 index·발행 manifest·
입력 준비 summary만 저장한다. 이 작은 보고서에서 정확 run/attempt/source/index SHA를
읽는다. 대형 archive의 로컬 복사·압축 해제나 로컬 기능 검사는 필요하지 않다.

`tools/native/publish_inputs.py`와 `Native source input publication` workflow가 그 후보를
검증하고 명시적으로 발행한다. workflow를 정상 PR로 dev에 반영한 뒤 사용한다.
GitHub의 [수동 실행 조건](https://docs.github.com/en/actions/how-tos/manage-workflow-runs/manually-run-a-workflow)에
따라 workflow는 기본 브랜치에 있어야 하며 이를 위해 기본 브랜치를 바꾸지 않는다.
producer SHA와 publisher 자체 SHA는 별도로 기록한다. 다른 producer를 선택해도 해당
SHA의 Git blob에서 source profile을 읽으며 artifact의 코드는 실행하지 않는다.

## 검증 범위

- 지정 run은 같은 저장소의 정확한 genuine workflow/source에서 성공해야 한다.
  foundation 두 matrix, genuine, kernel guard와 PR Rust/workspace/ARM·integration·fast·
  mapped docs의 최신 같은 SHA 결과와 필수 job/bin-actions를 확인한다. 이전 green,
  skipped ARM, 다른 runner 또는 대기 중인 최신 검사를 대신 사용하지 않는다.
  PR 검사는 같은 저장소의 정확 head SHA와 `dev` 대상을 요구하고, 모든 필수 PR
  결과에 공통인 PR 번호·base SHA가 있어야 한다. 다른 대상의 실행은 섞지 않으며,
  같은 source여도 서로 다른 PR/기준 커밋의 성공을 합쳐 게시하지 않는다.
  이때 실행 API의 `pull_requests[].head/base`는 원래 이벤트의 근거로 쓰지 않는다.
  실제 관측에서 source run40568613889의 `head_sha`는9b118711c 그대로인데 연결된
  PR head는 다음90a6fc3d1로 변경되었고 attempt1 조회도 같았다. 명시한
  `source_pr_number`는 실행을 선택할 PR 번호이며, source/base의 근거는 아래 불변 파일이다.
  해당 source에 kernel 검사 자동 실행이 없으면 그 ref의 `native-resource-guard.yml`을
  수동 실행해 정확 SHA의 결과를 확보한다. 기존 실행을 취소하거나 재시작하지 않는다.
- [Actions artifact API](https://docs.github.com/en/rest/actions/artifacts)의 repository/run/
  source/유효 기간·byte 수·SHA-256 digest를 확인한다. 입력 전용 ZIP 전체를 검증한 뒤 bounded
  central directory와 필요한 regular member만 읽는다.
- PR Rust workspace·통합의 실제 build release·fast·mapped docs의 네 job은 검사 당시
  `GITHUB_EVENT_PATH`에서 필요한 필드만 추출하고 실제 checkout이 `GITHUB_SHA`와
  같은지 확인해 `native-validation-context-<run>-<attempt>-<job>`에 저장한다.
  검사 종료 시에도 원래 이벤트·checkout·보존한444 파일의 byte를 재확인하며 변경은 거부한다.
  PR body·이벤트 원문·환경 변수 전체는 보존하지 않는다. 실제 merge commit object의
  전체 bytes를 제한된 data로 포함하므로 shallow clone도 SHA-1과 부모 두 개를
  재검증할 수 있다. publisher는 artifact API의 run/source/attempt·만료·byte·SHA-256을
  검증하고1MiB 이하 ZIP의 단일 JSON만 읽는다. commit data를 실행하지 않는다.
  이 JSON의 원래 dev/PR/head/base/repository/runner와 실제 commit의 두 부모가 맞고,
  네 검사에 같은 PR/base가 있어야 한다. 오래된 API 연결 정보로 이를 대체하지 않는다.
  main PR에서도 원래 검사를 유지하고 증거는 남기지만 dev 입력 게시에는 사용할 수 없다.
- source run/attempt/job, 독립 index SHA, 실제 입력 선택기의 initial/reused/docs와
  전체99개 sealed generation의 engineering 증거를 확인한다. source profile과 seed
  protocol/SDK/15 descriptor를 대조해 seed2개 파일·metadata17개·component archive15개,
  총34개만 발행 대상으로 인정한다. 모델·원래 전체 runtime·captures를 추가하지 않는다.
- 각 파일의 SHA/byte를 모두 검사한 뒤에만 원격 상태를 바꾼다. 기존 공개 asset의
  내용이 다르거나 새 tag가 다른 source를 가리키면 보존하고 실패한다. 다른 발행의
  draft도 변경하지 않는다. `--clobber`나 tag retarget은 사용하지 않는다.
- 새 release는 검증된 source를 target으로 하는 draft/prerelease로 준비한다. 모든
  group의 upload/hash 검증 뒤 공개하고, 공개 HTTPS URL의 실제 전체 body를 다시
  확인한다. 잃어버린 upload 응답은 상태를 다시 읽는다. 자기 draft의0byte starter만
  정리할 수 있으며 이미 공개된/완성된 파일은 제거하거나 덮어쓰지 않는다.

ZIP8GiB·선택 파일 합계4GiB는 이 입력 발행 작업의 envelope다. 실제 추가 예상량은
artifact 크기와34개 파일의 실제 선언 크기로 계산하고 파일시스템의 여유를 검사한다.
공유 source/cache를 정리하지 않는다. JSON/ZIP directory와 streaming I/O를 제한하며
보고서의 own peak RSS를 전체 worker/kernel 또는 comma 성능으로 표현하지 않는다.

## 실행과 결과

아래는 실제 성공한 producer와 작은 보고서의 pin을 확인한 뒤 Actions에서 사용하는
형태다. 먼저 읽기 검증을 실행하고 `report.json`의 `assets_verified`, `gates_ready`,
`ready_to_publish`, release별 충돌/누락 상태를 확인한다.

```sh
gh workflow run native-input-publication.yml --repo bin9208/openpilot-rust --ref dev \
  -f source_run_id="$VERIFIED_PRODUCER_RUN" -f source_commit="$VERIFIED_PRODUCER_SHA" \
  -f source_pr_number="$VERIFIED_SOURCE_PR" \
  -f index_sha256="$VERIFIED_INDEX_SHA256"
```

실제 발행은 같은 입력에 `-f publish=true`를 추가한다. 저장소/runner/수동 실행 context를
다시 검사하고 모든 source gate와 byte 검증을 반복한다. 기본값은 읽기 검증이다.
중간 실패는 `remote_releases`와 기존 bytes를 보존한다. 같은 source/index로 다시 실행해
상태를 확인하고 계속할 수 있다. 발행 중인 작업을 관측 timeout만으로 재시작하지 않는다.

`native-input-publication-result-<run>-<attempt>`의 `report.json`과 candidate index를
읽는다. 모든 공개 URL 검증이 끝나야 `public_downloads_verified=true`와 최종
`native-bootstrap-inputs.json`이 생성된다. 이 정확 bytes를 별도 정상 PR에서
`rust/native-bootstrap-inputs.json`에 반영하고 현재 source profile/필수 검사와 병합 후
검사를 확인한다. 발행 도구는 Git 변경·runtime 설치/활성화·기기 접속을 하지 않는다.

fixed seed와 변경되지 않은 component를 매 commit마다 다시 발행하거나 pin하지 않는다.
새 입력/준비 protocol에 필요한 경우만 검증·발행한다. 전체 source entry의 실제 빌드,
정상 boot/update/복구·모델·녹화·기존 업로드와 사용자 comma 3X 수용은
[전체 테스트 안내](native-build-test-guide-598.md)의 별도 완료 조건이다.
