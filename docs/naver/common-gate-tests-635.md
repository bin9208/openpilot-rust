# 공통 gate 회귀의 env 기준 선택 — #635

기준 소스는 `0ff1d176324a9c196df51df4e9d6f9c6e7037b9c`이며 기존 perf 작업
`9d78d07523f60a32d8a723daf4677980673382fe`는 별도 브랜치에 그대로 보존한다.
부모 main checkout은 수정하지 않는다.

실제 old606917 planner job123497008119에서 `test_plannerd_ci.py`가 fast의
첫 step에 env가 있다고 가정해 KeyError로 실패했다. runner 준비 step 추가로
순서가 바뀌었을 뿐 실제 gate나 수용 기준을 바꿀 문제는 아니다.

main `924710ae8ac25e52dace839cc9ba89f2fd0fc4e6`의 원문 두 줄을 그대로 사용해
planner는 PLANNER_MEMORY, Xiaoge는 XIAOGE_MEMORY env가 있는 step을 선택한다.
Card도 CARD env가 있는 step을 같은 단일 원소 unpack 방식으로 선택한다.
선택 대상이 없거나 둘 이상이면 실패하므로 gate 검증이 조용히 생략되지 않는다.

기존 needs·matrix assert, success/failure/cancelled/skipped/빈 값의 실행 검사,
Card의 `test "$CARD" = success` assert와 모든 fixture·workflow 실행부는 유지한다.
workflow·프로덕션 guard·환경·pin·threshold는 변경하지 않는다.

로컬은 세 Python AST와 정확한 diff만 확인했다. planner/Xiaoge selector가 main924
원문과 같은지, 이 선택자 변경을 되돌리면 세 파일이 기준 소스와 byte 단위로 같은지
대조했다. 기능·빌드·pytest·workflow 실행과 원격 조작은 하지 않았다. 새 정확 SHA의
실제 회귀 통과 여부는 부모 Actions 검증이 남아 있다.
