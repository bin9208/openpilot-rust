## 변경 내용

-

## 빠른 검증

- 실행한 x86/변경 영역 검사와 결과:
- 정확한 SHA/Actions 링크:

## 실제 기기 수용

런타임·설치·빌드 변경은 사용자가 수정 브랜치의 현재 HEAD에서 실제 확인한 뒤 아래 값을
기록합니다. 사용자 보고를 받기 전에 `passed`를 적지 않습니다. 실패하면 수정·집중 검사·기기
재시도를 진행하며 전체 회귀 성공을 기다리지 않습니다.

Device-Tested-SHA:
Device-Environment:
Device-Build: pending
Device-Boot: pending
Device-Affected-Behavior: pending
Device-Test-Evidence:

문서/CI 정책/발행 gate만 바뀌어 기기 실행에 영향이 없는 경우 위 실기 항목 대신 사유를 씁니다.

Device-Test-Not-Needed:

## 문서

- [ ] 설정 동작이 바뀌면 한·영 가이드와 설정 Wiki 설명을 반영했습니다.

일반 코드 변경의 문서 매핑은 검토 안내입니다. 별도 요청이 없는 사용자 가이드 수정은 하지 않습니다.
설정 파일을 수정했으나 설정 동작은 그대로라면 다음 항목에 구체적인 이유를 적습니다.

Docs-Not-Needed:
