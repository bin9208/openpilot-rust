# #630 진단 multipart 중복 버퍼 제거 — #351

기존 수정 `cd3c55fa279e717bf2e4fb2795468f72c530bdc1`의 multipart 내부 변경만
재사용한다. 파일을 별도 Vec로 읽은 뒤 본문에 복사하던 경로를, 열린 File에서
최종 본문 Vec에 직접 읽는 경로로 바꾼다. 완성된 본문을 네트워크 요청 전에
만들며 파일 오류 우선순위·헤더·307/308 재전송·업로드 완료 정책을 유지한다.
folder 일관성/완료 변경 #389는 포함하지 않는다.

필수 Actions 검증:

- web-upload 전체 package tests/strict Clippy와 `check_web_upload.py`의 기존 원본99개
  비교를 유지하고, 누락 파일/open 뒤 디렉터리 읽기 실패2개를 추가해 양쪽 오류와
  HTTP 요청0회를 확인한다. 실제 HTTP·multipart·redirect 비교를 생략하지 않는다.
- 같은 의존성/빌드 설정의 기준 dev example과 후보 example을 각각 빌드한 뒤
  `check_multipart_allocation_351.py --baseline <원본-example> --candidate <후보-example>
  --output <증거경로>`를 실행한다. 서로 독립한 새 프로세스에서64MiB 파일을
  loopback 전송하고 모든 multipart bytes·Content-Length와 파일 SHA를 확인한다.
  `/usr/bin/time`의 peak RSS를 기준·후보 각각3회 교차 실행하며 helper/바이너리/source
  SHA를 보존한다. 최소 기준 RSS와 최대 후보 RSS의 차이가 제거한64MiB 중 절반인
 32MiB 이상이어야 하며 단순 RSS 잡음을 개선으로 인정하지 않는다.
  host RSS이며 CPU 또는 실기 효과로 해석하지 않는다.

로컬에서는 기능/빌드 검증을 실행하지 않는다. 정확 후보 Actions 성공·필수 PR 검사·
dev 병합 및 병합 후 검증 전에는 개발 완료로 표시하지 않는다.
