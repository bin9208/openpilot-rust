# #630 LPA Cookie 헤더 중간 할당 제거 — #439

기존 수정 `f903d9765c2f13e8b5ed24a0c4fad7256bb115ac`의 HTTP 헤더 helper와
standalone 할당 probe만 재사용한다. 선택된 cookie의 secure 필터·안정적인 경로 길이
정렬 이후 borrowed name/value를 하나의 String에 순서대로 붙인다. TLS·redirect·
timeout·원본 cookie 선택은 바꾸지 않는다. 새 의존성·공용 Cargo.lock 변경은 없다.

Actions에서 LPA 전체 package tests/strict Clippy, `check_lpa.py`의 원본 PTY/TLS/
프로필/notification 수명 비교를 유지한다. `rustc --edition=2021 -O -D warnings
rust/tools/lpa_cookie_header_allocation_probe.rs -o <probe>` 후
`<probe> --require-improvement`로 과거 formatting 알고리즘과 실제 production helper의
헤더 bytes 및8/1,024쿠키·64회 할당 호출/요청 바이트를 비교한다. probe는 cookie 선택의
독립 oracle 또는 과거 LPA 전체 빌드가 아니며, 전체 원본 비교를 대신하지 않는다.

과거 Actions `39526488617/job118982236423`에 집중 성공 근거가 있으나 현재 후보
성공으로 대신하지 않는다. 로컬은 문법·서식·diff만 수행한다. 정확 source의 필수 PR
및 병합 후 검사까지 확인한 뒤 #630 개발 완료를 판정하며 실기 수용은 별도다.
