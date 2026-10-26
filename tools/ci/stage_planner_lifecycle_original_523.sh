#!/usr/bin/env bash
# source841의 실제 lifecycle 자료만 준비한다. Python/모델/SDK/원본/프로젝트 CLI 실행은 없다.
set +x
set +o posix
set -euo pipefail
set -C
umask 077
[[ $# == 0 ]] || { echo 'Planner lifecycle stager takes no arguments' >&2; exit 64; }
[[ ${GITHUB_ACTIONS:-}:${GITHUB_REPOSITORY:-}:${RUNNER_OS:-} == true:bin9208/openpilot-rust:Linux ]]
[[ ${GITHUB_SHA:-} =~ ^[0-9a-f]{40}$ && ${GITHUB_RUN_ID:-} =~ ^[1-9][0-9]*$ &&
   ${GITHUB_RUN_ATTEMPT:-} =~ ^[1-9][0-9]*$ && ${GITHUB_JOB:-} == planner-runtime ]]
for tool in curl jq sha256sum unzip timeout stat realpath df du head wc id find cat git uname awk mkdir rm; do
  test -x "/usr/bin/$tool"
done
token=${GITHUB_TOKEN:?GITHUB_TOKEN with actions:read is required}
uid=$(/usr/bin/id -u)
arch=$(/usr/bin/uname -m)
origin_source=841ee8a4e1357e7a0db4193cf7944e23c31ee0fb
case "$arch:${RUNNER_ARCH:-}" in
  x86_64:X64)
    audit_sha=9ae47494dcf2e201fa1c48a70e6acd934e08fefc008001bd0c95c9ba502eb88e
    origin_run=40258459503
    artifact=12623801541
    archive_bytes=58488480
    archive_sha=cb4b4b562fa5b143c32cb3b88d8e0c8879e2fc7066646a957cd01747b841bd3b
    artifact_name=rust-planner-local-x64-evidence
    workflow=.github/workflows/python-rust-523-planner-caller.yml
    zip_members=41687
    selected_bytes=2174197
    ;;
  aarch64:ARM64)
    audit_sha=23bace6376cc1d87a1857bc8f69b64d6f0ddf5c526e1f69a133502620bd81f80
    origin_run=40258459472
    artifact=12623646303
    archive_bytes=48144861
    archive_sha=5288aea003bdf02852307217587617aadcba90f017a1f1b581fec70deae4db81
    artifact_name=rust-planner-hosted-arm64-evidence
    workflow=.github/workflows/python-rust-523-native-arm-more.yml
    zip_members=37084
    selected_bytes=2174248
    ;;
  *) echo 'Actual native Linux X64 or ARM64 runner is required' >&2; exit 1 ;;
esac
# 같은10분 step 안에서 앞선 IPC 작업과 독립적으로 최대5분만 사용한다.
deadline=$((SECONDS + 300))
bounded() {
  local seconds=$1 remaining=$((deadline - SECONDS))
  shift
  ((remaining > 0))
  if ((seconds > remaining)); then seconds=$remaining; fi
  /usr/bin/timeout --signal=TERM --kill-after=5s "${seconds}s" "$@"
}
job=${CI_JOB_TEMP:?}
[[ $job == /tmp/openpilot-ci.?????? && ${job%/*} == /tmp ]]
owner=$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB
owned_dir() {
  test -d "$1" && test ! -L "$1"
  test "$(/usr/bin/realpath -e -- "$1")" = "$1"
  test "$(/usr/bin/stat -c '%u:%a' -- "$1")" = "$uid:700"
}
owned_dir "$job"; owned_dir "$job/proof"
test -f "$job/owner" && test ! -L "$job/owner"
test "$(/usr/bin/stat -c %u -- "$job/owner")" = "$uid"
(( $(/usr/bin/stat -c %s -- "$job/owner") <= 512 ))
test "$(/usr/bin/cat "$job/owner")" = "$owner"
job_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job")
parent_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job/proof")
owner_identity=$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$job/owner")
source_dir=${CI296_SOURCE:?}
[[ $source_dir == /* && -d $source_dir && ! -L $source_dir ]]
test "$(/usr/bin/realpath -e -- "$source_dir")" = "$source_dir"
test "$(bounded 10 /usr/bin/git --no-optional-locks -C "$source_dir" rev-parse HEAD)" = "$GITHUB_SHA"
test "$(/usr/bin/realpath -e -- "${BASH_SOURCE[0]}")" = "$source_dir/tools/ci/stage_planner_lifecycle_original_523.sh"
source_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$source_dir")
fixture=$source_dir/rust/crates/dev-tools/tests/fixtures/planner-lifecycle-original-audit/$arch.json
test -f "$fixture" && test ! -L "$fixture"
test "$(/usr/bin/realpath -e -- "$fixture")" = "$fixture"
(( $(/usr/bin/stat -c %s -- "$fixture") <= 1048576 ))
fixture_identity=$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$fixture")
printf '%s  %s\n' "$audit_sha" "$fixture" | /usr/bin/sha256sum --check --strict -
root=$job/planner-lifecycle-original
proof=$job/proof/planner-lifecycle-stage
test ! -e "$root" && test ! -L "$root"
test ! -e "$proof" && test ! -L "$proof"
/usr/bin/mkdir -m 700 -- "$root" "$proof" "$root/functional"
printf '%s' "$owner" > "$root/owner"
printf '%s' "$owner" > "$proof/owner"
root_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root")
proof_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof")
functional_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root/functional")
phase=admission
failed() {
  local code=$?
  trap - EXIT
  if ((code != 0)) && [[ -d $proof && ! -L $proof &&
      $(/usr/bin/realpath -e -- "$proof") == "$proof" &&
      $(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof") == "$proof_identity" ]]; then
    if [[ -f $proof/status.json && ! -L $proof/status.json &&
        $(/usr/bin/stat -c '%u:%a:%h' -- "$proof/status.json") == "$uid:600:1" ]]; then
      /usr/bin/rm -- "$proof/status.json" || :
    fi
    /usr/bin/jq -n --arg phase "$phase" --arg owner "$owner" --arg source "$GITHUB_SHA" \
      --argjson code "$code" '{status:"FAILED",phase:$phase,owner:$owner,
        source_commit:$source,exit_code:$code,corpus_exported:false}' > "$proof/failure.json" || :
  fi
  exit "$code"
}
trap failed EXIT
verify_owner() {
  owned_dir "$job"; owned_dir "$job/proof"; owned_dir "$root"; owned_dir "$proof"; owned_dir "$root/functional"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job")" = "$job_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job/proof")" = "$parent_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root")" = "$root_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof")" = "$proof_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root/functional")" = "$functional_identity"
  test -f "$job/owner" && test ! -L "$job/owner"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$job/owner")" = "$owner_identity"
  for marker in "$job/owner" "$root/owner" "$proof/owner"; do
    test -f "$marker" && test ! -L "$marker"
    test "$(/usr/bin/stat -c %u -- "$marker")" = "$uid"
    test "$(/usr/bin/cat "$marker")" = "$owner"
  done
}
test "$(/usr/bin/stat -c %d -- "$root")" = "$(/usr/bin/stat -c %d -- "$proof")"
available=$(/usr/bin/df -PB1 -- "$root" | /usr/bin/awk 'NR==2 {print $4}')
[[ $available =~ ^[0-9]+$ ]] && ((available >= 128 * 1024 * 1024))
/usr/bin/jq -n --arg owner "$owner" --arg source "$GITHUB_SHA" --argjson available "$available" \
  --argjson archive "$archive_bytes" --argjson selected "$selected_bytes" '{owner:$owner,source_commit:$source,
    available_bytes_before:$available,admitted_growth_bytes:134217728,archive_bytes:$archive,
    selected_payload_bytes:$selected,capture_limit_bytes:67108864,memory_admission:false,
    original_execution:false}' > "$proof/admission.json"
(
  ulimit -f 1024
  /usr/bin/cat "$fixture" > "$root/audit.json"
)
printf '%s  %s\n' "$audit_sha" "$root/audit.json" | /usr/bin/sha256sum --check --strict -
/usr/bin/jq -e --arg arch "$arch" --arg source "$origin_source" --arg run "$origin_run" \
  --argjson id "$artifact" --argjson bytes "$archive_bytes" --arg sha "$archive_sha" '
  .schema_version==1 and .kind=="audited-original-planner-lifecycle-inputs" and
  .source_commit==$source and .host_architecture==$arch and .origin_run_id==$run and
  .origin_run_attempt=="1" and .origin_job=="planner-runtime" and
  .artifact=={id:$id,bytes:$bytes,sha256:$sha,archive:(($id|tostring)+".zip"),extracted:"functional"} and
  (.source_sha256|length)==14 and (.runs|length)==26 and
  ([.runs[].publications[]]|length)==72
' "$root/audit.json" > /dev/null
api=https://api.github.com/repos/bin9208/openpilot-rust
get_json() (
  ulimit -f 1024
  test ! -e "$2" && test ! -L "$2"
  exec {fd}>"$2"
  status=$(bounded 35 /usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
    --connect-timeout 10 --max-time 30 --max-filesize 1048576 \
    --header "Authorization: Bearer $token" --header 'Accept: application/vnd.github+json' \
    --output "/dev/fd/$fd" --write-out '%{http_code}' "$api/$1")
  test "$status" = 200
)
phase=origin-api-auth
get_json "actions/runs/$origin_run" "$proof/origin-run.json"
/usr/bin/jq -e --argjson run "$origin_run" --arg source "$origin_source" --arg workflow "$workflow" '
  .id==$run and .head_sha==$source and .run_attempt==1 and .status=="completed" and .conclusion=="success" and
  .path==$workflow and .event=="push" and .head_branch=="codex/feat-523-python-rust" and
  .created_at=="2026-10-26T02:18:33Z" and
  .repository.id==1395501963 and .repository.full_name=="bin9208/openpilot-rust" and
  .head_repository.id==1395501963 and .head_repository.full_name=="bin9208/openpilot-rust"
' "$proof/origin-run.json" > /dev/null
get_json "actions/artifacts/$artifact" "$proof/artifact-api.json"
/usr/bin/jq -e --argjson id "$artifact" --arg name "$artifact_name" --argjson bytes "$archive_bytes" \
  --arg sha "sha256:$archive_sha" --argjson run "$origin_run" --arg source "$origin_source" \
  --arg api "$api" --slurpfile origin "$proof/origin-run.json" '
  .id==$id and .name==$name and .size_in_bytes==$bytes and .digest==$sha and .expired==false and
  .workflow_run.id==$run and .workflow_run.head_sha==$source and
  .workflow_run.repository_id==1395501963 and .workflow_run.head_repository_id==1395501963 and
  .workflow_run.head_branch=="codex/feat-523-python-rust" and
  .url==($api+"/actions/artifacts/"+($id|tostring)) and
  .archive_download_url==($api+"/actions/artifacts/"+($id|tostring)+"/zip") and
  .expires_at=="2027-01-24T02:18:33Z" and
  (.created_at|fromdateiso8601)>=($origin[0].created_at|fromdateiso8601) and
  (.created_at|fromdateiso8601)<(.expires_at|fromdateiso8601) and
  (.expires_at|fromdateiso8601)==(($origin[0].created_at|fromdateiso8601)+90*86400) and
  now<(.expires_at|fromdateiso8601)
' "$proof/artifact-api.json" > /dev/null
phase=download
verify_owner
redirect=$(bounded 35 /usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
  --connect-timeout 10 --max-time 30 --header "Authorization: Bearer $token" \
  --output /dev/null --write-out '%{http_code}\n%{redirect_url}' "$api/actions/artifacts/$artifact/zip")
[[ $redirect == $'302\nhttps://'* ]]
url=${redirect#*$'\n'}
[[ $url != *$'\n'* && $url != *$'\r'* ]]
# 2026-10-26 실제 두 artifact의 API Location host만 허용한다. userinfo/port/fragment는 거부한다.
[[ $url =~ ^https://(productionresultssa5[.]blob[.]core[.]windows[.]net|productionresultssa10[.]blob[.]core[.]windows[.]net)/[^[:space:]#]+$ ]]
unset GITHUB_TOKEN GH_TOKEN token
archive=$root/$artifact.zip
(
  ulimit -f 65536
  exec {fd}>"$archive"
  bounded 120 /usr/bin/curl --disable --silent --show-error --fail --proto '=https' \
    --connect-timeout 10 --max-time 110 --max-filesize "$archive_bytes" "$url" |
    /usr/bin/head -c "$((archive_bytes + 1))" >&"$fd"
)
test -f "$archive" && test ! -L "$archive"
test "$(/usr/bin/stat -c '%u:%a:%s:%h' -- "$archive")" = "$uid:600:$archive_bytes:1"
printf '%s  %s\n' "$archive_sha" "$archive" | bounded 30 /usr/bin/sha256sum --check --strict -
phase=zip-safety
(
  ulimit -f 8192
  bounded 30 /usr/bin/unzip -Z -1 "$archive" > "$proof/zip-members.txt"
  bounded 30 /usr/bin/unzip -Z -l "$archive" > "$proof/zip-modes.txt"
)
/usr/bin/awk -v expected="$zip_members" '
  length($0)>512 || $0!~/^[A-Za-z0-9._\/-]+$/ || $0~/^\// || $0~/\/\// || seen[$0]++ {exit 1}
  {n=split($0,p,"/");for(i=1;i<=n;i++)if(p[i]=="." || p[i]==".." || (p[i]=="" && i<n))exit 1}
  END {if(NR!=expected)exit 1}' "$proof/zip-members.txt"
/usr/bin/awk -v expected="$zip_members" 'length($1)==10 && $1~/^[-d][rwxStTs-]+$/ && $3=="unx" {n++}
  END {if(n!=expected)exit 1}' "$proof/zip-modes.txt"
(
  ulimit -f 128
  bounded 120 /usr/bin/unzip -tqq "$archive" > "$proof/zip-crc.txt"
)
# 고정 SHA의 audit→index→run manifest 순서로 필요한 exact member 목록만 확장한다.
phase=selected-metadata
index_member=$(/usr/bin/jq -er .index.path "$root/audit.json")
capture_root=${index_member%/index.json}
[[ $capture_root == tmp/openpilot-ci.??????/planner-comparison/lifecycle-original-inputs ]]
(
  ulimit -f 1024
  bounded 15 /usr/bin/unzip -p "$archive" "$index_member" > "$proof/index.json"
)
index_sha=$(/usr/bin/jq -er .index.sha256 "$root/audit.json")
index_bytes=$(/usr/bin/jq -er .index.bytes "$root/audit.json")
test "$(/usr/bin/stat -c %s -- "$proof/index.json")" = "$index_bytes"
printf '%s  %s\n' "$index_sha" "$proof/index.json" | /usr/bin/sha256sum --check --strict -
/usr/bin/jq -e --slurpfile audit "$root/audit.json" '
  .schema_version==1 and .kind=="independent-original-planner-lifecycle-inputs" and .status=="COMPLETE" and
  .provenance.source_commit==$audit[0].source_commit and .provenance.source_sha256==$audit[0].source_sha256 and
  .provenance.owner==($audit[0].origin_run_id+":1:planner-runtime") and .capture_limit_bytes==67108864 and
  .inputs==234 and .params_snapshots==30 and (.runs|length)==26 and
  .receipt_sha256==$audit[0].receipt.sha256
' "$proof/index.json" > /dev/null
(
  ulimit -f 1024
  /usr/bin/jq -r '(.index,.receipt,.cases),(.runs[]|.receipt,.publications[].raw) |
    [.path,.bytes,.sha256]|@tsv' "$root/audit.json" > "$proof/members.tsv"
  /usr/bin/jq -r --arg base "$capture_root/" '.runs[].manifest|
    [($base+.path),.bytes,.sha256]|@tsv' "$proof/index.json" > "$proof/manifests.tsv"
)
test "$(/usr/bin/wc -l < "$proof/manifests.tsv")" = 26
printf '%s\n' "$capture_root" > "$proof/capture-directories.txt"
index=0
while IFS=$'\t' read -r member bytes digest extra; do
  [[ -z $extra && $bytes =~ ^[1-9][0-9]{0,6}$ && $digest =~ ^[0-9a-f]{64}$ ]] && ((bytes<=1048576))
  [[ $member == "$capture_root/"* && $member != *'..'* ]]
  file=$proof/run-$index.json
  (
    ulimit -f 1024
    bounded 10 /usr/bin/unzip -p "$archive" "$member" > "$file"
  )
  test "$(/usr/bin/stat -c %s -- "$file")" = "$bytes"
  printf '%s  %s\n' "$digest" "$file" | /usr/bin/sha256sum --check --strict -
  case_path=${member#"$capture_root/"}
  case_path=${case_path%/manifest.json}
  [[ $member == "$capture_root/$case_path/manifest.json" ]]
  /usr/bin/jq -e --arg path "$case_path" --argjson index "$index" '
    .path==$path and .run_index==$index and .epoch==0 and
    (.mode=="source" or .mode=="rust") and
    (.case|type=="string" and test("^[A-Za-z0-9_-]+$")) and
    .path==((("0"+($index|tostring))[-2:])+"-"+.case+"-"+.mode) and
    (.params|type)=="object" and
    (.params|keys)==(if .phase=="recover" then ["prelaunch","recovery-after","recovery-before"]
                    else ["prelaunch"] end)
  ' "$file" > /dev/null
  printf '%s\n' "$capture_root/$case_path" >> "$proof/capture-directories.txt"
  /usr/bin/jq -r --arg base "$capture_root/$case_path/" \
    '$base+"inputs", (.params|keys[]|$base+"params-"+.)' "$file" >> "$proof/capture-subdirectories.txt"
  printf '%s\t%s\t%s\n' "$member" "$bytes" "$digest" >> "$proof/members.tsv"
  (
    ulimit -f 8192
    /usr/bin/jq -r --arg base "$capture_root/" \
      '(.inputs[].raw),(.params[].entries[]|select(.kind=="file")|.raw)|
      [($base+.path),.bytes,.sha256]|@tsv' "$file" >> "$proof/members.tsv"
  )
  index=$((index+1))
done < "$proof/manifests.tsv"
test "$(/usr/bin/wc -l < "$proof/capture-subdirectories.txt")" = 56
/usr/bin/cat "$proof/capture-subdirectories.txt" >> "$proof/capture-directories.txt"
/usr/bin/awk 'seen[$0]++ {exit 1} END {if(NR!=83)exit 1}' "$proof/capture-directories.txt"
/usr/bin/awk -F '\t' -v base="$capture_root/" -v bytes="$selected_bytes" '
  NF!=3 || length($1)>512 || $1!~/^[A-Za-z0-9._\/-]+$/ || $1~/^\// || $1~/\/\// ||
    $2!~/^[0-9]+$/ || $2>1048576 || length($3)!=64 || $3!~/^[0-9a-f]+$/ || seen[$1]++ {exit 1}
  {n=split($1,p,"/");for(i=1;i<=n;i++)if(p[i]=="" || p[i]=="." || p[i]=="..")exit 1;
    total+=$2;if(index($1,base)==1)capture++}
  END {if(NR!=5891 || capture!=5791 || total!=bytes)exit 1}' "$proof/members.tsv"
/usr/bin/awk -F '\t' 'NR==FNR {wanted[$1]=1;next} ($0 in wanted) {seen[$0]++}
  END {for(name in wanted)if(seen[name]!=1)exit 1}' "$proof/members.tsv" "$proof/zip-members.txt"
phase=selected-extraction
verify_owner
mapfile -t members < <(/usr/bin/awk -F '\t' '{print $1}' "$proof/members.tsv")
test "${#members[@]}" = 5891
(
  ulimit -f 1024
  bounded 120 /usr/bin/unzip -q -n "$archive" "${members[@]}" -d "$root/functional"
)
# artifact ZIP에는 directory entry가 없다. 인증된 manifest의 빈 inputs까지 정확히 복원한다.
phase=selected-directories
verify_owner
owned_dir "$root/functional/$capture_root"
while IFS= read -r relative; do
  directory=$root/functional/$relative
  owned_dir "${directory%/*}"
  if [[ ! -e $directory && ! -L $directory ]]; then
    /usr/bin/mkdir -m 700 -- "$directory"
  fi
  owned_dir "$directory"
done < "$proof/capture-subdirectories.txt"
phase=final-pins
verify_owner
bad=$(bounded 30 /usr/bin/find "$root/functional" -xdev \
  \( ! -uid "$uid" -o \( -type d ! -perm 0700 \) -o \( -type f \( ! -perm 0600 -o ! -links 1 \) \) \
     -o \( ! -type f ! -type d \) \) -print -quit)
test -z "$bad"
bounded 30 /usr/bin/find "$root/functional/$capture_root" -xdev -type d -printf '%p\n' > "$proof/capture-extracted-directories.txt"
/usr/bin/awk -v base="$root/functional/" 'NR==FNR {wanted[base $0]=1;next}
  !($0 in wanted) || seen[$0]++ {exit 1}
  END {for(name in wanted)if(seen[name]!=1)exit 1}' \
  "$proof/capture-directories.txt" "$proof/capture-extracted-directories.txt"
bounded 30 /usr/bin/find "$root/functional" -xdev -type f -printf '%P\t%s\n' > "$proof/extracted.tsv"
/usr/bin/awk -F '\t' 'NR==FNR {wanted[$1]=$2;next}
  NF!=2 || !($1 in wanted) || $2!=wanted[$1] || seen[$1]++ {exit 1}
  END {for(name in wanted)if(seen[name]!=1)exit 1}' "$proof/members.tsv" "$proof/extracted.tsv"
while IFS=$'\t' read -r member bytes digest; do
  printf '%s  %s\n' "$digest" "$root/functional/$member"
done < "$proof/members.tsv" > "$proof/selected.sha256"
bounded 60 /usr/bin/sha256sum --check --strict --quiet "$proof/selected.sha256" > "$proof/selected-after.txt"
test "$(/usr/bin/stat -c '%u:%a:%s:%h' -- "$archive")" = "$uid:600:$archive_bytes:1"
printf '%s  %s\n' "$archive_sha" "$archive" | bounded 30 /usr/bin/sha256sum --check --strict -
test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$source_dir")" = "$source_identity"
test "$(bounded 10 /usr/bin/git --no-optional-locks -C "$source_dir" rev-parse HEAD)" = "$GITHUB_SHA"
test "$(/usr/bin/stat -c '%d:%i:%u:%f:%s:%Y:%Z' -- "$fixture")" = "$fixture_identity"
printf '%s  %s\n' "$audit_sha" "$fixture" "$audit_sha" "$root/audit.json" | /usr/bin/sha256sum --check --strict -
/usr/bin/jq -n --arg source "$GITHUB_SHA" --arg origin "$origin_source" --arg run "$origin_run" \
  --arg owner "$owner" --arg arch "$arch" --arg audit "$audit_sha" --arg root "$root" '{
    status:"STAGED_FOR_NATIVE_REVALIDATION",source_commit:$source,origin_source:$origin,origin_run:$run,
    owner:$owner,architecture:$arch,audit_sha256:$audit,root:$root,selected_members:5891,
    capture_members:5791,runs:26,inputs:234,params_snapshots:30,publications:72,
    original_execution:false,native_revalidation_required:true,full_lifecycle_acceptance:false
  }' > "$proof/status.json"
phase=growth
for measure in allocated apparent; do
  options=(-sx --block-size=1)
  if [[ $measure == apparent ]]; then options+=(--apparent-size); fi
  total=$(bounded 20 /usr/bin/du "${options[@]}" -- "$root" "$proof" |
    /usr/bin/awk 'NF==2 && $1~/^[0-9]+$/ {n++;v+=$1} END {if(n!=2)exit 1;printf "%.0f\n",v}')
  printf '%s\t%s\n' "$measure" "$total" >> "$proof/staging-growth.tsv"
  ((total<=128*1024*1024))
done
phase=export
verify_owner
test -f "${GITHUB_ENV:?}" && test ! -L "$GITHUB_ENV"
test "$(/usr/bin/stat -c %u -- "$GITHUB_ENV")" = "$uid"
((SECONDS<deadline))
printf 'PLANNER_LIFECYCLE_CORPUS=%s\nPLANNER_LIFECYCLE_CORPUS_AUDIT=%s\n' \
  "$root" "$root/audit.json" >> "$GITHUB_ENV"
