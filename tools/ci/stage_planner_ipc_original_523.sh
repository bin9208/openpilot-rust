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
for tool in curl jq sha256sum unzip timeout stat realpath df du head wc id find cat git uname awk; do
  test -x "/usr/bin/$tool"
done
token=${GITHUB_TOKEN:?GITHUB_TOKEN with actions:read is required}
uid=$(/usr/bin/id -u)
arch=$(/usr/bin/uname -m)
case "$arch:${RUNNER_ARCH:-}" in
  x86_64:X64)
    audit_sha=384bcef1bafc962019557d7b89ef5dac4c85ee2d1683906c1942458b04118e54
    outputs_sha=525333b68bda155b622a2e315880bcf378b7425833760ea40a259545f7703b21
    receipt_name=rust-planner-local-x64-evidence
    capture_expanded=2022221
    transport_role=host
    transport_capture=12620219669
    transport_name=planner-ipc-aa79-host-capture-12615278417.zip
    transport_bytes=1241625
    transport_sha=3df5894ffe4a0ba003a670dee9978981d57656644dc16f83fb3da953db823a8d
    ;;
  aarch64:ARM64)
    audit_sha=7dd3abad75e472a7241d16143b155dd3ad6e8bb826359bd0afedca1360689efc
    outputs_sha=a28890f6f79eb24f0018d62ec21dae5d8c2228d6b7da0f62ce33ffab48e80c80
    receipt_name=rust-planner-hosted-arm64-evidence
    capture_expanded=2022235
    transport_role=arm
    transport_capture=12620435668
    transport_name=planner-ipc-aa79-arm-capture-12615287431.zip
    transport_bytes=1239760
    transport_sha=efa5a289799a4e1fbcd63cc8baae5f3379df20240f6c48ecad6dc68b15c5682b
    ;;
  *) echo 'Actual native Linux X64 or ARM64 runner is required' >&2; exit 1 ;;
esac
preservation_run=40250326019
preservation_source=0283e6345fc8c8e60cd3537424f1c2df383d9088
preservation_proof=12621030512
preservation_proof_sha=97d499b95cee38a7472b8dc9009aeb9f1ba8513f2767b2d2a1827ebcf3213bfa
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
outputs_fixture=$source_dir/rust/crates/dev-tools/tests/fixtures/planner-ipc-original-outputs/$arch.json
test -f "$outputs_fixture" && test ! -L "$outputs_fixture"
test "$(/usr/bin/realpath -e -- "$outputs_fixture")" = "$outputs_fixture"
outputs_bytes=$(/usr/bin/stat -c %s -- "$outputs_fixture")
((outputs_bytes > 0 && outputs_bytes <= 1048576))
outputs_identity=$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%y:%z' -- "$outputs_fixture")
printf '%s  %s\n' "$outputs_sha" "$outputs_fixture" | /usr/bin/sha256sum --check --strict -
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
phase=outputs-metadata
(
  ulimit -f 1024
  /usr/bin/cat "$outputs_fixture" > "$proof/outputs-audit.json"
)
printf '%s  %s\n' "$outputs_sha" "$proof/outputs-audit.json" | /usr/bin/sha256sum --check --strict -
# compile-in 원문과 같은 metadata만 사용한다. path는 고정 summary cohort에 결합한다.
/usr/bin/jq -e --arg audit "$audit_sha" --argjson metadata_bytes "$outputs_bytes" \
  --slurpfile input "$root/audit.json" '
  def member:
    type == "object" and (keys == ["bytes","path","sha256"]) and
    (.bytes | type == "number") and .bytes > 0 and .bytes <= 1048576 and
    ((.bytes | floor) == .bytes) and (.sha256 | type == "string") and
    (.sha256 | length == 64 and test("^[0-9a-f]{64}$")) and (.path | type == "string") and
    (.path | length > 0 and length <= 256) and
    (.path | test("^[A-Za-z0-9._/-]+$")) and
    all(.path | explode[]; . >= 33 and . <= 126) and
    all(.path | split("/")[]; length > 0 and . != "." and . != "..");
  $input[0] as $a |
  ($a.receipt.path | rtrimstr("/receipt.json")) as $base |
  (keys == ["input_audit_sha256","kind","receipt_artifact_id","runs","schema_version"]) and
  .schema_version == 1 and .kind == "audited-original-planner-ipc-outputs" and
  .input_audit_sha256 == $audit and .receipt_artifact_id == $a.artifacts.receipt.id and
  ($a.receipt.path | test("^tmp/openpilot-ci\\.[A-Za-z0-9]{6}/planner-comparison/ipc/receipt\\.json$")) and
  (.runs | type == "array" and length == 4) and
  ([.runs[] | [.mode,.epoch]] == [["source",0],["source",1],["rust",0],["rust",1]]) and
  all(.runs[];
    ($base + "/" + .mode + "-" + (.epoch | tostring)) as $dir |
    (keys == ["epoch","mode","publications","receipt"]) and
    (.receipt | member) and .receipt.path == ($dir + "/receipt.json") and
    (.publications | type == "array" and length == 121) and
    all(.publications[]; member and
      ((.path | split("/") | .[:-1] | join("/")) == $dir) and
      (.path | split("/")[-1] | test("^[0-9]{4}-(longitudinalPlan|lateralPlan|driverAssistance)\\.bin$"))) and
    ([.publications[].path | split("/")[-1] | split("-")[0] | tonumber] | sort) == [range(0;121)]) and
  ([$a.receipt.path] + [.runs[] | (.receipt.path,.publications[].path)] | length == 489 and
    (unique | length == 489)) and
  ([$metadata_bytes,$a.receipt.bytes] + [.runs[] | (.receipt.bytes,.publications[].bytes)] | add) <= 8388608
  ' "$proof/outputs-audit.json" > /dev/null
(
  ulimit -f 1024
  /usr/bin/jq -r '.runs[] | (.receipt,.publications[]) | [.path,.bytes,.sha256] | @tsv' \
    "$proof/outputs-audit.json" > "$proof/output-members.tsv"
  /usr/bin/jq -r --slurpfile input "$root/audit.json" \
    '$input[0].receipt.path, (.runs[] | (.receipt.path,.publications[].path))' \
    "$proof/outputs-audit.json" > "$proof/receipt-members.txt"
)
test "$(/usr/bin/wc -l < "$proof/output-members.tsv")" -eq 488
test "$(/usr/bin/wc -l < "$proof/receipt-members.txt")" -eq 489
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
artifact_identity() {
  /usr/bin/jq -e --arg id "$2" --arg name "$3" --argjson bytes "$4" \
    --arg digest "sha256:$5" --arg run "$6" --arg source "$7" --arg api "$api" '
    .id == ($id | tonumber) and .name == $name and .size_in_bytes == $bytes and
    .digest == $digest and .expired == false and .workflow_run.id == ($run | tonumber) and
    .workflow_run.head_sha == $source and .workflow_run.repository_id == 1395501963 and
    .workflow_run.head_repository_id == 1395501963 and
    .workflow_run.head_branch == "codex/feat-523-python-rust" and
    .url == ($api + "/actions/artifacts/" + $id) and
    .archive_download_url == ($api + "/actions/artifacts/" + $id + "/zip")
  ' "$1" > /dev/null
}
retention90() {
  /usr/bin/jq -e --slurpfile run "$2" --arg expires "$3" '
    def epoch:
      if type == "string" and test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")
      then fromdateiso8601 else error("invalid UTC timestamp") end;
    ($run[0].created_at | epoch) as $anchor |
    (.created_at | epoch) as $created | (.expires_at | epoch) as $end |
    .expires_at == $expires and $anchor <= $created and $created <= $end and
    $anchor + 90*86400 <= $end and $end <= $anchor + 90*86400 + 600 and now < $end
  ' "$1" > /dev/null
}
# proof와 payload 모두 기존 token/redirect 분리·시간·크기·CRC 경계를 사용한다.
download_archive() {
  local artifact=$1 bytes=$2 digest=$3 file=$4 label=$5 redirect url
  phase=$label-download
  verify_owner
  redirect=$(/usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
    --connect-timeout 10 --max-time 30 --header "Authorization: Bearer $token" \
    --output /dev/null --write-out '%{http_code}\n%{redirect_url}' "$api/actions/artifacts/$artifact/zip")
  [[ $redirect == $'302\nhttps://'* ]]
  url=${redirect#*$'\n'}
  [[ $url != *$'\n'* && $url != *$'\r'* ]]
  (
    ulimit -f 65536
    test ! -e "$file" && test ! -L "$file"
    exec {fd}>"$file"
    /usr/bin/timeout --signal=TERM --kill-after=5s 120s \
      /usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
      --connect-timeout 10 --max-time 110 --max-filesize "$bytes" "$url" |
      /usr/bin/head -c "$((bytes + 1))" >&"$fd"
  )
  test -f "$file" && test ! -L "$file"
  test "$(/usr/bin/realpath -e -- "$file")" = "$file"
  test "$(/usr/bin/stat -c '%u:%a:%s:%h' -- "$file")" = "$uid:600:$bytes:1"
  printf '%s  %s\n' "$digest" "$file" | /usr/bin/sha256sum --check --strict -
  phase=$label-crc
  (
    ulimit -f 16384
    /usr/bin/timeout --signal=TERM --kill-after=5s 120s \
      /usr/bin/unzip -t "$file" > "$proof/$label-crc.txt"
  )
}
phase=origin-run-auth
get_json "actions/runs/$origin_run" "$proof/origin-run.json"
/usr/bin/jq -e --arg run "$origin_run" --arg source "$origin_source" '
  (.id | tostring) == $run and .run_attempt == 1 and .head_sha == $source and
  .status == "completed" and .conclusion == "success" and
  .repository.full_name == "bin9208/openpilot-rust" and .repository.id == 1395501963 and
  .head_repository.full_name == "bin9208/openpilot-rust" and .head_repository.id == 1395501963
' "$proof/origin-run.json" > /dev/null
phase=preservation-api-auth
get_json "actions/runs/$preservation_run" "$proof/preservation-live-run.json"
/usr/bin/jq -e --arg source "$preservation_source" '
  .id == 40250326019 and .run_attempt == 1 and .head_sha == $source and
  .status == "completed" and .conclusion == "success" and .event == "workflow_dispatch" and
  .path == ".github/workflows/python-rust-523-capacity.yml" and
  .head_branch == "codex/feat-523-python-rust" and .created_at == "2026-10-26T00:38:18Z" and
  .repository.full_name == "bin9208/openpilot-rust" and .repository.id == 1395501963 and
  .head_repository.full_name == "bin9208/openpilot-rust" and .head_repository.id == 1395501963
' "$proof/preservation-live-run.json" > /dev/null
get_json "actions/artifacts/$preservation_proof" "$proof/preservation-proof-api.json"
artifact_identity "$proof/preservation-proof-api.json" "$preservation_proof" \
  planner-ipc-witness-provenance-40250326019-1 57919 "$preservation_proof_sha" \
  "$preservation_run" "$preservation_source"
retention90 "$proof/preservation-proof-api.json" "$proof/preservation-live-run.json" 2027-01-24T00:38:20Z
download_archive "$preservation_proof" 57919 "$preservation_proof_sha" "$root/preservation-proof.zip" preservation-proof
phase=preservation-proof-members
# 고정 SHA로 감사한19개 canonical regular member만 허용한다. 필요한 JSON7개만 읽는다.
proof_members=(admission.json arm-capture-api.json arm-capture-crc.txt arm-capture-members.txt
  arm-capture-preserved-api.json arm-capture-roundtrip.json arm-capture.identity arm-functional-api.json
  host-capture-api.json host-capture-crc.txt host-capture-members.txt host-capture-preserved-api.json
  host-capture-roundtrip.json host-capture.identity host-functional-api.json
  origin-run-40237263421.json origin-run-40237263524.json origins.psv preservation-run.json)
printf '%s\n' "${proof_members[@]}" > "$proof/preservation-expected-members.txt"
(
  ulimit -f 64
  /usr/bin/timeout --signal=TERM --kill-after=5s 30s /usr/bin/unzip -Z -1 \
    "$root/preservation-proof.zip" > "$proof/preservation-members.txt"
  /usr/bin/timeout --signal=TERM --kill-after=5s 30s /usr/bin/unzip -Z -l \
    "$root/preservation-proof.zip" > "$proof/preservation-modes.txt"
)
/usr/bin/awk 'NR==FNR {wanted[$0]=1; next}
  !($0 in wanted) || ++seen[$0]!=1 {exit 1}
  END {for (name in wanted) if (seen[name]!=1) exit 1}' \
  "$proof/preservation-expected-members.txt" "$proof/preservation-members.txt"
/usr/bin/awk '$1=="-rw-------" && $3=="unx" && NF==10 {n++;bytes+=$4}
  END {if(n!=19 || bytes!=725259) exit 1}' "$proof/preservation-modes.txt"
selected_members=(admission.json preservation-run.json "origin-run-$origin_run.json"
  "$transport_role-capture-api.json" "$transport_role-capture-preserved-api.json"
  "$transport_role-capture-roundtrip.json" "$transport_role-functional-api.json")
for member in "${selected_members[@]}"; do
  file=$proof/preserved-$member
  # capture-api.json은 원래 metadata로 남기며 새 운송 source로 덮어쓰지 않는다.
  if [[ $member == "$transport_role-capture-api.json" ]]; then file=$proof/capture-api.json; fi
  (
    ulimit -f 64
    test ! -e "$file" && test ! -L "$file"
    exec {fd}>"$file"
    /usr/bin/timeout --signal=TERM --kill-after=5s 30s \
      /usr/bin/unzip -p "$root/preservation-proof.zip" "$member" | /usr/bin/head -c 65537 >&"$fd"
  )
  test -f "$file" && test ! -L "$file"
  test "$(/usr/bin/realpath -e -- "$file")" = "$file"
  test "$(/usr/bin/stat -c '%u:%a:%h' -- "$file")" = "$uid:600:1"
  (( $(/usr/bin/stat -c %s -- "$file") > 0 && $(/usr/bin/stat -c %s -- "$file") <= 65536 ))
  /usr/bin/sha256sum -- "$file" >> "$proof/preservation-selected.sha256"
done
phase=preservation-lifecycle
/usr/bin/jq -e --slurpfile live "$proof/preservation-live-run.json" '
  def identity: [.id,.run_attempt,.head_sha,.event,.path,.head_branch,.created_at,
    .repository.id,.repository.full_name,.head_repository.id,.head_repository.full_name];
  identity == ($live[0] | identity) and .status == "in_progress" and .conclusion == null
' "$proof/preserved-preservation-run.json" > /dev/null
/usr/bin/jq -e --slurpfile live "$proof/origin-run.json" '
  def identity: [.id,.run_attempt,.head_sha,.event,.path,.head_branch,.created_at,
    .repository.id,.repository.full_name,.head_repository.id,.head_repository.full_name];
  identity == ($live[0] | identity) and .status == "completed" and .conclusion == "success"
' "$proof/preserved-origin-run-$origin_run.json" > /dev/null
/usr/bin/jq -e --arg source "$preservation_source" '
  .status == "AUTHENTICATED_CAPTURE_ONLY" and .source_commit == $source and
  .owner == "40250326019:1:inventory" and .admitted_growth_bytes == 67108864 and
  .payloads == 2 and .functional_metadata_only == 2 and .functional_raw_downloads == 0 and
  .functional_raw_uploads == 0 and .replay_executed == false and
  .consumer_binding_changed == false and .memory_admission == false
' "$proof/preserved-admission.json" > /dev/null
# 원래 ID/run/source/bytes/SHA는 immutable audit와 대조한다. old artifact live 조회는 없다.
original_capture=$(/usr/bin/jq -er .artifacts.capture.id "$root/audit.json")
test "$(/usr/bin/jq -er .artifacts.capture.bytes "$root/audit.json")" = "$transport_bytes"
test "$(/usr/bin/jq -er .artifacts.capture.sha256 "$root/audit.json")" = "$transport_sha"
artifact_identity "$proof/capture-api.json" "$original_capture" \
  "private-rust-planner-runtime-0-$origin_run-1" "$transport_bytes" "$transport_sha" "$origin_run" "$origin_source"
/usr/bin/jq -e --slurpfile origin "$proof/origin-run.json" \
  --slurpfile preserved "$proof/preservation-live-run.json" '
  ($origin[0].created_at | fromdateiso8601) <= (.created_at | fromdateiso8601) and
  (.created_at | fromdateiso8601) <= ($preserved[0].created_at | fromdateiso8601) and
  ($preserved[0].created_at | fromdateiso8601) < (.expires_at | fromdateiso8601)
' "$proof/capture-api.json" > /dev/null
/usr/bin/jq -e --arg role "$transport_role-capture" --arg original "$original_capture" \
  --arg artifact "$transport_capture" --arg sha "$transport_sha" --argjson bytes "$transport_bytes" \
  --arg source "$preservation_source" --arg run "$preservation_run" '
  .status == "ROUNDTRIP_VERIFIED" and .role == $role and .original_artifact_id == $original and
  .artifact_id == $artifact and .sha256 == $sha and .bytes == $bytes and
  .source_commit == $source and .run_id == $run and .requested_retention_days == 90 and
  .endpoint == "/zip" and .unwrapped == false and .original_bytes_equal == true
' "$proof/preserved-$transport_role-capture-roundtrip.json" > /dev/null
for role in capture receipt; do
  phase=$role-api-auth
  verify_owner
  artifact=$(/usr/bin/jq -er ".artifacts.$role.id" "$root/audit.json")
  bytes=$(/usr/bin/jq -er ".artifacts.$role.bytes" "$root/audit.json")
  digest=$(/usr/bin/jq -er ".artifacts.$role.sha256" "$root/audit.json")
  [[ $artifact =~ ^[1-9][0-9]{0,18}$ && $bytes =~ ^[1-9][0-9]{0,8}$ && $digest =~ ^[0-9a-f]{64}$ ]]
  ((bytes <= 64 * 1024 * 1024))
  metadata=$proof/$role-api.json
  api_run=$origin_run
  api_source=$origin_source
  anchor=$proof/origin-run.json
  expires=2027-01-23T21:49:10Z
  name=$receipt_name
  saved=$proof/preserved-$transport_role-functional-api.json
  if [[ $role == capture ]]; then
    artifact=$transport_capture
    metadata=$proof/capture-transport-api.json
    api_run=$preservation_run
    api_source=$preservation_source
    anchor=$proof/preservation-live-run.json
    expires=2027-01-24T00:38:20Z
    name=$transport_name
    saved=$proof/preserved-$transport_role-capture-preserved-api.json
  fi
  get_json "actions/artifacts/$artifact" "$metadata"
  artifact_identity "$metadata" "$artifact" "$name" "$bytes" "$digest" "$api_run" "$api_source"
  retention90 "$metadata" "$anchor" "$expires"
  /usr/bin/jq -e --slurpfile saved "$saved" '
    def identity: [.id,.name,.size_in_bytes,.digest,.expired,.workflow_run,.url,
      .archive_download_url,.created_at,.expires_at];
    identity == ($saved[0] | identity)
  ' "$metadata" > /dev/null
  download_archive "$artifact" "$bytes" "$digest" "$root/$role.zip" "$role"
done
/usr/bin/jq -n --arg origin "$original_capture" --arg capture "$transport_capture" \
  --arg run "$preservation_run" --arg source "$preservation_source" --arg proof "$preservation_proof" \
  --arg sha "$preservation_proof_sha" '{status:"PRESERVED_TRANSPORT_AUTHENTICATED",
    original_capture_artifact_id:$origin, transport_capture_artifact_id:$capture,
    preservation_run_id:$run, preservation_source:$source, proof_artifact_id:$proof,
    proof_sha256:$sha, original_bytes_equal:true, roundtrip_verified:true,
    original_execution:false, native_revalidation_required:true, full_ipc_acceptance:false
  }' > "$proof/transport-authentication.json"
phase=private-extraction
verify_owner
# 외부 pin→감사 JSON→exact ZIP SHA가 기존 safe unique path/regular-file/CRC 감사를 결합한다.
# 일반 ZIP이나 전체 functional archive를 풀지 않는다. 생성한 private root만 사용한다.
receipt_member=$(/usr/bin/jq -er .receipt.path "$root/audit.json")
[[ $receipt_member =~ ^tmp/openpilot-ci\.[A-Za-z0-9]{6}/planner-comparison/ipc/receipt\.json$ ]]
/usr/bin/timeout --signal=TERM --kill-after=5s 120s /usr/bin/unzip -q -n \
  "$root/capture.zip" 'planner-ipc-original-inputs/*' -d "$root/capture"
# ZIP 중앙 목록은 이미 정한 allowlist의 존재/중복만 검사한다. 경로를 추측하지 않는다.
(
  ulimit -f 16384
  /usr/bin/timeout --signal=TERM --kill-after=5s 30s /usr/bin/unzip -Z -1 \
    "$root/receipt.zip" > "$proof/receipt-zip-members.txt"
)
/usr/bin/awk 'NR==FNR {wanted[$0]=1; next} ($0 in wanted) {seen[$0]++}
  END {for (name in wanted) if (seen[name] != 1) exit 1}' \
  "$proof/receipt-members.txt" "$proof/receipt-zip-members.txt"
mapfile -t receipt_members < "$proof/receipt-members.txt"
test "${#receipt_members[@]}" -eq 489
for member in "${receipt_members[@]}"; do
  test ! -e "$root/receipt/$member" && test ! -L "$root/receipt/$member"
done
(
  ulimit -f 1024
  /usr/bin/timeout --signal=TERM --kill-after=5s 30s /usr/bin/unzip -q -n \
    "$root/receipt.zip" "${receipt_members[@]}" -d "$root/receipt"
)
for dir in "$root/capture" "$root/receipt"; do owned_dir "$dir"; done
bad=$(/usr/bin/find "$root/capture" "$root/receipt" -xdev \
  \( ! -uid "$uid" -o \( -type d ! -perm 0700 \) -o \( -type f ! -perm 0600 \) \
     -o \( ! -type f ! -type d \) \) -print -quit)
test -z "$bad"
test "$(/usr/bin/find "$root/capture" -xdev -type f -printf '.\n' | /usr/bin/wc -l)" = 2349
test "$(/usr/bin/find "$root/receipt" -xdev -type f -printf '.\n' | /usr/bin/wc -l)" = 489
actual_expanded=$(/usr/bin/find "$root/capture" -xdev -type f -printf '%s\n' | /usr/bin/awk '{n+=$1} END {print n}')
test "$actual_expanded" = "$capture_expanded"
phase=final-pins
test "$(/usr/bin/stat -c '%u:%a:%s:%h' -- "$root/preservation-proof.zip")" = "$uid:600:57919:1"
printf '%s  %s\n' "$preservation_proof_sha" "$root/preservation-proof.zip" | /usr/bin/sha256sum --check --strict -
/usr/bin/sha256sum --check --strict "$proof/preservation-selected.sha256"
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
while IFS=$'\t' read -r member bytes digest extra; do
  [[ -z $extra && $bytes =~ ^[1-9][0-9]{0,6}$ && $digest =~ ^[0-9a-f]{64}$ ]] && ((bytes <= 1048576))
  file=$root/receipt/$member
  test -f "$file" && test ! -L "$file"
  test "$(/usr/bin/realpath -e -- "$file")" = "$file"
  test "$(/usr/bin/stat -c '%u:%a:%s:%h' -- "$file")" = "$uid:600:$bytes:1"
  printf '%s  %s\n' "$digest" "$file" | /usr/bin/sha256sum --check --strict -
done < "$proof/output-members.tsv"
verify_owner
test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$source_dir")" = "$source_identity"
test "$(/usr/bin/git --no-optional-locks -C "$source_dir" rev-parse HEAD)" = "$GITHUB_SHA"
test "$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$fixture")" = "$fixture_identity"
printf '%s  %s\n' "$audit_sha" "$fixture" "$audit_sha" "$root/audit.json" | /usr/bin/sha256sum --check --strict -
test "$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%y:%z' -- "$outputs_fixture")" = "$outputs_identity"
printf '%s  %s\n' "$outputs_sha" "$outputs_fixture" "$outputs_sha" "$proof/outputs-audit.json" | /usr/bin/sha256sum --check --strict -
# archive 두 개와 transport proof, 기존 inputs/outputs까지 원래128MiB 범위 안에 둔다.
for measure in allocated apparent; do
  options=(-sx --block-size=1)
  if [[ $measure == apparent ]]; then options+=(--apparent-size); fi
  total=$(/usr/bin/timeout --signal=TERM --kill-after=5s 20s /usr/bin/du "${options[@]}" -- "$root" "$proof" |
    /usr/bin/awk 'NF==2 && $1~/^[0-9]+$/ {n++;v+=$1} END {if(n!=2)exit 1;printf "%.0f\n",v}')
  printf '%s\t%s\n' "$measure" "$total" >> "$proof/staging-growth.tsv"
  ((total <= 128 * 1024 * 1024))
done
/usr/bin/jq -n --arg source "$GITHUB_SHA" --arg origin "$origin_source" --arg run "$origin_run" \
  --arg owner "$owner" --arg arch "$arch" --arg audit "$audit_sha" --arg outputs "$outputs_sha" --arg root "$root" '{
    status:"STAGED_FOR_NATIVE_REVALIDATION", source_commit:$source, origin_source:$origin,
    origin_run:$run, owner:$owner, architecture:$arch, audit_sha256:$audit, root:$root,
    capture_members:2349, receipt_members:489, output_case_receipts:4, output_publications:484,
    outputs_audit_sha256:$outputs, original_execution:false,
    native_revalidation_required:true, full_ipc_acceptance:false
  }' > "$proof/status.json"
phase=export
test -f "${GITHUB_ENV:?}" && test ! -L "$GITHUB_ENV"
test "$(/usr/bin/stat -c %u -- "$GITHUB_ENV")" = "$uid"
# 마지막 쓰기다. 다음 step의 NativeCapture가 두 archive와 각 member를 다시 검증한다.
printf 'PLANNER_NATIVE_IPC_CORPUS=%s\nPLANNER_NATIVE_IPC_AUDIT=%s\nPLANNER_NATIVE_IPC_AUDIT_SHA256=%s\n' \
  "$root" "$root/audit.json" "$audit_sha" >> "$GITHUB_ENV"
