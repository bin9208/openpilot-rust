#!/usr/bin/env bash
# 같은 job의 인증된 LC archive에서 원래 Gap 자료3개만 읽는다. 네트워크/원본 실행은 없다.
set +x
set +o posix
set -euo pipefail
set -C
umask 077
unset GITHUB_TOKEN GH_TOKEN
[[ $# == 0 ]] || { echo 'Planner Gap stager takes no arguments' >&2; exit 64; }
[[ ${GITHUB_ACTIONS:-}:${GITHUB_REPOSITORY:-}:${RUNNER_OS:-}:${GITHUB_JOB:-} == true:bin9208/openpilot-rust:Linux:planner-runtime ]]
[[ ${GITHUB_SHA:-} =~ ^[0-9a-f]{40}$ && ${GITHUB_RUN_ID:-} =~ ^[1-9][0-9]*$ && ${GITHUB_RUN_ATTEMPT:-} =~ ^[1-9][0-9]*$ ]]
for tool in jq sha256sum unzip timeout stat realpath df du head id find cat git uname awk mkdir rm; do
  test -x "/usr/bin/$tool"
done
uid=$(/usr/bin/id -u)
arch=$(/usr/bin/uname -m)
origin_source=841ee8a4e1357e7a0db4193cf7944e23c31ee0fb
case "$arch:${RUNNER_ARCH:-}" in
  x86_64:X64)
    audit_sha=1d56f0148c2da0e65be6a4ef47a3f8461691656daf59bc35f12ff76d4c28728c
    lc_audit_sha=9ae47494dcf2e201fa1c48a70e6acd934e08fefc008001bd0c95c9ba502eb88e
    origin_run=40258459503
    artifact=12623801541
    archive_bytes=58488480
    archive_sha=cb4b4b562fa5b143c32cb3b88d8e0c8879e2fc7066646a957cd01747b841bd3b
    member_root=tmp/openpilot-ci.afCEBf/planner-comparison/gap
    ;;
  aarch64:ARM64)
    audit_sha=72c7716cd954dd6160a5c3d49ebbc21ff4fede5660513aad50e9733b46bf7709
    lc_audit_sha=23bace6376cc1d87a1857bc8f69b64d6f0ddf5c526e1f69a133502620bd81f80
    origin_run=40258459472
    artifact=12623646303
    archive_bytes=48144861
    archive_sha=5288aea003bdf02852307217587617aadcba90f017a1f1b581fec70deae4db81
    member_root=tmp/openpilot-ci.s6xgRN/planner-comparison/gap
    ;;
  *) echo 'Actual native Linux X64 or ARM64 runner is required' >&2; exit 1 ;;
esac
deadline=$((SECONDS + 120))
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
regular() {
  test -f "$1" && test ! -L "$1"
  test "$(/usr/bin/realpath -e -- "$1")" = "$1"
  local observed
  observed=$(/usr/bin/stat -c '%u:%a:%h' -- "$1")
  if [[ $1 == "$job/owner" ]]; then
    # 고정 job-temp action은 기본 umask를 사용한다. 부모700과 쓰기 금지는 계속 요구한다.
    printf 'Planner Gap common owner uid:mode:links=%s\n' "$observed"
    [[ $observed == "$uid:600:1" || $observed == "$uid:644:1" ]]
  else
    test "$observed" = "$uid:600:1"
  fi
}
stamp() { /usr/bin/stat -c '%d:%i:%u:%f:%h:%s:%y:%z' -- "$1"; }
owned_dir "$job"; owned_dir "$job/proof"
regular "$job/owner"
(( $(/usr/bin/stat -c %s -- "$job/owner") <= 512 ))
test "$(/usr/bin/cat "$job/owner")" = "$owner"
source_dir=${CI296_SOURCE:?}
[[ $source_dir == /* && -d $source_dir && ! -L $source_dir ]]
test "$(/usr/bin/realpath -e -- "$source_dir")" = "$source_dir"
test "$(bounded 10 /usr/bin/git --no-optional-locks -C "$source_dir" rev-parse HEAD)" = "$GITHUB_SHA"
test "$(/usr/bin/realpath -e -- "${BASH_SOURCE[0]}")" = "$source_dir/tools/ci/stage_planner_gap_original_523.sh"
fixture=$source_dir/rust/crates/dev-tools/tests/fixtures/planner-gap-original-audit/$arch.json
test -f "$fixture" && test ! -L "$fixture"
test "$(/usr/bin/realpath -e -- "$fixture")" = "$fixture"
(( $(/usr/bin/stat -c %s -- "$fixture") <= 65536 ))
fixture_identity=$(stamp "$fixture")
source_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$source_dir")
printf '%s  %s\n' "$audit_sha" "$fixture" | /usr/bin/sha256sum --check --strict -
lc=$job/planner-lifecycle-original
lc_proof=$job/proof/planner-lifecycle-stage
owned_dir "$lc"; owned_dir "$lc_proof"
job_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job")
parent_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job/proof")
lc_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$lc")
lc_proof_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$lc_proof")
held_files=("$job/owner" "$lc/owner" "$lc_proof/owner" "$lc/audit.json" "$lc_proof/status.json")
held_identities=()
held_shas=()
for file in "${held_files[@]}"; do
  regular "$file"
  (( $(/usr/bin/stat -c %s -- "$file") <= 1048576 ))
  held_identities+=("$(stamp "$file")")
  digest=$(/usr/bin/sha256sum -- "$file")
  held_shas+=("${digest%% *}")
done
# 같은 step의 GITHUB_ENV는 아직 적용되지 않는다. 고정 sibling의 실제 status로 인증한다.
test "$(/usr/bin/cat "$job/owner")" = "$owner"
for file in "$lc/owner" "$lc_proof/owner"; do
  regular "$file"
  test "$(/usr/bin/cat "$file")" = "$owner"
done
regular "$lc_proof/status.json"; regular "$lc/audit.json"
test ! -e "$lc_proof/failure.json" && test ! -L "$lc_proof/failure.json"
printf '%s  %s\n' "$lc_audit_sha" "$lc/audit.json" | /usr/bin/sha256sum --check --strict -
/usr/bin/jq -e --arg source "$GITHUB_SHA" --arg origin "$origin_source" --arg run "$origin_run" \
  --arg owner "$owner" --arg arch "$arch" --arg audit "$lc_audit_sha" --arg root "$lc" '
  .status=="STAGED_FOR_NATIVE_REVALIDATION" and .source_commit==$source and .origin_source==$origin and
  .origin_run==$run and .owner==$owner and .architecture==$arch and .audit_sha256==$audit and .root==$root and
  .selected_members==5891 and .capture_members==5791 and .runs==26 and .inputs==234 and
  .params_snapshots==30 and .publications==72 and .original_execution==false and
  .native_revalidation_required==true and .full_lifecycle_acceptance==false
' "$lc_proof/status.json" > /dev/null
/usr/bin/jq -e --arg source "$origin_source" --arg run "$origin_run" --argjson id "$artifact" \
  --argjson bytes "$archive_bytes" --arg sha "$archive_sha" '
  .source_commit==$source and .origin_run_id==$run and .origin_run_attempt=="1" and
  .origin_job=="planner-runtime" and .artifact.id==$id and .artifact.bytes==$bytes and
  .artifact.sha256==$sha and .artifact.archive==(($id|tostring)+".zip") and .artifact.extracted=="functional"
' "$lc/audit.json" > /dev/null
archive=$lc/$artifact.zip
regular "$archive"
test "$(/usr/bin/stat -c %s -- "$archive")" = "$archive_bytes"
archive_identity=$(stamp "$archive")
exec {archive_fd}<"$archive"
archive_held=/proc/$BASHPID/fd/$archive_fd
test "$(/usr/bin/stat -Lc '%d:%i:%u:%f:%h:%s:%y:%z' -- "$archive_held")" = "$archive_identity"
printf '%s  %s\n' "$archive_sha" "$archive_held" | bounded 20 /usr/bin/sha256sum --check --strict -
# 기존 LC의 whole CRC/path 감사는 같은 immutable archive SHA와 성공 status로 재사용한다.
root=$job/planner-gap-original
proof=$job/proof/planner-gap-stage
test ! -e "$root" && test ! -L "$root"
test ! -e "$proof" && test ! -L "$proof"
/usr/bin/mkdir -m 700 -- "$root" "$proof"
printf '%s' "$owner" > "$proof/owner"
root_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root")
proof_identity=$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof")
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
      --argjson code "$code" '{status:"FAILED",phase:$phase,owner:$owner,source_commit:$source,
        exit_code:$code,corpus_exported:false}' > "$proof/failure.json" || :
  fi
  exit "$code"
}
trap failed EXIT
verify_owner() {
  for path in "$job" "$job/proof" "$lc" "$lc_proof" "$root" "$proof"; do owned_dir "$path"; done
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job")" = "$job_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$job/proof")" = "$parent_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$lc")" = "$lc_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$lc_proof")" = "$lc_proof_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$root")" = "$root_identity"
  test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$proof")" = "$proof_identity"
  regular "$proof/owner"
  test "$(/usr/bin/cat "$proof/owner")" = "$owner"
}
verify_owner
test "$(/usr/bin/stat -c %d -- "$root")" = "$(/usr/bin/stat -c %d -- "$proof")"
available=$(/usr/bin/df -PB1 -- "$root" | /usr/bin/awk 'NR==2 {print $4}')
[[ $available =~ ^[0-9]+$ ]] && ((available>=16*1024*1024))
/usr/bin/jq -n --arg owner "$owner" --arg source "$GITHUB_SHA" --arg archive "$archive" \
  --argjson reused "$archive_bytes" --argjson available "$available" '{owner:$owner,source_commit:$source,
    archive:$archive,reused_archive_bytes:$reused,available_bytes_before:$available,
    admitted_growth_bytes:16777216,selected_payload_bytes:3559611,original_execution:false,
    network_requests:0,memory_admission:false}' > "$proof/admission.json"
(
  ulimit -f 64
  /usr/bin/cat "$fixture" > "$root/audit.json"
)
printf '%s  %s\n' "$audit_sha" "$root/audit.json" | /usr/bin/sha256sum --check --strict -
/usr/bin/jq -e --arg arch "$arch" --arg source "$origin_source" --arg run "$origin_run" \
  --argjson id "$artifact" --argjson bytes "$archive_bytes" --arg sha "$archive_sha" --arg base "$member_root/" '
  .schema_version==1 and .kind=="audited-original-planner-gap-inputs" and .source_commit==$source and
  .host_architecture==$arch and .origin_run_id==$run and .origin_run_attempt=="1" and .origin_job=="planner-runtime" and
  .source_sha256=={"rust/tools/plannerd_gap_source.py":"b047d9fcf33e42795e4b3bae13fa8f7fde17a39bbd189c15d30f4aecc27f46ba"} and
  .artifact=={role:"lifecycle-functional",path:(($id|tostring)+".zip"),id:$id,bytes:$bytes,sha256:$sha} and
  .input=={path:"input.json",member:($base+"input.json"),bytes:3293789,sha256:"61ac965124d01f02a0eab1aee353de7148d87648c00e8f5f017bab9f91c2903e"} and
  .expected=={path:"expected.json",member:($base+"expected.json"),bytes:265691,sha256:"81109deaf892524fc3a4563e1f50efdeeeeafd71c42568a97c119024129cb657"} and
  .receipt=={path:"receipt.json",member:($base+"receipt.json"),bytes:131,sha256:"a587ce69d39c9c0cea41750234aeb7970477239258d6848a2c151a6559a7f48c"}
' "$root/audit.json" > /dev/null
phase=selected-members
# 전체 archive를 풀거나 복제하지 않고 exact3개 이름의 존재/유일성만 다시 확인한다.
bounded 15 /usr/bin/unzip -Z -1 "$archive_held" |
  bounded 10 /usr/bin/awk -v base="$member_root/" '
    $0==base "input.json" || $0==base "expected.json" || $0==base "receipt.json" {seen[$0]++}
    END {if(seen[base "input.json"]!=1 || seen[base "expected.json"]!=1 || seen[base "receipt.json"]!=1)exit 1}'
phase=extract
for role in input expected receipt; do
  member=$member_root/$role.json
  bytes=$(/usr/bin/jq -er ".$role.bytes" "$root/audit.json")
  digest=$(/usr/bin/jq -er ".$role.sha256" "$root/audit.json")
  blocks=1024
  if [[ $role == input ]]; then blocks=4096; fi
  [[ $bytes =~ ^[1-9][0-9]{0,6}$ && $digest =~ ^[0-9a-f]{64}$ ]] && ((bytes<=blocks*1024))
  file=$root/$role.json
  (
    ulimit -f "$blocks"
    exec {fd}>"$file"
    bounded 20 /usr/bin/unzip -p "$archive_held" "$member" | /usr/bin/head -c "$((bytes+1))" >&"$fd"
  )
  regular "$file"
  test "$(/usr/bin/stat -c %s -- "$file")" = "$bytes"
  printf '%s  %s\n' "$digest" "$file" | /usr/bin/sha256sum --check --strict -
done
/usr/bin/jq -e --slurpfile audit "$root/audit.json" '.==$audit[0].receipt_contract' "$root/receipt.json" > /dev/null
phase=final-pins
verify_owner
bad=$(bounded 10 /usr/bin/find "$root" -xdev \
  \( ! -uid "$uid" -o \( -type d ! -perm 0700 \) -o \( -type f \( ! -perm 0600 -o ! -links 1 \) \) \
     -o \( ! -type f ! -type d \) \) -print -quit)
test -z "$bad"
bounded 10 /usr/bin/find "$root" -xdev -printf '%P\t%y\n' > "$proof/root-members.tsv"
/usr/bin/awk -F '\t' '
  $1=="" && $2=="d" {next}
  $2!="f" || ($1!="audit.json" && $1!="input.json" && $1!="expected.json" && $1!="receipt.json") || seen[$1]++ {exit 1}
  {n++}
  END {if(NR!=5 || n!=4)exit 1}' "$proof/root-members.tsv"
for ((index=0;index<${#held_files[@]};index++)); do
  file=${held_files[$index]}
  regular "$file"
  test "$(stamp "$file")" = "${held_identities[$index]}"
  printf '%s  %s\n' "${held_shas[$index]}" "$file" | /usr/bin/sha256sum --check --strict -
done
regular "$archive"
test "$(stamp "$archive")" = "$archive_identity"
test "$(/usr/bin/stat -Lc '%d:%i:%u:%f:%h:%s:%y:%z' -- "$archive_held")" = "$archive_identity"
printf '%s  %s\n' "$archive_sha" "$archive_held" | bounded 20 /usr/bin/sha256sum --check --strict -
test "$(/usr/bin/stat -c '%d:%i:%u:%f' -- "$source_dir")" = "$source_identity"
test "$(bounded 10 /usr/bin/git --no-optional-locks -C "$source_dir" rev-parse HEAD)" = "$GITHUB_SHA"
test "$(stamp "$fixture")" = "$fixture_identity"
printf '%s  %s\n' "$audit_sha" "$fixture" "$audit_sha" "$root/audit.json" | /usr/bin/sha256sum --check --strict -
for role in input expected receipt; do
  regular "$root/$role.json"
  bytes=$(/usr/bin/jq -er ".$role.bytes" "$root/audit.json")
  digest=$(/usr/bin/jq -er ".$role.sha256" "$root/audit.json")
  test "$(/usr/bin/stat -c %s -- "$root/$role.json")" = "$bytes"
  printf '%s  %s\n' "$digest" "$root/$role.json" | /usr/bin/sha256sum --check --strict -
done
/usr/bin/jq -n --arg source "$GITHUB_SHA" --arg origin "$origin_source" --arg run "$origin_run" \
  --arg owner "$owner" --arg arch "$arch" --arg audit "$audit_sha" --arg root "$root" '{
    status:"STAGED_FOR_NATIVE_REVALIDATION",source_commit:$source,origin_source:$origin,origin_run:$run,
    owner:$owner,architecture:$arch,audit_sha256:$audit,root:$root,selected_members:3,
    original_execution:false,network_requests:0,native_revalidation_required:true,full_gap_acceptance:false
  }' > "$proof/status.json"
phase=growth
for measure in allocated apparent; do
  options=(-sx --block-size=1)
  if [[ $measure == apparent ]]; then options+=(--apparent-size); fi
  total=$(bounded 10 /usr/bin/du "${options[@]}" -- "$root" "$proof" |
    /usr/bin/awk 'NF==2 && $1~/^[0-9]+$/ {n++;v+=$1} END {if(n!=2)exit 1;printf "%.0f\n",v}')
  printf '%s\t%s\n' "$measure" "$total" >> "$proof/staging-growth.tsv"
  ((total<=16*1024*1024))
done
exec {archive_fd}<&-
phase=export
verify_owner
test -f "${GITHUB_ENV:?}" && test ! -L "$GITHUB_ENV"
test "$(/usr/bin/stat -c %u -- "$GITHUB_ENV")" = "$uid"
((SECONDS<deadline))
printf 'PLANNER_GAP_CORPUS=%s\nPLANNER_GAP_CORPUS_AUDIT=%s\n' "$root" "$root/audit.json" >> "$GITHUB_ENV"
