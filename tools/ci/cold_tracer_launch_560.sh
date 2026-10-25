#!/usr/bin/env bash
# #560 source-only closed-launch library. Defines functions only: no set/umask,
# traps, environment mutation, installation, command execution or file I/O here.
# All function bytes are extracted unchanged from diagnostic blob
# a8c80cad02466ecaf9110d4e3e4dfaec9c136df6.
# Caller contract: docs/naver/python-rust-523-cold-tracer-launch-560.md.
# Keep fail()/admit(), real runner/SDK base_env, deadline and state in the caller.

stamp() { stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$1"; }
hold() {
  local path=$1 canonical before fd hash
  canonical=$(readlink -e "$path") || return 1
  [[ -f $canonical && ! -L $canonical && $(stat -c %s "$canonical") -le 536870912 ]] || return 1
  before=$(stamp "$canonical") || return 1
  if [[ -v aliases[$path] ]]; then
    [[ ${aliases[$path]} == "$canonical" && ${alias_id[$path]} == "$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path")" ]] || return 1
  else aliases[$path]=$canonical;alias_id[$path]=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path");fi
  [[ ! -v held_fd[$canonical] ]] || { [[ ${held_id[$canonical]} == "$before" ]];return; }
  exec {fd}<"$canonical" || return 1
  [[ $(stamp "/proc/$$/fd/$fd") == "$before" ]] || return 1
  hash=$(sha256sum "/proc/$$/fd/$fd") || return 1;hash=${hash%% *}
  [[ $(stamp "$canonical") == "$before" && $(stamp "/proc/$$/fd/$fd") == "$before" ]] || return 1
  held_fd[$canonical]=$fd;held_id[$canonical]=$before;held_sha[$canonical]=$hash
  printf '%s\t%s\t%s\n' "$canonical" "$before" "$hash" >> "$out/held-before.tsv"
}
verify_held() {
  local path fd hash status=0
  for path in "${!aliases[@]}";do
    [[ $(readlink -e "$path") == "${aliases[$path]}" && $(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' "$path") == "${alias_id[$path]}" ]] || status=1
  done
  for path in "${!held_fd[@]}";do
    fd=${held_fd[$path]}
    [[ $(stamp "$path") == "${held_id[$path]}" && $(stamp "/proc/$$/fd/$fd") == "${held_id[$path]}" ]] || { status=1;continue; }
    hash=$(sha256sum "/proc/$$/fd/$fd") || { status=1;continue; };hash=${hash%% *}
    [[ $hash == "${held_sha[$path]}" && $(stamp "$path") == "${held_id[$path]}" ]] || status=1
    printf '%s\t%s\t%s\n' "$path" "${held_id[$path]}" "$hash" >> "$out/held-after.tsv" || status=1
  done
  return "$status"
}

pid_identity() {
  local row rest;local -a fields
  { IFS= read -r -n 8193 row < "/proc/$1/stat"; } 2>/dev/null || return 1
  rest=${row##*) };read -r -a fields <<< "$rest"
  [[ ${#row} -le 8192 && $row == "$1 ("*") "* && ${#fields[@]} -ge 20 && ${fields[0]} != Z && ${fields[0]} != X &&
     ${fields[1]} == "$$" && ${fields[2]} == "$1" && ${fields[3]} == "$1" && ${fields[19]} =~ ^[0-9]+$ &&
     ( -z ${2:-} || ${fields[0]} == "$2" ) ]] || return 1
  printf '%s\n' "${fields[19]}"
}
kill_owned() { [[ -n $active && -n $identity && $(pid_identity "$active") == "$identity" ]] && kill -KILL -- "-$active"; }
group_failure() {
  local report=$1 kind=$2 code=$3 pgid=$4 exempt=$5 entry=$6 row=$7 disappeared=$8 rest exe_code=0
  local -a fields=()
  rest=${row##*) };read -r -a fields <<< "$rest" || :
  # 최초 실패만 create-new/64KiB에 보존한다. proc comm/raw stat와 exe 외 argv/env는 읽지 않는다.
  (
    set -o noclobber;ulimit -f 64 || exit 1
    {
      printf 'schema=1\nkind=%s\nreturn_code=%s\nexpected_pgid=%s\nexempt_pid=%s\nentry=%s\ndisappeared_before_failure=%s\n' \
        "$kind" "$code" "$pgid" "$exempt" "$entry" "$disappeared" || exit 1
      printf 'state=%s\nppid=%s\npgid=%s\nsession=%s\nstartticks=%s\nraw_stat_bytes=%s\nraw_stat=%s\n' \
        "${fields[0]:-unavailable}" "${fields[1]:-unavailable}" "${fields[2]:-unavailable}" \
        "${fields[3]:-unavailable}" "${fields[19]:-unavailable}" "${#row}" "$row" || exit 1
      printf 'exe=' || exit 1
      /usr/bin/readlink -e -- "${entry%/stat}/exe" 2>/dev/null || exe_code=$?
      printf '\nexe_read_status=%s\n' "$exe_code" || exit 1
    } > "$report"
  )
}
group_quiet() {
  local pgid=$1 exempt=${2:-} report=$3 entry row rest pid disappeared=0 seen=0
  local -a fields
  for entry in /proc/[0-9]*/stat;do
    row=;fields=();pid=${entry#/proc/};pid=${pid%/stat};seen=$((seen+1))
    if ! { IFS= read -r -n 8193 row < "$entry"; } 2>/dev/null;then
      if [[ ! -e $entry && ! -d ${entry%/stat} ]];then disappeared=$((disappeared+1));continue;fi
      group_failure "$report" unreadable-stat 2 "$pgid" "$exempt" "$entry" "$row" "$disappeared" || return 2
      return 2
    fi
    rest=${row##*) };read -r -a fields <<< "$rest"
    if [[ ${#row} -gt 8192 || $row != "$pid ("*") "* || ${#fields[@]} -lt 20 ||
          ! ${fields[0]} =~ ^[A-Za-z]$ || ! ${fields[1]} =~ ^[0-9]+$ || ! ${fields[2]} =~ ^[0-9]+$ ||
          ! ${fields[3]} =~ ^[0-9]+$ || ! ${fields[19]} =~ ^[0-9]+$ ]];then
      group_failure "$report" malformed-stat 2 "$pgid" "$exempt" "$entry" "$row" "$disappeared" || return 2
      return 2
    fi
    if [[ ${fields[2]} == "$pgid" && $pid != "$exempt" && ${fields[0]} != Z && ${fields[0]} != X ]];then
      group_failure "$report" live-member 1 "$pgid" "$exempt" "$entry" "$row" "$disappeared" || return 2
      return 1
    fi
  done
  (ulimit -f 64 || exit 1;printf '%s\t%s\t%s\t%s\t%s\n' "$pgid" "$exempt" "$seen" "$disappeared" "$EPOCHREALTIME" >> "$out/group-scans.tsv") || return 2
}
await_stopped() {
  local leader=$1 start=$2 end=$3 count
  for ((count=0;count<300;count++));do
    [[ $(pid_identity "$leader") == "$start" ]] || return 1
    [[ $(pid_identity "$leader" T) != "$start" ]] || return 0
    ((SECONDS<end)) || return 1
    sleep 0.01
  done
  return 1
}

approve_launch() {
  local record tag schema pid start dev ino parent pstart pdev pino source_sha helper_sha provider_sha extra
  [[ $mode == run-inherited && -n $launch_label && $phase == "$launch_label" && $launch_seen == false ]] || return 0
  [[ -e $launch_dir/ready || -L $launch_dir/ready ]] || return 0
  record=$(/usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/timeout --signal=KILL 5s "$fixture" ready "$launch_dir") || fail 'launch request reader'
  read -r tag schema pid start dev ino parent pstart pdev pino source_sha helper_sha provider_sha extra <<< "$record"
  [[ $tag == COLD560_LAUNCH && $schema == 1 && -z $extra && $pid =~ ^[1-9][0-9]*$ && $parent =~ ^[1-9][0-9]*$ &&
     $source_sha == "$GITHUB_SHA" && $helper_sha == "$COLD560_HELPER_SHA256" && $provider_sha == "${held_sha[$tracer]}" ]] || fail 'launch request binding'
  # 실제 STOP한 tracer/timeout의 ELF를 이미 열린 provider FD의 full SHA/identity와 연결한다.
  [[ $(readlink -e "/proc/$pid/exe") == "$tracer" && $(stamp "/proc/$pid/exe") == "${held_id[$tracer]}" &&
     $(readlink -e "/proc/$parent/exe") == "$(readlink -e /usr/bin/timeout)" &&
     $(stamp "/proc/$parent/exe") == "${held_id[$(readlink -e /usr/bin/timeout)]}" ]] || fail 'live launch ELF identity'
  printf '%s  %s\n' "${held_sha[$tracer]}" "/proc/$pid/exe" \
    "${held_sha[$(readlink -e /usr/bin/timeout)]}" "/proc/$parent/exe" | sha256sum --check --strict --quiet - || fail 'live launch ELF SHA'
  /usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/timeout --signal=KILL 5s "$fixture" approve "$launch_dir" \
    "$active" "$identity" "$(stat -c '%d:%i' "$tracer")" "$(stat -Lc '%d:%i' /usr/bin/timeout)" \
    "$launch_dir/tracer.argv.nul" "$launch_dir/timeout.argv.nul" "$GITHUB_SHA" "$COLD560_HELPER_SHA256" "${held_sha[$tracer]}" strict "$launch_fault" \
    > "$launch_dir/approval.stdout" 2> "$launch_dir/approval.stderr" || fail 'fresh launch approval refused'
  [[ $(stat -c %s "$launch_dir/approval.stdout") -le 4096 && $(stat -c %s "$launch_dir/approval.stderr") -le 4096 &&
     $(pid_identity "$active") == "$identity" && $(stamp "/proc/$pid/exe") == "${held_id[$tracer]}" ]] || fail 'launch changed before CONT'
  (set -o noclobber;ulimit -f 4
    jq -n --arg source "$GITHUB_SHA" --arg helper "$COLD560_HELPER_SHA256" --arg provider "${held_sha[$tracer]}" \
      --arg request "$record" --arg leader "$active" --arg leader_start "$identity" --arg fault "$launch_fault" \
      '{schema:1,source:$source,helper_sha256:$helper,provider_sha256:$provider,request:$request,
        owned_leader:$leader,owned_leader_start:$leader_start,approval_fault:$fault,
        actual_timeout_parent_and_exact_argv_verified:true,parent_link_observed:false}' > "$launch_dir/binding.json"
  ) || fail 'bounded launch binding receipt'
  # ready/argv/approval은 아직 tracee startup 전이다. proc start는 approver가 두 번 대조했다.
  launch_tracer=$pid;launch_parent=$parent;launch_seen=true
  kill -CONT "$pid" || fail 'fresh tracer CONT'
}
inherited_command() {
  local label=$1 seconds=$2 expectation=$3 fault=$4 path
  shift 4
  local -a saved_env=("${extra_env[@]}")
  launch_label=$label;launch_fault=$fault;launch_seen=false;launch_tracer=;launch_parent=
  launch_dir=$out/inheritance/$label
  mkdir -- "$launch_dir"
  local value=1
  [[ $expectation != bad-mode ]] || value=2
  extra_env+=(COLD560_FILTER_INHERIT="$value" COLD560_LAUNCH_DIR="$launch_dir" COLD560_SOURCE_SHA="$GITHUB_SHA"
    COLD560_HELPER_SHA="$COLD560_HELPER_SHA256" COLD560_PROVIDER_SHA="${held_sha[$tracer]}")
  start_owned "$label" "$seconds" "$@"
  group_quiet "$active" "$active" "$out/logs/$label.group-before-release" || fail 'inherited command live/unknown group'
  complete_owned "$label"
  if [[ $expectation == pass ]];then
    [[ $owned_code == 0 && $launch_seen == true ]] || fail "$label inherited invocation failed"
    [[ -f $launch_dir/inherit.log && ! -L $launch_dir/inherit.log && $(stat -c '%u:%a:%h' "$launch_dir/inherit.log") == "$(id -u):600:1" &&
       $(stat -c %s "$launch_dir/inherit.log") -le 16384 ]] || fail 'inherited receipt shape/bound'
    # 고정 다섯 record와 정상 EOF를 함께 요구한다.
    awk -v tracer="$launch_tracer" -v parent="$launch_parent" '
      NR==1 {if($0!="COLD560_INHERIT start schema=1 tracer=" tracer " parent=" parent)bad=1;next}
      NR==2 {if($0!~/^COLD560_INHERIT root pid=[1-9][0-9]*$/)bad=1;split($3,a,"=");root=a[2];next}
      NR==3 {if($0!="COLD560_INHERIT root_seccomp pid=" root)bad=1;next}
      NR==4 {if($0!="COLD560_INHERIT root_exit pid=" root)bad=1;next}
      NR==5 {if(NF!=8||$1!="COLD560_INHERIT"||$2!="finish"||$3!="root=" root||$4!="root_seccomp=1"||$5!~/^fresh=[0-9]+$/||$6!~/^inherited=[0-9]+$/||$7!="additional_ptrace=0"||$8!="parent_link_observed=0")bad=1;split($5,a,"=");split($6,b,"=");if(a[2]!=b[2]||a[2]<0)bad=1;next}
      {bad=1} END {if(bad||NR!=5)exit 1}
    ' "$launch_dir/inherit.log" || fail 'inherited receipt health'
    for path in "$launch_dir/ready" "$launch_dir/approve" "$launch_dir/inherit.log" "$launch_dir/binding.json" "$launch_dir/approval.stdout" "$launch_dir/tracer.argv.nul" "$launch_dir/timeout.argv.nul";do hold "$path";done
  else
    [[ $owned_code == 1 && ! -s $out/logs/$label.stdout && ! -e $launch_dir/inherit.log && ! -L $launch_dir/inherit.log ]] || fail 'negative executed target/accepted context'
    if [[ $expectation == binding ]];then
      [[ $launch_seen == true ]] && grep -Fq 'COLD560_INHERIT launch approval' "$out/logs/$label.stderr" || fail 'wrong approval exact error'
    else
      [[ $launch_seen == false ]] && grep -Fq 'COLD560_INHERIT unsupported context' "$out/logs/$label.stderr" || fail 'unsupported context exact error'
    fi
    [[ ! -s $out/inheritance/$label.exec ]] || fail 'negative has target exec trace'
  fi
  extra_env=("${saved_env[@]}");launch_label=;launch_fault=;launch_dir=
  admit
}

start_owned() {
  local label=$1 seconds=$2 leader ready go done release status count end remaining row
  shift 2;phase=$label;admit
  [[ -z $active && -z $identity && $label =~ ^[A-Za-z0-9_.-]+$ ]] || fail 'owned command overlap/label'
  remaining=$((deadline-SECONDS));((remaining>0)) || fail '1200s diagnostic deadline'
  ((seconds<=remaining)) || seconds=$remaining
  ready=$out/logs/$label.ready;go=$out/logs/$label.go;done=$out/logs/$label.done;release=$out/logs/$label.release;status=$out/logs/$label.status
  printf '%s\0' "$label" "$@" >> "$out/commands.nul"
  # command 종료 뒤 builtin STOP만 한다. release 대기용 외부 자식을 만들지 않는다.
  /usr/bin/setsid /usr/bin/bash --noprofile --norc -c '
    set -u;ulimit -c 0;ulimit -f 524288 || exit 125
    ready=$1;go=$2;done=$3;release=$4;status=$5;seconds=$6;shift 6
    printf "%s\n" "$$" > "$ready"
    for ((n=0;n<300;n++));do [[ ! -e $go ]] || break;sleep 0.01;done
    [[ -e $go ]] || exit 125
    code=0
    /usr/bin/timeout --foreground --signal=TERM --kill-after=15s "${seconds}s" "$@" || code=$?
    printf "%s\n" "$code" > "$status" || exit 125
    : > "$done" || exit 125
    kill -STOP "$$" || exit 125
    [[ -f $release && ! -L $release ]] || exit 125
    exit "$code"
  ' cold560-owned "$ready" "$go" "$done" "$release" "$status" "$seconds" /usr/bin/env -i "${base_env[@]}" "${extra_env[@]}" "$@" \
    > "$out/logs/$label.stdout" 2> "$out/logs/$label.stderr" < /dev/null &
  leader=$!;active=$leader;identity=
  for ((count=0;count<300;count++));do
    if [[ -s $ready && $(cat "$ready") == "$leader" ]];then identity=$(pid_identity "$leader") && break;fi
    sleep 0.01
  done
  [[ -n $identity ]] || fail 'owned leader handshake failed'
  if [[ $mode == run-inherited && $label == "$launch_label" ]];then
    (set -o noclobber;printf '%s\0' "$@" > "$launch_dir/tracer.argv.nul") || fail 'tracer argv create-new'
    (set -o noclobber;printf '%s\0' /usr/bin/timeout --foreground --signal=TERM --kill-after=15s "${seconds}s" /usr/bin/env -i \
      "${base_env[@]}" "${extra_env[@]}" "$@" > "$launch_dir/timeout.argv.nul") || fail 'timeout argv create-new'
  fi
  : > "$go";end=$((SECONDS+seconds+20))
  while [[ ! -e $done ]];do
    [[ $(pid_identity "$leader") == "$identity" ]] || fail 'leader disappeared before status'
    ((SECONDS<end)) || fail 'owned command deadline'
    if [[ $mode == run-inherited && $label == "$launch_label" ]];then approve_launch;fi
    admit || fail 'resource guard failed'
    sleep 0.5
  done
  await_stopped "$leader" "$identity" "$end" || fail 'owned leader did not reach stable T state'
  { IFS= read -r -n 8193 row < "/proc/$leader/stat"; } 2>/dev/null || fail 'stopped leader stat unreadable'
  [[ ${#row} -le 8192 && $(pid_identity "$leader" T) == "$identity" ]] || fail 'stopped leader identity differs'
  (set -o noclobber;printf '%s\n' "$row" > "$out/logs/$label.stopped.stat") || fail 'stopped leader proof failed'
}
complete_owned() {
  local label=$1 leader=$active code waited=0
  [[ $(pid_identity "$leader" T) == "$identity" ]] || fail 'leader not owned/stopped before release'
  IFS= read -r code < "$out/logs/$label.status"
  [[ $code =~ ^[0-9]+$ ]] && ((code<=255)) || fail 'command status malformed'
  (set -o noclobber;: > "$out/logs/$label.release") || fail 'release marker failed'
  [[ $(pid_identity "$leader" T) == "$identity" ]] || fail 'leader identity changed before CONT'
  kill -CONT "$leader" || fail 'owned CONT failed'
  wait -f "$leader" || waited=$?
  owned_code=$code;owned_waited=$waited
  active= identity=
  group_quiet "$leader" '' "$out/logs/$label.group-after-wait" || { groups_ok=false;fail 'group live/unknown after direct wait'; }
  printf '%s\t%s\t%s\t%s\n' "$label" "$code" "$waited" "$EPOCHREALTIME" >> "$out/exit-codes.tsv"
  [[ $waited == "$code" ]] || fail "$label status/wait mismatch $code/$waited"
}
