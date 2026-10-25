#!/usr/bin/env bash
# #549 fixture-build 전용 zero-argument worker. 원본/ELF 실행 없이 copy와 full-schema1만 생성한다.
set -euo pipefail
set +m
umask 077
set -C
export PATH=/usr/bin:/bin LC_ALL=C TZ=UTC
fail() { printf 'logging fixture batches: %s\n' "$*" >&2; exit 1; }
[[ $# == 0 ]] || fail 'zero arguments required'
[[ ${GITHUB_ACTIONS:-} == true && ${GITHUB_REPOSITORY:-} == bin9208/openpilot-rust &&
   ${GITHUB_EVENT_NAME:-} == push && ${GITHUB_REF:-} == refs/heads/codex/feat-523-python-rust &&
   ${GITHUB_JOB:-} == logging-collision8 && ${RUNNER_ENVIRONMENT:-}:${RUNNER_OS:-}:${RUNNER_ARCH:-} == github-hosted:Linux:X64 &&
   $(uname -m) == x86_64 && ${GITHUB_SHA:-} =~ ^[a-f0-9]{40}$ ]] || fail 'trusted hosted source required'
[[ ! -v GH_TOKEN && ! -v GITHUB_TOKEN && ! -v BASH_ENV && ! -v LD_PRELOAD && ! -v LD_LIBRARY_PATH ]] || fail 'unexpected inherited environment'
source=${DIAG_SOURCE:?}; job=${CI_JOB_TEMP:?}; owner=${DIAG_SOURCE_OWNER:?}
case "$job" in /tmp/openpilot-ci.??????) ;; *) fail 'job root differs';; esac
case "$owner" in "$DIAG_ACTIONS_WORKSPACE/.collision549-source-${GITHUB_RUN_ID}-${GITHUB_RUN_ATTEMPT}."??????) ;; *) fail 'source owner differs';; esac
[[ $PWD == "$source" && $GITHUB_WORKSPACE == "$source" && $source == "$owner/source" &&
   $source =~ ^/[A-Za-z0-9_./-]+$ && $job =~ ^/[A-Za-z0-9_./-]+$ ]] || fail 'source path differs'
sdk=$job/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu
manifest=$job/proof/prepared-rust-files.before.sha256
[[ ${CI_PREPARED_RUST_ROOT:-} == "$sdk" && ${CI_PREPARED_RUST_MANIFEST:-} == "$manifest" &&
   ${CARGO_HOME:-} == "$job/build/cargo-home" && ${CARGO_TARGET_DIR:-} == "$job/build/target" &&
   ${RUSTUP_HOME:-} == "$job/rustup-home" ]] || fail 'private SDK context differs'
proof=$job/proof/c8; original=$proof/cargo-trace; output=$proof/fixture-batches
for path in "$job" "$owner" "$source" "$job/proof" "$proof" "$original" "$sdk"; do
  [[ -d $path && ! -L $path && $(readlink -e "$path") == "$path" && $(stat -c %u "$path") == "$(id -u)" ]] || fail 'owned canonical directory required'
done
[[ $(stat -c %a "$job") == 700 && $(stat -c %a "$original") == 700 &&
   $(stat -c '%d:%i' "$owner") == "$DIAG_SOURCE_IDENTITY" ]] || fail 'private identity differs'
for path in "$job/owner" "$owner/.diag296-owner"; do
  [[ -f $path && ! -L $path && $(cat "$path") == "$GITHUB_RUN_ID:$GITHUB_RUN_ATTEMPT:$GITHUB_JOB" ]] || fail 'owner marker differs'
done
[[ $(git --no-optional-locks rev-parse HEAD) == "$GITHUB_SHA" ]] || fail 'source HEAD differs'
worker=$source/tools/ci/logging_collision8_batches_549.sh
[[ $(readlink -e "${BASH_SOURCE[0]}") == "$worker" ]] || fail 'worker path differs'
for value in "${COLLISION8_BATCHES_SHA256:-}" "${COLLISION8_CHECKER_SHA256:-}" "${COLLISION8_SDK_MANIFEST_SHA256:-}" "${COLLISION8_ARTIFACTS_SHA256:-}"; do
  [[ $value =~ ^[a-f0-9]{64}$ ]] || fail 'context pin missing'
done
for path in "$worker" "$manifest" "$proof/current-artifacts.json"; do
  [[ -f $path && ! -L $path && $(stat -c %s "$path") -le 1048576 ]] || fail 'context file unsafe'
done
printf '%s  %s\n' "$COLLISION8_BATCHES_SHA256" "$worker" "$COLLISION8_SDK_MANIFEST_SHA256" "$manifest" \
  "$COLLISION8_ARTIFACTS_SHA256" "$proof/current-artifacts.json" | sha256sum --check --strict --quiet -
checker=$(jq -ser --arg build "$job/build/target/" '
  if length==1 and (.[0]|keys)==["builder","checker","fixture","probe"] and
    (.[0]|all(.[]; type=="string" and startswith($build)))
  then .[0].checker else error("current artifacts differ") end' "$proof/current-artifacts.json")
[[ $checker == "$job/build/target/debug/check-native-logging" && -f $checker && ! -L $checker && -x $checker &&
   $(readlink -e "$checker") == "$checker" && $(stat -c %u "$checker") == "$(id -u)" ]] || fail 'current checker differs'
printf '%s  %s\n' "$COLLISION8_CHECKER_SHA256" "$checker" | sha256sum --check --strict --quiet -
[[ $(head -c 4 "$checker" | od -An -tx1 | tr -d ' \n') == 7f454c46 ]] || fail 'checker is not ELF'
[[ ! -e $output && ! -L $output ]]
mkdir "$output" "$output/raw" "$output/reports"
exec {original_fd}<"$original"
original_id=$(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "/proc/$$/fd/$original_fd")
snapshot() {
  local label=$1
  [[ $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$original") == "$original_id" ]]
  find -P "$original" -mindepth 1 -maxdepth 1 -printf '%f\t%D\t%i\t%m\t%U\t%G\t%n\t%s\t%T@\t%C@\t%y\n' | sort > "$output/original.$label.tsv"
  find -P "$original" -mindepth 1 -maxdepth 1 -type f -print0 | sort -z | xargs -0 -r sha256sum > "$output/original.$label.sha256"
  [[ $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$original") == "$original_id" &&
     $(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "/proc/$$/fd/$original_fd") == "$original_id" ]]
}
snapshot before
declare -A hashes=() seen_pid=()
while read -r digest path; do
  [[ $digest =~ ^[a-f0-9]{64}$ && $path == "$original/"* && ! -v hashes[$path] ]]
  hashes[$path]=$digest
done < "$output/original.before.sha256"
: > "$output/selected.tsv"
count=0; selected=0; total=0
while IFS=$'\t' read -r name dev ino mode uid gid links size mtime ctime kind; do
  [[ $name =~ ^(checker|probe|fixture)\.([1-9][0-9]{0,9})$ && $kind == f ]]
  pid=${BASH_REMATCH[2]}; prefix=${name%%.*}
  ((pid <= 4294967295))
  [[ ! -v seen_pid[$prefix:$pid] ]]; seen_pid[$prefix:$pid]=1
  [[ $mode =~ ^[0-7]{3,4}$ && $uid == "$(id -u)" && $links == 1 &&
     $dev =~ ^[0-9]+$ && $ino =~ ^[0-9]+$ && $gid =~ ^[0-9]+$ && $size =~ ^[0-9]+$ &&
     $mtime =~ ^[0-9]+\.[0-9]{10}$ && $ctime =~ ^[0-9]+\.[0-9]{10}$ ]]
  (( (8#$mode & 8#022) == 0 && size <= 536870912 ))
  count=$((count+1)); ((count <= 8192))
  path=$original/$name
  [[ -v hashes[$path] && -f $path && ! -L $path ]]
  if [[ $prefix == fixture ]]; then
    ((size > 0 && size <= 1048576)) || fail 'whole PID exceeds batch capacity'
    selected=$((selected+1)); total=$((total+size)); ((total <= 1073741824))
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$name" "$dev" "$ino" "$mode" "$uid" "$gid" "$links" "$size" "$mtime" "$ctime" "$kind" "${hashes[$path]}" >> "$output/selected.tsv"
  fi
done < "$output/original.before.tsv"
((count == ${#hashes[@]} && selected > 0))
# 한 번만 작은 metadata를 JSONL로 바꾼다. Event 배열을 합치지 않는다.
identity_filter='def identity($x):
 {dev:($x[1]|tonumber),ino:($x[2]|tonumber),mode:(32768+(reduce ($x[3]|explode[]) as $d (0;.*8+$d-48))),
  uid:($x[4]|tonumber),gid:($x[5]|tonumber),links:($x[6]|tonumber),size:($x[7]|tonumber),
  mtime:($x[8]|split(".")[0]|tonumber),mtime_nsec:($x[8]|split(".")[1][0:9]|tonumber),
  ctime:($x[9]|split(".")[0]|tonumber),ctime_nsec:($x[9]|split(".")[1][0:9]|tonumber)};'
jq -Rnc "$identity_filter inputs|split(\"\t\") as \$x|
  {name:\$x[0],pid:(\$x[0]|split(\".\")[1]|tonumber),identity:identity(\$x),sha256:\$x[11]}" \
  "$output/selected.tsv" > "$output/original.jsonl"
: > "$output/coverage.tsv"; : > "$output/batches.jsonl"
batch=0; batch_bytes=0; batch_files=0
budget() {
  local pending=$1 used apparent available values
  values=$(find -P "$job/proof" -printf '%b %s\n' | awk '{b+=$1*512;s+=$2} END {printf "%.0f %.0f\n",b,s}')
  read -r used apparent <<< "$values"
  [[ $used =~ ^[0-9]+$ && $apparent =~ ^[0-9]+$ ]]
  ((used+pending <= 1073741824 && apparent+pending <= 1073741824))
  available=$(df --output=avail -B1 "$job/proof" | awk 'NR==2&&NF==1&&$1~/^[0-9]+$/{n++;v=$1} END{if(NR!=2||n!=1)exit 1;print v}')
  ((available >= pending+536870912))
}
close_batch() {
  local report=$output/reports/$batch_name.json identity digest
  budget 67108864
  "$checker" inspect-trace --directory "$batch_dir" --prefix fixture --output "$report"
  [[ -f $report && ! -L $report && $(stat -c %s "$report") -le 67108864 ]]
  identity=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$report")
  digest=$(sha256sum "$report"); digest=${digest%% *}
  [[ $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$report") == "$identity" ]]
  jq -cn --arg batch "$batch_name" --arg identity "$identity" --arg sha "$digest" --argjson bytes "$batch_bytes" --argjson files "$batch_files" \
    '{batch:$batch,files:$files,raw_bytes:$bytes,report_identity:$identity,report_sha256:$sha}' >> "$output/batches.jsonl"
  budget 0
}
while IFS= read -r row; do
  IFS=$'\t' read -r name dev ino mode uid gid links size mtime ctime kind digest <<< "$row"
  if ((batch_files > 0 && (batch_files == 128 || batch_bytes+size > 1048576))); then
    close_batch; batch_files=0; batch_bytes=0
  fi
  if ((batch_files == 0)); then
    batch=$((batch+1)); printf -v batch_name '%04d' "$batch"
    batch_dir=$output/raw/$batch_name; mkdir "$batch_dir"
  fi
  budget "$((size+67108864))"
  path=$original/$name; copy=$batch_dir/$name
  before=$(find -P "$path" -maxdepth 0 -printf '%f\t%D\t%i\t%m\t%U\t%G\t%n\t%s\t%T@\t%C@\t%y')
  [[ $before$'\t'$digest == "$row" ]]
  # dd는 원본 FD를 NOFOLLOW/NONBLOCK으로 연다. +1 read는 성장/short copy를 숨기지 않는다.
  /usr/bin/dd if="$path" of="$copy" bs=65536 count="$((size+1))" \
    iflag=nofollow,nonblock,fullblock,count_bytes oflag=nofollow,nonblock conv=excl status=none
  [[ $(stat -c %s "$copy") == "$size" && -f $copy && ! -L $copy && $(stat -c %h "$copy") == 1 &&
     $(stat -c '%d:%i' "$copy") != "$dev:$ino" ]]
  actual=$(sha256sum "$copy"); [[ ${actual%% *} == "$digest" ]]
  after=$(find -P "$path" -maxdepth 0 -printf '%f\t%D\t%i\t%m\t%U\t%G\t%n\t%s\t%T@\t%C@\t%y')
  [[ $before == "$after" ]]
  copied=$(find -P "$copy" -maxdepth 0 -printf '%f\t%D\t%i\t%m\t%U\t%G\t%n\t%s\t%T@\t%C@\t%y')
  copied_stat=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$copy")
  printf '%s\t%s\t%s\t%s\n' "$batch_name" "$row" "$copied" "$copied_stat" >> "$output/coverage.tsv"
  batch_files=$((batch_files+1)); batch_bytes=$((batch_bytes+size))
done < "$output/selected.tsv"
close_batch
jq -Rnc "$identity_filter inputs|split(\"\t\") as \$x|
 {batch:\$x[0],name:\$x[1],pid:(\$x[1]|split(\".\")[1]|tonumber),original_identity:identity(\$x[1:12]),
  sha256:\$x[12],copy_identity:identity(\$x[13:24]),copy_stat:\$x[24]}" "$output/coverage.tsv" > "$output/coverage.jsonl"
snapshot after
cmp "$output/original.before.tsv" "$output/original.after.tsv"
cmp "$output/original.before.sha256" "$output/original.after.sha256"
jq -n --arg source "$GITHUB_SHA" --arg worker "$COLLISION8_BATCHES_SHA256" --arg checker "$COLLISION8_CHECKER_SHA256" \
  --argjson files "$selected" --argjson bytes "$total" --argjson batches "$batch" \
  '{schema:1,source_commit:$source,worker_sha256:$worker,checker_sha256:$checker,files:$files,raw_bytes:$bytes,batches:$batches}' > "$output/worker-index.json"
# complete/candidate는 부모의 모든 policy/coverage/deadline 검사 뒤에만 생성한다.
budget 0
