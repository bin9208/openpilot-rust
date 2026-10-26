# #523 aa79 Planner IPC private capture 보존 mode

2026-10-26. 상태: **source 구현·문법/diff 검토 완료, 새 보존 mode는 UNRUN**.
작업 기준 capacity 파일은 `.github/workflows/python-rust-523-capacity.yml`, 기존 SHA256은
`5650d6ad33f5b7746d9177dfa0d8cf9c03da8eb29b105d61312aae62fec09c8b`다.
완성 WF는60242B/SHA256 `f16d6c1d2fd87177a1177c006b63f7bdd439c781154c995d8b6c1d837af3916c`다.
부모가 source pin/ledger/발행/dispatch를 소유한다. 이 작업에서 실제 GHA·업로드·stage/commit/push는0이다.

먼저 source47fb의 HOST/ARM 실제 qualification을 완료했고 결과는 기존
`python-rust-523-planner-ipc-reader-next.md` 끝에 append했다. 두 arch의 unit4/strict Clippy/stager/
순차 native four-run·기존 baseline·160 scheduler·봉인/정리의 수용과 이 **미실행 보존 mode**는 구별한다.
`evidence-retention.md`는 Hypatia의 동시 편집을 위해 이 보존 작업에서 수정하지 않았다.

## 보존 범위 정정과 고정 원문

처음4개 ZIP 보존안을 받았으나, 부모/Ohm의 실제 API 확인에 따라 **private capture2개만**
as-is로90일 보존한다. functional2개는 이미 `2027-01-23T21:49:10Z` 만료90일 artifact다.
functional raw ZIP을 다시 받거나 올리지 않고 인증된 API metadata만 별도 proof에 보존한다.
이번 source 준비를 위해 기존 cached aa79 ZIP4개를 재다운로드하지 않았다.

공통 원래 source는 `aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf`, repo는
`bin9208/openpilot-rust`/repository_id1395501963이며 원래 run attempt는1이다.

| 역할 | 원래 artifact / run | bytes / SHA256 | 동작 |
| --- | --- | --- | --- |
| HOST private capture | 12615278417 /40237263524 | 1241625 / `3df5894ffe4a0ba003a670dee9978981d57656644dc16f83fb3da953db823a8d` | 원문 ZIP download·as-is upload·동일 ZIP API readback |
| ARM private capture | 12615287431 /40237263421 | 1239760 / `efa5a289799a4e1fbcd63cc8baae5f3379df20240f6c48ecad6dc68b15c5682b` | 원문 ZIP download·as-is upload·동일 ZIP API readback |
| HOST functional receipt | 12615802090 /40237263524 | 55187534 / `c583db165f1b5c0226dc25d2a25fb1cbba7a1fad0e282d25506f327f151cf828` | metadata only, raw download/upload0 |
| ARM functional receipt | 12614614983 /40237263421 | 44843072 / `2726bee5baa231c56c1856cea7bc0096b549bd6bb600cd262e04836551311798` | metadata only, raw download/upload0 |

원래 private 만료는 각각 `2026-11-08T21:54:46Z`/`2026-11-08T21:57:37Z`다.
고정4개 원문 목록은 WF literal의 origins.psv로 만들며, 별도의 고정 SHA
`29b13c7e8da80fe11ccdbd151d5f86ca90c20f0f6f1fe303f861bc843a012742`를 매 단계 확인한다.
현재 native 출력이나 새 capture에서 expected/입력 pin을 생성하지 않는다.

## 실제 구현 계약

기존 `jobs.inventory`에 closed choice `preserve-planner-ipc-witnesses`를 추가했다.
default inventory와 preserve-driver476-witness를 유지한다. 처음 공통 guard에는 새 mode의 분기만
추가했고, 기존 inventory/driver의 후속 step8개는 run/env/if/with를 포함한 전체 object가 같다.
기존 first guard도 새 mode 분기를 제거하면 원문 bytes로 돌아간다. 기존 runner/bin-actions 검증,
10분 timeout·concurrency/queue·permissions·push trigger와 inventory의 두 census 본문/14일 upload를
바꾸지 않았다. 새 job/WF/source alias/일반 controller/Python/helper는 추가하지 않았다.

보존 mode는 명시 dispatch/검토 branch와 해당 WF ref·실제 bin-actions/self-hosted/Linux/X64를
요구한다. 기존 inventory/census와 driver step은 실행하지 않는다. source compile/model/Cargo/
SDK 검사/프로세스 census/새 input 생성·원본 실행은 없다.

1. 현재 run/head/repo/attempt/WF와 원래 두 run의 source/attempt1/completed/success/repo를 인증한다.
   원문4개 artifact의 ID/name/run/head/bytes/digest/expired=false/repo IDs를 전부 인증한다.
   functional2개는 원래 run-anchor의90일 expires와 artifact.created_at 순서도 확인한다.
   **이 네 인증이 모두 끝난 뒤에만** capture2개 raw 다운로드를 시작한다.
2. 각 capture는 정확한 HTTPS API302에서 토큰 없는 별도 HTTPS GET으로 받는다.
   curl config/trace를 사용하지 않으며 signed URL/token을 proof에 저장하지 않는다.
   metadata JSON은30초/1MiB·kernel fsize, 각 capture/readback은 timeout120초/curl110초/
   expected size+1 head·2MiB kernel fsize와 pipefail로 제한한다. 경로는 fresh owned report의
   정해진 payload/provenance/roundtrip 아래뿐이다.
3. 원문 길이/SHA를 먼저 확인한 뒤 unzip-t CRC와 member roster2378/2371을 검사한다.
   absolute/backslash/빈 component/`.`/`..`와 duplicate path를 거부한다.
   special-file/no-symlink 상세 속성은 exact SHA가 결합한 기존 원문 전체 안전 감사가 근거다.
   ZIP을 추출하거나 내부 실행물을 실행하지 않는다.
4. 검토 v7 pin `cf430e030ddbb5b0abf93d22962f4752f3646cd9`의 archive:false/90일/
   overwrite=false로 다음 두 파일만 올린다. functional용 raw upload step은 없다.

   ```text
   planner-ipc-aa79-host-capture-12615278417.zip
   planner-ipc-aa79-arm-capture-12615287431.zip
   ```

5. 두 새 artifact ID/digest를 action output과 API의 현재 run/head/repo/name/bytes/digest에
   결합하고 원래와 **같은 `/actions/artifacts/<new-id>/zip`** endpoint로 읽는다.
   길이와 원문 SHA가 맞은 뒤 각각 ROUNDTRIP_VERIFIED를 쓴다. wrapper/unwrap/fallback/retry는 없다.
6. 원문/current API·run·CRC/roster·held identity·admission·readback JSON은 별도
   `planner-ipc-witness-provenance-<run>-<attempt>`90일 proof로 올린다.
   auth 이후 실패에도 이 proof upload를 시도하지만 이전 실패/미완료를 성공으로 바꾸지 않는다.
7. 새 payload2개+proof의 서로 다른 ID/digest를 API로 다시 인증하고, 모든 current run/head/repo/
   이름/길이/expired/expiry와 각 roundtrip receipt를 확인한다. payload 원문 bytes/SHA와
   uid/mode/dev-inode/size/mtime/ctime도 확인한다. proof ZIP은0 초과16MiB 이하를 요구한다.
   report의 canonical/uid700/owner/dev-inode를 다시 확인한 뒤 **그 report만** 회수하고
   PLANNER_CAPTURE_PRESERVATION_VERIFIED와 새2ID/proofID를 log/step summary에 기록한다.

proof는 자신의 upload ID/digest를 내부에 담을 수 없으므로 최종 세 upload receipt의 인증은
cleanup 이후 job summary/log와 결합한다. 초기 auth나 readback이 실패하면 성공 receipt와 cleanup은 없다.
새 artifact ID는 아직 없고 current consumer에 미리 넣는 값도 없다.

### 90일의 기준 시각

새2개와 proof의 만료 검사는 현재 preservation run의 인증된 created_at 기준이다.

```text
current_run.created_at <= artifact.created_at
current_run.created_at + 90*86400 <= artifact.expires_at
artifact.expires_at <= current_run.created_at + 90*86400 + 600
```

이는 실제 old driver 보존 run40241602401에서 확인된 run-anchor 서비스 계약을 사용한다.
queue를 artifact 생성 후 정확한90일로 잘못 결합하지 않는다. 실제 expires_at을 proof에 보존하며
artifact 생성 후 온전한90×86400초를 별도 보장한다고 주장하지 않는다. 같은 규칙으로 원래
functional2개의 기존90일 만료를 확인하되 새로 연장하지 않는다.

### 64MiB 증가량 계산과 한계

아래는 측정 peak가 아닌 operation forecast다. 실제 report FS에서64MiB available을 먼저 요구한다.

| 항목 | 보수적 bytes |
| --- | ---: |
| capture2개+readback2개 | 4962770 |
| API JSON12개×1MiB | 12582912 |
| CRC2개×2MiB+member roster2개×1MiB | 6291456 |
| 별도 proof ZIP staging 상한 | 16777216 |
| plan/receipt/identity 등 여유 | 2097152 |
| 합계 | 42711506 |
| 실제 admission | 67108864 |

512MiB 계획은 사용하지 않았다. 각 transfer/JSON/log에도 별도 fsize/길이 상한을 적용한다.
memory/cgroup admission이나 peak memory 계측은 없으며 그 성공을 주장하지 않는다.
기존 inventory/driver의 admission·기한을 바꾸는 수정도 없다.

## 부모 인계와 미실행 경계

부모가 검토 source를 발행한 뒤 기존 WF에 한 input만 주면 된다. 아래 명령은 실행하지 않았다.

```sh
gh workflow run python-rust-523-capacity.yml \
  --repo bin9208/openpilot-rust \
  --ref codex/feat-523-python-rust \
  -f operation=preserve-planner-ipc-witnesses
```

로컬에서는 YAML/Bash 문법·guard 괄호·고정 source plan/hash·기존 body 불변과 diff만 확인했다.
추가 rawfunctional 다운로드/업로드나 원래4개 ZIP 재다운로드로 이 mode를 시험하지 않았다.
기능/GHA/dispatch·취소·재실행·stage/commit/push는0이다. 새 source의 실제2 ZIP as-is/API
roundtrip·proof·run-anchor90일·cleanup 수용은 부모의 마지막 실제 실행 전까지 UNRUN이다.

NativeCapture의 원래 audit JSON/source/ID/원문 bytes와 stager6ffd는 수정하지 않았다.
현재 stager는 여전히 원래 ID를 인증하므로 private 만료 뒤에는 실패한다. 향후 실제 새2ID가
생긴 뒤에만 원래 audit와 별도 transport binding을 부모가 결합한다. 이번 보존 source 자체로
Python 퇴역/credit·모델/기기 수용이나 consumer 자동 변경을 선언하지 않는다.
