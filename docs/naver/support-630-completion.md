# 대단원8 #630 개발·검증·통합 완료 기록 — 2026-10-30 KST

[#630](https://github.com/bin9208/openpilot-rust/issues/630)의 선정9개는
[PR #631](https://github.com/bin9208/openpilot-rust/pull/631)로 정상 dev 통합했다.
이 기록은 지원 서비스9개의 완료 판정이며 전체 Rust 전환·AGNOS·실기 수용을 뜻하지 않는다.
계획과 상세 계측은 [기존 실행 기록](independent-support-batch-630.md)에 보존한다.

## 정확 소스와 정상 검사

| 단계 | 정확 SHA와 결과 |
|---|---|
| 검증한 PR head | `875e7a4b6172305dbc6ae4d16c78d5f02ba84fe3`, 필수5개 모두 성공, 실제 GitHub Actions app15368 확인 |
| 정상 dev 병합 | `3fd7fd051b0af237fb8f75a4260e48707e3ae7c8`, PR head와 동일 tree `79705a3b6b04c3b2859ff14adf036add4f6c7655`, 보호 규칙 우회 없음 |
| 후속 dev 검증 | `abea63adf5140cb670a1880fa2c350727a0feda9`, PR #639 통합 후 별도 push 검사 |
| 공용 CI 후보와의 대조 | #611 `c7bd7b6a514e122a6cd6ea8bba957f3ae8709297`에서도 선정8개 crate와 process-supervision의 3fd 대비 byte diff0 |

PR head의 [Rust40895279139](https://github.com/bin9208/openpilot-rust/actions/runs/40895279139)는
46/46, [Integration40895279424](https://github.com/bin9208/openpilot-rust/actions/runs/40895279424)는
16/16 성공했다. [fast40895278947](https://github.com/bin9208/openpilot-rust/actions/runs/40895278947),
[docs40895278986](https://github.com/bin9208/openpilot-rust/actions/runs/40895278986) 및
전체 ARM 필수 검사도 성공했다. 집중 controller 결과를 필수 context로 복사하지 않았다.

후속 dev abea의 별도 post는 다음과 같다.

| 검사 | 실제 결과 |
|---|---|
| [workspace](https://github.com/bin9208/openpilot-rust/actions/runs/40923802725/job/123623813213) | 성공. fresh process-child, 실제 metadata target, Wi-Fi/modem helper 연결, modem opt-in15개, workspace tests·strict Clippy |
| [전체 ARM](https://github.com/bin9208/openpilot-rust/actions/runs/40923802725/job/123623813700) | 성공. generic ARM 빌드·QEMU/PRoot 증거이며 AGNOS/실기 증거가 아님 |
| 관련 runtime | support123623813588, UI/connectivity123623813535, sensor/audio123623813158, startup services123623813503, hardware/boot123623813529, telemetry123623813609, platform123623813933, upload timeout123623813531 모두 성공 |
| [Integration40923803137](https://github.com/bin9208/openpilot-rust/actions/runs/40923803137) | 16/16 성공 |
| [fast40923802660](https://github.com/bin9208/openpilot-rust/actions/runs/40923802660) / [docs40923802623](https://github.com/bin9208/openpilot-rust/actions/runs/40923802623) | 모두 성공 |
| [Messaging40923802914](https://github.com/bin9208/openpilot-rust/actions/runs/40923802914) | X64/native ARM 모두 성공 |
| [Rust40923802725 attempt1](https://github.com/bin9208/openpilot-rust/actions/runs/40923802725/attempts/1) | **44성공/2실패**. logger123623813616과 이를 반영한 aggregate123657579727 실패 |

logger의 ASan burst 진단은 원본5회/Rust4회로 달랐다.
[#641](https://github.com/bin9208/openpilot-rust/issues/641)에 실제 실패와 artifact를 보존했다.
원인은 미입증이며 비교·sanitizer를 완화하지 않았다. 판정 시 다른 담당자가 시작한
동일 소스 attempt2의 logger123657993407은 진행 중이었다. 재실행 성공만으로 #641을 닫지 않는다.

이전의 “post 필수5개 모두 PASS 뒤 종료”라는 실행 설명은 전체 post가 모두 성공했다는
뜻으로 충족되지 않았다. 원래 #630은 선정9개의 개발·검증·통합을 완료하고 독립 새 결함은
분리하도록 범위를 고정했다. 선정 범위의 실제 post 성공·종료를 확인했으므로 그 조건으로
#630을 완료 처리하고, logger와 전체 Rust 집계의 실패를 그대로 남긴다. #641 해결이나
#598 전체 발행·설치·실기 완료를 새 종료 조건으로 추가하지 않는다.

## 개별 수용과 남은 실기

| 이슈 | 이번 완료 근거와 최종 처리 |
|---|---|
| #351 | 정확 multipart bytes·307/308·파일 오류·원본 비교와 host RSS 확인. 64MiB 입력에서 보수적 peak RSS 감소61,116KiB. 원래 범위 완료로 종료 |
| #353 | capture별 lazy scratch2개, xattr 소유권·메타데이터·errno·실패 복사 보존과 snapshot/원본 비교. 원래 범위 완료로 종료 |
| #397 | snapshot9필드·정렬·46개 의미 비교·private protocol/정책 비교. 1,024네트워크×8 할당54,672→43,720. 원래 범위 완료로 종료 |
| #439 | Cookie bytes·선택·순서와 serial/TLS/profile/notification 원본9개 비교. 1,024쿠키×64 할당196,736→832. 원래 범위 완료로 종료 |
| #363 | 실제 reap-before-signal RED와 leader 소유권·정확1회 reap·오류/취소 회귀 완료. 사용자 기기 검증이 남아 OPEN |
| #392 | 경계 RED5개/대조군3개, 샘플5,235개 보존/1,113개 의도한 수정 확인. 실제 알림 청취가 남아 OPEN. 별도 주행 조건은 추가하지 않음 |
| #394 | 세대별 prompt cleanup·overlap/private bus·기존60/90초 deadline 확인. 실제 Bluetooth 페어링이 남아 OPEN |
| #395 | leader 생존/종료 상태의 자손 정리·취소/정상 완료·private NetworkManager 확인. 실제 Wi-Fi/테더링 수용이 남아 OPEN |
| #398 | helper15개·공용 session3개·old-source EOF RED·modem14개 원본/실제 CLI 취소 확인. 실제 부팅·녹화·업로드가 남아 OPEN. 별도 modem 연결 조건은 추가하지 않음 |

호스트4건에 새로운 실기 게이트를 추가하지 않는다. 남은5건은 원래 이슈와
[#528](https://github.com/bin9208/openpilot-rust/issues/528)에서 추적한다.
host RSS·누적 할당 계측은 차량 CPU/메모리 개선이나 실제 연결·주행 결과가 아니다.
실제 기기 접속·설치·검사는 수행하지 않았다.

## 공용 CI 중복 조정과 대기 정리

사용자가 승인한 중복 조정에 따라 공용 routing/helper는
[PR #611](https://github.com/bin9208/openpilot-rust/pull/611)의 c7 source를 먼저 통합한다.
큰 workspace·전체 ARM·모델·큰 X64 oracle·AGNOS/kernel/발행은 self-hosted,
짧은 X64/native ARM 비교는 검증된 hosted 배치를 유지한다.
bin-actions2/3은 같은16CPU PC를 공유한다. 물리 PC3대로 계산하지 않는다.

중복 [PR #640](https://github.com/bin9208/openpilot-rust/pull/640)은 미병합 종료한다.
그 source `3e6d2a2c3374e66afb117040ee2e28192691a35f`와 브랜치·이력은 보존한다.
고유 APT2개 후보(dpkg 잠금600초, 명시적 cross source의 작업별 metadata 경로)는
별도 patch로 보존하며 c7에 적용하거나 새 완료 조건으로 추가하지 않았다.
#606 담당자의 native ARM/backend 연결과 기존 긴 producer를 수정하지 않았다.

사용자는 “시작 전 중복 대기만 정리”를 승인했다. 각 job이 QUEUED·runner 미배정이고
실행/완료0개임을 직전 확인한 이 작업 소유 Rust40930102526(45개),
Integration40930102386(15개), Messaging40930102338(2개), 합계62개만 취소했다.
세 run의 completed/cancelled를 직접 확인했다. 다른 채팅의 실행·대기·cache 및
Genuine40904508393/40925722174는 보존했다.

앞선 3fd post Rust40908894073과 #640 이전 후보의 취소·부분 성공은 과거 결과로
보존한다. #640의 가벼운 docs 재실행 성공을 기능 검증으로 대체하지 않는다.
최종 기록 커밋은 문서만 바꾸며 위 기능 검증 SHA와 구별한다.
