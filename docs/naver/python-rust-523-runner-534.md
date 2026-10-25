상위: #527(공통 CI 자원 경계), 연결 #523/#296/PR343. 60GB 증설 없이 복구·재발 방지할 범위다.

[PR343 docs40111549937/job120924154222](https://github.com/bin9208/openpilot-rust/actions/runs/40111549937/job/120924154222), source `0538007a74256ec1dbdbfe0c86d4cc5c33cefea1`에서 `Check code and user documentation changes`는 성공했다. 다음 `Test user documentation checker` 실행 중 GitHub worker가 종료됐다. 문서 정책/테스트 assertion 실패로 관측한 것이 아니다.

check-run annotation 원문은 `System.IO.IOException: No space left on device`이며 경로는 `/home/actions/actions-runner/_diag/Worker_20261024-164508-utc.log`다. worker 종료로 후속 봉인·upload·cleanup은 완료되지 않았고 job logs API는404다. 이후 runner22 `bin-actions`는 offline/busy=false, 실제 in-progress run은0으로 확인했다. 현재 free/증가 경로·남은 자식 상태는 원격 조사 전 미확정이다.

앞선 소유 캐시 회수에서 실제 free16,874,405,888B를 확인했고 Paramsd A2의 before15,011,434,496B/cleanup 전 after11,652,952,064B도 보존했다. 이 수치가 현재 여유 보장이나 docs target 단독 성장량은 아니다. 현재 user-docs workflow는 각 Cargo 단계 전2GiB 증가량만 확인하며, dev/test debug profile과 반복 성장 감시가 없다. profile 미설정이 실제 ENOSPC의 단독 원인이라는 주장은 하지 않는다.

완료 조건:
- 현재 source/입력/proof/SDK를 보존하고 실제 process·소유권·참조를 조사해 unused build cache만 정리한다. GitHub worker 정상 online 복구를 확인한다.
- 문서 checker의 기존 의미·전체 test selector·Rust 버전·필수 gate 이름을 유지하며 작업별 실제 admission/성장 중단과 소유 compiler 정리·실패 evidence/after-seal을 연결한다. guard를 낮추거나 통과 skip하지 않는다.
- 필요 없는 debug 산출물 증가를 줄이는 후보는 source 검토와 registered bin-actions의 exact-SHA 실제 docs checker/test/봉인·cleanup/용량 측정으로 검증한다. 로컬 기능/빌드는 실행하지 않는다.
- 원래 작업의 current required checks를 다시 확인하고, 증거 없는 원본 제거/전체 #523 완료를 주장하지 않는다.

2026-10-25 사용자는 60GB 증설이 어렵다고 답했고, 이어 과거 빌드·캐시를 모두 지우고 현재 문제의 빌드만 남기도록 명시했다. 해결된 작업의 결과는 Git과 이 컴퓨터의 문서에 남기며, 재현 가능한 과거 바이너리를 별도로 보존·압축할 필요가 없다. 현재 소스 변경과 현재 검증의 원본 입력은 보존한다. 이 지시는 과거 빌드 산출물을 보존하기 위한 종전의 좁은 회수 기준보다 우선한다.

이후 runner22는 `online`으로 복귀했다. 그러나 checkout·빌드·삭제가 전혀 없는 기존 read-only 용량 조사 [run40078386189 attempt2/job120936948937](https://github.com/bin9208/openpilot-rust/actions/runs/40078386189/attempts/2), source `b7647a59c6ced588969eaad77cea09448e0ef24b`도 `steps: []` 상태에서 실패했다. annotation은 `/home/actions/actions-runner/_diag/Worker_20261024-172405-utc.log`에 대한 `No space left on device`다. 온라인 heartbeat만으로 shell 실행 가능 또는 여유 공간 복구를 판정하지 않는다. 실제 free·inode 사용·잔존 프로세스·과거 build 크기는 아직 원격 측정하지 못했다.

복구 직후 과거의 대규모 검사가 먼저 실행되지 않도록, 자기 PR343의 source `0538007a74256ec1dbdbfe0c86d4cc5c33cefea1`에서 모든 job이 미시작·0단계인 두 run만 취소했다. [Integration40111549950](https://github.com/bin9208/openpilot-rust/actions/runs/40111549950)은 queued15개, [Rust40111549951](https://github.com/bin9208/openpilot-rust/actions/runs/40111549951)은 queued48개였다. 일반 cancel 요청으로 종료되지 않아 상태와 소스를 재확인한 뒤 force-cancel했고, 둘 다 실제 `completed/cancelled`를 확인했다. 실행 중인 다른 세션의 job은 종료하지 않았다. 해당 필수 검사는 미충족 상태이며 자원 복구·수정 소스의 실제 재검증이 필요하다.

현재 이 컴퓨터에는 runner SSH/콘솔 설정이 없고 runner API는 임의 shell 실행/디스크 복구를 제공하지 않는다. 실제 job 로그의 machine name도 `bin-actions`다. 기존 SSH 설정·에이전트와 연결된 Tailscale peer에서 해당 호스트를 찾지 못했다. 사용자에게 삭제 승인을 재요청하지 않고 직접 접속 방식 정보만 요청했다. 접속 후 현재 작업과 live 참조를 확인하고 승인된 과거 build·cache를 정리한다.

재발 방지 후보는 기존 docs gate·Rust1.94·두 검사 selector를 유지한다. prepared SDK를 재사용하고, debug/incremental 산출물을 줄이며, 두 Cargo 단계가 합쳐 8GiB 증가 예산을 공유하도록 한다. 512MiB 마무리 공간과 1GiB 유입 여유를 두고 100ms마다 용량을 관측하며 소유 child group 종료·wait·실패 evidence와 after-seal/upload/cleanup을 연결한다. 이 8GiB는 docs leaf에만 적용하며 전체 #296 검사나 ARM의 자원 admission을 낮추지 않는다.

독립 리뷰에서 발견한 PID/PGID 정리 문제는 소스상 수정·재검토했다. 정상 wait 뒤에는 이전 verified 값으로 KILL하지 않으며, 중단 시 현재 PPID/PGID/SID/starttime이 일치할 때만 신호를 보낸다. 소유성 또는 잔여 group의 non-live 상태가 미확정이면 recovery/cleanup 성공을 기록하지 않는다. 동결 guard SHA256은 `88e292d7718216bc16341f1ae8eedee2558ae4fdb52a2c2b4a983197f5982c12`, WF는 `c1227d57011d494cb5a71e1a33fb5fe4d9785b3f78ac387c1faea0112b55ead6`다. 현재 후보의 검증은 소스·문법·diff에 한정되고, 실패 주입·실제 docs 기능/빌드/봉인/회수는 모두 미실행이다.

### 액션 다운로드를 생략하는 응급 복구 후보

다음 source `b7ab74bf0979dce2f02ad53bf6a8ef24643c83a3`의 [setup40116974870/job120940160021](https://github.com/bin9208/openpilot-rust/actions/runs/40116974870/job/120940160021) 및 [normal40116974938/job120940160271](https://github.com/bin9208/openpilot-rust/actions/runs/40116974938/job/120940160271)은 steps0에서 실패했다. 이번 annotation은 worker 시작 뒤 `actions/checkout` tar.gz를 받을 공간이 없다는 ENOSPC다. 따라서 직접 접속 정보 확인과 병행해 `uses`·checkout·artifact 업로드가 모두 없는 inline 복구 WF를 준비한다. 아직 실제 shell 진입·삭제·공간 복구 증거는 없다.

첫 삭제 범위는 과거 공용 `/home/actions/actions-runner/_work/openpilot-rust/openpilot-rust/rust/target` 한 곳이다. 현재 docs의 private build/source·검증 원본 입력·현재 SDK는 그 밖에 남긴다. root/상위 경로의 canonical identity·소유자·동일 filesystem·하위 mount 부재를 확인하고, metadata inode 집합과 모든 UID의 cwd/exe/FD/maps 교차 참조·활성 compiler를 조사한다. 참조/미확정/변경이 있으면 삭제하지 않는다. 조건을 만족하면 정확한 literal target만 `rm -rf --one-file-system`으로 제한 시간 안에 지운다. 큰 evidence/파일 내용 hash/압축을 만들지 않고 콘솔의 작은 JSON에 before/after available bytes·inode·실제 삭제 결과를 남긴다. 이는 순간 snapshot이며 새 writer의 원자적 배제를 주장하지 않는다. timeout 뒤 부분 삭제 가능성은 실패로 기록한다.
