# #562 collector 원문의 helper provenance pair

2026-10-25. 이슈: https://github.com/bin9208/openpilot-rust/issues/562

f55 PR Rust40146470996/job121033363423은 첫 root-0-driver-collector-ordinary에서
정확 helper provenance를 거부했다. 기존 attestation은 collector=false/source481ae/manifest1b8e
한 pair만 수용하며 실제 collector=true/source2ecf/manifest532281은 검토되지 않은 pair였다.
현재 helper 전체 SHA10df는 기존 attestation과 같으므로 새 helper hash로 갱신할 문제가 아니다.

artifact12581536801=155,960,708B/SHA
`a55fb18ac708215b50a998b3df0204ddc09fa81a63ebf7c18e66ef14e826cc93`,1638CRC/해제213,121,609B의
invocation과 exit1/오류 원문을 확인했다. 뒤의 diagnose_first_driver_frame ENOENT는 아직 별도
미확정이며 이 provenance 수정의 성공으로 대신 수용하지 않는다.

## 원문과 최소 변경

기존 observations.tar.xz(8,254,920B/SHA
`0b4cbeb4b8c6201c150ccc85c916e5ff979b66da433c217a2f198ad4bd8c1210`)에서 정확
collector-current/observations/reference-manifest.json만 inert data로 읽었다. 원문29,876B/SHA
`532281d6682b2d5a4ae8b8acf354b3a39d04eaf98cd49cc9a2c1432be30b8795`를 그대로 fixture에
추가했다. source는 `2ecf3f6813f601ea52e7a61ccf75219952daefa0`이며 원본 실행/PKL·모델 실행,
재capture·기대값 생성은 없다.

원문의 loaded/source helper는63e8, original checker는ea1266으로 기존 pair와 동일하다.
Git의 원문 helper blob과 현재 helper의 execute/write_tensor 두 함수 source bytes 및
Python3.12 AST digest가 기존 attestation과 같은 것을 독립 data 검토로 확인했다.
현재 helper 전체 SHA와 기존9개 테스트 본문/expected는 수정하지 않았다.

attestation은 정확 (collector flag, source commit, raw manifest SHA) tuple 두 개만 인정한다.
나머지 전체 helper/function/checker pin·현재 catalog 인증·실제 manifest FD/SHA·원본 root
identity와 최종 verify를 유지한다. receipt에 실제 선택한 pair를 추가하며 전체 수치/provider
동등성이나 원본 제거 수용을 주장하지 않는다.

새 회귀3개는 collector 정확 양성,8가지 flag/source/manifest 조합 및 실제 manifest 교체,
현재 catalog 미인증/원문 checkout/전체 helper 수정/최종 FD 교체 거부를 검사한다.
기존9개와 합계12개는 기존 model-family의 driver_daemon_check:: lib selector에 연결된다.
로컬 Rustfmt/JSON/원문 byte·AST·digest/diff 확인과 독립 source review는 끝났으나 실제
컴파일·12회귀·Clippy·같은 held ELF의2root/collector 양쪽/6원문 부재 전체 검증은 **UNRUN**이다.

실제 실패를 통과해야 #562를 종료한다. baseline3580의293교체/3부분/3284대기와 #523/#296
전체 미완료를 유지하며 이번 source fixture/attestation의 retirement credit은0이다.
