#!/usr/bin/env bash
# 실제 aa79 IPC 원문만 준비한다. 모델/원본/컴파일러/NativeCapture를 실행하지 않는다.
# 호출: GITHUB_TOKEN(actions:read)을 가진 별도 Actions step에서 /usr/bin/bash로 인자 없이 실행.
set +x
set +o posix # Bash ulimit -f의1024B 단위를 고정한다.
set -euo pipefail
set -C
umask 077
[[ $# == 0 ]] || { echo 'Planner IPC stager takes no arguments' >&2; exit 64; }
[[ ${GITHUB_ACTIONS:-}:${GITHUB_REPOSITORY:-}:${RUNNER_OS:-} == true:bin9208/openpilot-rust:Linux ]]
[[ ${GITHUB_SHA:-} =~ ^[0-9a-f]{40}$ && ${GITHUB_RUN_ID:-} =~ ^[1-9][0-9]*$ &&
   ${GITHUB_RUN_ATTEMPT:-} =~ ^[1-9][0-9]*$ && ${GITHUB_JOB:-} =~ ^[A-Za-z0-9_-]+$ ]]
for tool in curl jq sha256sum unzip timeout stat realpath df head wc id find cat git uname awk; do
  test -x "/usr/bin/$tool"
done
token=${GITHUB_TOKEN:?GITHUB_TOKEN with actions:read is required}
uid=$(/usr/bin/id -u)
arch=$(/usr/bin/uname -m)
case "$arch:${RUNNER_ARCH:-}" in
  x86_64:X64)
    audit_sha=384bcef1bafc962019557d7b89ef5dac4c85ee2d1683906c1942458b04118e54
    receipt_name=rust-planner-local-x64-evidence
    capture_expanded=2022221
    ;;
  aarch64:ARM64)
    audit_sha=7dd3abad75e472a7241d16143b155dd3ad6e8bb826359bd0afedca1360689efc
    receipt_name=rust-planner-hosted-arm64-evidence
    capture_expanded=2022235
    ;;
  *) echo 'Actual native Linux X64 or ARM64 runner is required' >&2; exit 1 ;;
esac
job=${CI_JOB_TEMP:?}
[[ $job == /tmp/openpilot-ci.?????? && ${job%/*} == /tmp ]]
owner=$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB
owned_dir() {
  test -d "$1" && test ! -L "$1"
  test "$(/usr/bin/realpath -e -- "$1")" = "$1"
  test "$(/usr/bin/stat -c '%u:%a' -- "$1")" = "$uid:700"
}
owned_dir "$job"
owned_dir "$job/proof"
test -f "$job/owner" && test ! -L "$job/owner"
test "$(/usr/bin/stat -c %u -- "$job/owner")" = "$uid"
(( $(/usr/bin/stat -c %s -- "$job/owner") <= 512 ))
test "$(/usr/bin/cat "$job/owner")" = "$owner"
job_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job")
parent_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job/proof")
owner_identity=$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$job/owner")
source_dir=${DIAG_SOURCE:-${CI296_SOURCE:?DIAG_SOURCE or CI296_SOURCE is required}}
if [[ -n ${DIAG_SOURCE:-} && -n ${CI296_SOURCE:-} ]]; then test "$DIAG_SOURCE" = "$CI296_SOURCE"; fi
[[ $source_dir == /* && -d $source_dir && ! -L $source_dir ]]
test "$(/usr/bin/realpath -e -- "$source_dir")" = "$source_dir"
test "$(/usr/bin/git --no-optional-locks -C "$source_dir" rev-parse HEAD)" = "$GITHUB_SHA"
test "$(/usr/bin/realpath -e -- "${BASH_SOURCE[0]}")" = "$source_dir/tools/ci/stage_planner_ipc_original_523.sh"
source_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$source_dir")
fixture=$source_dir/rust/crates/dev-tools/tests/fixtures/planner-ipc-original-audit/$arch.json
test -f "$fixture" && test ! -L "$fixture"
test "$(/usr/bin/realpath -e -- "$fixture")" = "$fixture"
(( $(/usr/bin/stat -c %s -- "$fixture") <= 1048576 ))
fixture_identity=$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$fixture")
# 신뢰 시작점은 위 script literal이다. 현재 JSON의 계산값을 새 pin으로 사용하지 않는다.
printf '%s  %s\n' "$audit_sha" "$fixture" | /usr/bin/sha256sum --check --strict -
root=$job/planner-ipc-original
proof=$job/proof/planner-ipc-stage
test ! -e "$root" && test ! -L "$root"
test ! -e "$proof" && test ! -L "$proof"
mkdir -m 700 -- "$proof"
printf '%s' "$owner" > "$proof/owner"
proof_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof")
phase=allocate
failed() {
  local code=$?
  trap - EXIT
  if ((code != 0)) && [[ -d $proof && ! -L $proof &&
      $(/usr/bin/realpath -e -- "$proof") == "$proof" &&
      $(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof") == "$proof_identity" ]]; then
    /usr/bin/jq -n --arg phase "$phase" --arg owner "$owner" --arg source "$GITHUB_SHA" \
      --argjson code "$code" '{status:"FAILED", phase:$phase, owner:$owner,
        source_commit:$source, exit_code:$code, corpus_exported:false}' > "$proof/failure.json" || :
  fi
  exit "$code"
}
trap failed EXIT
mkdir -m 700 -- "$root" "$root/capture" "$root/receipt"
printf '%s' "$owner" > "$root/owner"
root_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root")
capture_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root/capture")
receipt_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root/receipt")
verify_owner() {
  owned_dir "$job"; owned_dir "$job/proof"; owned_dir "$root"; owned_dir "$proof"
  owned_dir "$root/capture"; owned_dir "$root/receipt"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job")" = "$job_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job/proof")" = "$parent_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root")" = "$root_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof")" = "$proof_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root/capture")" = "$capture_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root/receipt")" = "$receipt_identity"
  test -f "$job/owner" && test ! -L "$job/owner"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$job/owner")" = "$owner_identity"
  for marker in "$job/owner" "$root/owner" "$proof/owner"; do
    test -f "$marker" && test ! -L "$marker"
    test "$(/usr/bin/stat -c %u -- "$marker")" = "$uid"
    test "$(/usr/bin/cat "$marker")" = "$owner"
  done
}
phase=admission
verify_owner
test "$(/usr/bin/stat -c %d -- "$root")" = "$(/usr/bin/stat -c %d -- "$proof")"
available=$(/usr/bin/df -PB1 -- "$root" | /usr/bin/awk 'NR == 2 {print $4}')
[[ $available =~ ^[0-9]+$ ]] && ((available >= 128 * 1024 * 1024))
/usr/bin/jq -n --arg owner "$owner" --arg source "$GITHUB_SHA" --arg arch "$arch" \
  --arg job "$job_identity" --arg root "$root_identity" --arg proof "$proof_identity" \
  --argjson available "$available" '{owner:$owner, source_commit:$source, architecture:$arch,
    job_identity:$job, root_identity:$root, proof_identity:$proof,
    available_bytes_before:$available, admitted_growth_bytes:134217728,
    memory_admission:false, original_execution:false}' > "$proof/admission.json"
(
  ulimit -f 1024
  /usr/bin/cat "$fixture" > "$root/audit.json"
)
printf '%s  %s\n' "$audit_sha" "$root/audit.json" | /usr/bin/sha256sum --check --strict -
/usr/bin/jq -e --arg arch "$arch" '
  .schema_version == 1 and .kind == "audited-original-planner-ipc-capture" and
  .source_commit == "aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf" and
  .host_architecture == $arch and .origin_job == "planner-runtime" and .origin_run_attempt == "1" and
  (.artifacts | keys) == ["capture","receipt"] and
  .artifacts.capture.archive == "capture.zip" and .artifacts.capture.extracted == "capture" and
  .artifacts.receipt.archive == "receipt.zip" and .artifacts.receipt.extracted == "receipt" and
  .index.path == "planner-ipc-original-inputs/index.json"
' "$root/audit.json" > /dev/null
origin_run=$(/usr/bin/jq -er .origin_run_id "$root/audit.json")
origin_source=$(/usr/bin/jq -er .source_commit "$root/audit.json")
[[ $origin_run =~ ^[1-9][0-9]{0,18}$ ]]
api=https://api.github.com/repos/bin9208/openpilot-rust
get_json() (
  ulimit -f 1024
  test ! -e "$2" && test ! -L "$2"
  exec {fd}>"$2"
  status=$(/usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
    --connect-timeout 10 --max-time 30 --max-filesize 1048576 \
    --header "Authorization: Bearer $token" --header 'Accept: application/vnd.github+json' \
    --output "/dev/fd/$fd" --write-out '%{http_code}' "$api/$1")
  test "$status" = 200
)
phase=origin-run-auth
get_json "actions/runs/$origin_run" "$proof/origin-run.json"
/usr/bin/jq -e --arg run "$origin_run" --arg source "$origin_source" '
  (.id | tostring) == $run and .run_attempt == 1 and .head_sha == $source and
  .status == "completed" and .conclusion == "success" and
  .repository.full_name == "bin9208/openpilot-rust" and .repository.id == 1395501963 and
  .head_repository.full_name == "bin9208/openpilot-rust" and .head_repository.id == 1395501963
' "$proof/origin-run.json" > /dev/null
for role in capture receipt; do
  phase=$role-api-auth
  verify_owner
  artifact=$(/usr/bin/jq -er ".artifacts.$role.id" "$root/audit.json")
  bytes=$(/usr/bin/jq -er ".artifacts.$role.bytes" "$root/audit.json")
  digest=$(/usr/bin/jq -er ".artifacts.$role.sha256" "$root/audit.json")
  [[ $artifact =~ ^[1-9][0-9]{0,18}$ && $bytes =~ ^[1-9][0-9]{0,8}$ && $digest =~ ^[0-9a-f]{64}$ ]]
  ((bytes <= 64 * 1024 * 1024))
  get_json "actions/artifacts/$artifact" "$proof/$role-api.json"
  name=$receipt_name
  if [[ $role == capture ]]; then name=private-rust-planner-runtime-0-$origin_run-1; fi
  /usr/bin/jq -e --arg id "$artifact" --arg name "$name" --argjson bytes "$bytes" \
    --arg digest "sha256:$digest" --arg run "$origin_run" --arg source "$origin_source" '
    (.id | tostring) == $id and .name == $name and .size_in_bytes == $bytes and
    .digest == $digest and .expired == false and (.workflow_run.id | tostring) == $run and
    .workflow_run.head_sha == $source and .workflow_run.repository_id == 1395501963 and
    .workflow_run.head_repository_id == 1395501963
  ' "$proof/$role-api.json" > /dev/null
  phase=$role-download
  redirect=$(/usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
    --connect-timeout 10 --max-time 30 --header "Authorization: Bearer $token" \
    --output /dev/null --write-out '%{http_code}\n%{redirect_url}' "$api/actions/artifacts/$artifact/zip")
  [[ $redirect == $'302\nhttps://'* ]]
  url=${redirect#*$'\n'}
  [[ $url != *$'\n'* && $url != *$'\r'* ]]
  (
    ulimit -f 65536
    test ! -e "$root/$role.zip" && test ! -L "$root/$role.zip"
    exec {fd}>"$root/$role.zip"
    /usr/bin/timeout --signal=TERM --kill-after=5s 120s \
      /usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
      --connect-timeout 10 --max-time 110 --max-filesize "$bytes" "$url" |
      /usr/bin/head -c "$((bytes + 1))" >&"$fd"
  )
  test -f "$root/$role.zip" && test ! -L "$root/$role.zip"
  test "$(/usr/bin/stat -c '%u:%a:%s' -- "$root/$role.zip")" = "$uid:600:$bytes"
  printf '%s  %s\n' "$digest" "$root/$role.zip" | /usr/bin/sha256sum --check --strict -
  phase=$role-crc
  (
    ulimit -f 16384
    /usr/bin/timeout --signal=TERM --kill-after=5s 120s \
      /usr/bin/unzip -t "$root/$role.zip" > "$proof/$role-crc.txt"
  )
done
phase=private-extraction
verify_owner
# 외부 pin→감사 JSON→exact ZIP SHA가 기존 safe unique path/regular-file/CRC 감사를 결합한다.
# 일반 ZIP이나 전체 functional archive를 풀지 않는다. 생성한 private root만 사용한다.
receipt_member=$(/usr/bin/jq -er .receipt.path "$root/audit.json")
[[ $receipt_member == tmp/openpilot-ci.??????/planner-comparison/ipc/receipt.json ]]
/usr/bin/timeout --signal=TERM --kill-after=5s 120s /usr/bin/unzip -q -n \
  "$root/capture.zip" 'planner-ipc-original-inputs/*' -d "$root/capture"
/usr/bin/timeout --signal=TERM --kill-after=5s 30s /usr/bin/unzip -q -n \
  "$root/receipt.zip" "$receipt_member" -d "$root/receipt"
for dir in "$root/capture" "$root/receipt"; do owned_dir "$dir"; done
bad=$(/usr/bin/find "$root/capture" "$root/receipt" -xdev \
  \( ! -uid "$uid" -o \( -type d ! -perm 0700 \) -o \( -type f ! -perm 0600 \) \
     -o \( ! -type f ! -type d \) \) -print -quit)
test -z "$bad"
test "$(/usr/bin/find "$root/capture" -xdev -type f -printf '.\n' | /usr/bin/wc -l)" = 2349
test "$(/usr/bin/find "$root/receipt" -xdev -type f -printf '.\n' | /usr/bin/wc -l)" = 1
actual_expanded=$(/usr/bin/find "$root/capture" -xdev -type f -printf '%s\n' | /usr/bin/awk '{n+=$1} END {print n}')
test "$actual_expanded" = "$capture_expanded"
phase=final-pins
for role in capture receipt; do
  digest=$(/usr/bin/jq -er ".artifacts.$role.sha256" "$root/audit.json")
  bytes=$(/usr/bin/jq -er ".artifacts.$role.bytes" "$root/audit.json")
  test "$(/usr/bin/stat -c '%u:%a:%s' -- "$root/$role.zip")" = "$uid:600:$bytes"
  printf '%s  %s\n' "$digest" "$root/$role.zip" | /usr/bin/sha256sum --check --strict -
done
for role in index receipt; do
  extracted=capture
  if [[ $role == receipt ]]; then extracted=receipt; fi
  member=$(/usr/bin/jq -er ".$role.path" "$root/audit.json")
  bytes=$(/usr/bin/jq -er ".$role.bytes" "$root/audit.json")
  digest=$(/usr/bin/jq -er ".$role.sha256" "$root/audit.json")
  file=$root/$extracted/$member
  [[ $bytes =~ ^[1-9][0-9]{0,6}$ && $digest =~ ^[0-9a-f]{64}$ ]] && ((bytes <= 1048576))
  test -f "$file" && test ! -L "$file"
  test "$(/usr/bin/realpath -e -- "$file")" = "$file"
  test "$(/usr/bin/stat -c '%u:%a:%s' -- "$file")" = "$uid:600:$bytes"
  printf '%s  %s\n' "$digest" "$file" | /usr/bin/sha256sum --check --strict -
done
verify_owner
test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$source_dir")" = "$source_identity"
test "$(/usr/bin/git --no-optional-locks -C "$source_dir" rev-parse HEAD)" = "$GITHUB_SHA"
test "$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$fixture")" = "$fixture_identity"
printf '%s  %s\n' "$audit_sha" "$fixture" "$audit_sha" "$root/audit.json" | /usr/bin/sha256sum --check --strict -
/usr/bin/jq -n --arg source "$GITHUB_SHA" --arg origin "$origin_source" --arg run "$origin_run" \
  --arg owner "$owner" --arg arch "$arch" --arg audit "$audit_sha" --arg root "$root" '{
    status:"STAGED_FOR_NATIVE_REVALIDATION", source_commit:$source, origin_source:$origin,
    origin_run:$run, owner:$owner, architecture:$arch, audit_sha256:$audit, root:$root,
    capture_members:2349, receipt_members:1, original_execution:false,
    native_revalidation_required:true, full_ipc_acceptance:false
  }' > "$proof/status.json"
phase=export
test -f "${GITHUB_ENV:?}" && test ! -L "$GITHUB_ENV"
test "$(/usr/bin/stat -c %u -- "$GITHUB_ENV")" = "$uid"
# 마지막 쓰기다. 다음 step의 NativeCapture가 두 archive와 각 member를 다시 검증한다.
printf 'PLANNER_NATIVE_IPC_CORPUS=%s\nPLANNER_NATIVE_IPC_AUDIT=%s\nPLANNER_NATIVE_IPC_AUDIT_SHA256=%s\n' \
  "$root" "$root/audit.json" "$audit_sha" >> "$GITHUB_ENV"
