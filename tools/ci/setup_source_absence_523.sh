#!/usr/bin/env bash
# setup 원본1의 sampled absence 전용 함수다. source하여 쓰며 원본/Python/후보 ELF는 실행하지 않는다.
# 명령 경계/2초 sampling은 whole Python absence나 연속 filesystem 감시가 아니다.

_setup_absence_error() {
  printf '%s\n' "$2" >&2
  return "$1"
}

_setup_absence_parents() {
  local root=$1 path canonical uid metadata seal=
  [[ $root == /* && $root != *$'\n'* && $root != *$'\t'* ]] || {
    _setup_absence_error 43 'setup source absence inspection failed'; return $?;
  }
  canonical=$(readlink -e -- "$root") || {
    _setup_absence_error 43 'setup source absence inspection failed'; return $?;
  }
  [[ $canonical == "$root" ]] || {
    _setup_absence_error 42 'setup source parent boundary differs'; return $?;
  }
  uid=$(id -u) || return 43
  for path in "$root" "$root/tools" "$root/tools/scripts"; do
    [[ -d $path && ! -L $path && -r $path && -x $path ]] || {
      _setup_absence_error 42 'setup source parent boundary differs'; return $?;
    }
    metadata=$(stat -c '%d:%i:%f:%u:%g' -- "$path") || return 43
    [[ $metadata =~ ^[0-9]+:[0-9]+:[a-f0-9]+:${uid}:[0-9]+$ ]] || {
      _setup_absence_error 42 'setup source parent boundary differs'; return $?;
    }
    seal+="${seal:+|}$metadata"
  done
  printf '%s\n' "$seal"
}

_setup_absence_check() {
  local root=$1 expected=$2 observed entries
  observed=$(_setup_absence_parents "$root") || return $?
  [[ $observed == "$expected" ]] || {
    _setup_absence_error 42 'setup source parent boundary differs'; return $?;
  }
  # find는 dangling symlink도 entry로 세며, 조사 오류를 missing으로 바꾸지 않는다.
  entries=$(find -P "$root/tools/scripts" -mindepth 1 -maxdepth 1 -name setup_ssh_keys.py -printf x) || {
    _setup_absence_error 43 'setup source absence inspection failed'; return $?;
  }
  [[ -z $entries ]] || {
    _setup_absence_error 41 'setup source entry reappeared'; return $?;
  }
  observed=$(_setup_absence_parents "$root") || return $?
  [[ $observed == "$expected" ]] || {
    _setup_absence_error 42 'setup source parent boundary differs'; return $?;
  }
}

setup_absence_verify() {
  local root=$1 source_sha=$2 stage=$3
  [[ ${SETUP_SOURCE_PHASE:-} == absent && $root == "${SETUP_ABSENCE_ROOT:-}" &&
     $source_sha == "${SETUP_ABSENCE_SOURCE_SHA:-}" && $source_sha =~ ^[a-f0-9]{40}$ &&
     $stage =~ ^[a-z0-9_-]+$ && -n ${SETUP_ABSENCE_PARENTS:-} ]] || {
    _setup_absence_error 43 'setup source absence context differs'; return $?;
  }
  _setup_absence_check "$root" "$SETUP_ABSENCE_PARENTS" || return $?
  printf '%s\t%s\t%s\ttools/scripts/setup_ssh_keys.py\t%s\t%s\tabsent\n' \
    "$stage" "$source_sha" "$root" "$SETUP_ABSENCE_PARENTS" "$EPOCHREALTIME"
}

setup_absence_admit() {
  local root=$1 source_sha=$2 proof=$3
  [[ ${SETUP_SOURCE_PHASE:-} == absent && $root == "${DIAG_SOURCE:-}" &&
     $source_sha == "${GITHUB_SHA:-}" && $proof == "${CI_JOB_TEMP:-}/proof" &&
     $source_sha =~ ^[a-f0-9]{40}$ && -d $proof && ! -L $proof ]] || return 43
  SETUP_ABSENCE_ROOT=$root
  SETUP_ABSENCE_SOURCE_SHA=$source_sha
  SETUP_ABSENCE_PARENTS=$(_setup_absence_parents "$root") || return $?
  _setup_absence_check "$root" "$SETUP_ABSENCE_PARENTS" || return $?
  [[ $(git --no-optional-locks -C "$root" rev-parse HEAD) == "$source_sha" ]] || return 43
  (set -o noclobber
    git --no-optional-locks -C "$root" ls-tree -z HEAD -- tools/scripts/setup_ssh_keys.py > "$proof/setup-absence-head.bin" &&
    git --no-optional-locks -C "$root" ls-files -z -- tools/scripts/setup_ssh_keys.py > "$proof/setup-absence-index.bin" &&
    printf '%s\t%s\ttools/scripts/setup_ssh_keys.py\tab157e4943734c969ffc1e8113dc9d09ded355f3463da34c2b9132fc24181f3f\t%s\n' \
      "$source_sha" "$root" "$SETUP_ABSENCE_PARENTS" > "$proof/setup-absence-before.tsv" &&
    : > "$proof/setup-absence-checks.tsv") || return 43
  [[ ! -s $proof/setup-absence-head.bin && ! -s $proof/setup-absence-index.bin ]] || {
    _setup_absence_error 43 'setup source remains in Git roster'; return $?;
  }
  setup_absence_verify "$root" "$source_sha" checkout >> "$proof/setup-absence-checks.tsv"
}

setup_absence_require_stages() {
  local proof=$1 stage
  [[ $proof == "${CI_JOB_TEMP:-}/proof" && ${SETUP_SOURCE_PHASE:-} == absent ]] || return 43
  printf '%s\t%s\ttools/scripts/setup_ssh_keys.py\tab157e4943734c969ffc1e8113dc9d09ded355f3463da34c2b9132fc24181f3f\t%s\n' \
    "$SETUP_ABSENCE_SOURCE_SHA" "$SETUP_ABSENCE_ROOT" "$SETUP_ABSENCE_PARENTS" |
    cmp - "$proof/setup-absence-before.tsv" || return 43
  [[ -f $proof/setup-absence-head.bin && ! -L $proof/setup-absence-head.bin && ! -s $proof/setup-absence-head.bin &&
     -f $proof/setup-absence-index.bin && ! -L $proof/setup-absence-index.bin && ! -s $proof/setup-absence-index.bin ]] || return 43
  awk -F '\t' -v sha="$SETUP_ABSENCE_SOURCE_SHA" -v root="$SETUP_ABSENCE_ROOT" -v parents="$SETUP_ABSENCE_PARENTS" '
    NF != 7 || $2 != sha || $3 != root || $4 != "tools/scripts/setup_ssh_keys.py" || $5 != parents ||
      $6 !~ /^[0-9]+[.][0-9]+$/ || $7 != "absent" {bad=1}
    {seen[$1]=1}
    END {split("checkout qualify controls-before controls-after sampled-budget finish-before-seals finish-before-receipt finish-before-publish source-final-before source-final-after closure", stages, " ");
      for (i in stages) if (!(stages[i] in seen)) bad=1; exit bad}
  ' "$proof/setup-absence-checks.tsv" || return 43
  for stage in frozen21 negative-0 negative-1 negative-2 public-missing; do
    awk -F '\t' -v before="command-before-$stage" -v after="command-after-$stage" '
      $1 == before {b++} $1 == after {a++} END {exit !(b == 1 && a == 1)}
    ' "$proof/setup-absence-checks.tsv" || return 43
  done
  [[ $(cat "$proof/setup-absence-controls.tsv") == $'absent\tpass\nregular-reappearance\tpass\ndirectory\tpass\nsymlink\tpass\ndangling-symlink\tpass\nparent-symlink\tpass' ]] || return 43
}

setup_absence_controls() {
  local job=$1 proof=$2 control expected label code observed
  [[ $job == "${CI_JOB_TEMP:-}" && $proof == "$job/proof" ]] || return 43
  control=$(mktemp -d "$job/setup-absence-controls.XXXXXX") || return 43
  chmod 700 "$control" || return 43
  mkdir "$control/tools" "$control/tools/scripts" || return 43
  printf 'inert control\n' > "$control/target" || return 43
  expected=$(_setup_absence_parents "$control") || return $?
  (set -o noclobber; : > "$proof/setup-absence-controls.tsv") || return 43
  _setup_absence_check "$control" "$expected" || return $?
  printf 'absent\tpass\n' >> "$proof/setup-absence-controls.tsv" || return 43
  for label in regular-reappearance directory symlink dangling-symlink parent-symlink; do
    case "$label" in
      regular-reappearance) printf 'inert control\n' > "$control/tools/scripts/setup_ssh_keys.py" || return 43 ;;
      directory) mkdir "$control/tools/scripts/setup_ssh_keys.py" || return 43 ;;
      symlink) ln -s "$control/target" "$control/tools/scripts/setup_ssh_keys.py" || return 43 ;;
      dangling-symlink) ln -s "$control/missing" "$control/tools/scripts/setup_ssh_keys.py" || return 43 ;;
      parent-symlink)
        mv "$control/tools/scripts" "$control/saved-scripts" &&
          ln -s "$control/saved-scripts" "$control/tools/scripts" || return 43 ;;
    esac
    code=0
    observed=$(_setup_absence_check "$control" "$expected" 2>&1) || code=$?
    if [[ $label == parent-symlink ]]; then
      [[ $code == 42 && $observed == 'setup source parent boundary differs' ]] || return 43
      rm -- "$control/tools/scripts" && mv "$control/saved-scripts" "$control/tools/scripts" || return 43
    else
      [[ $code == 41 && $observed == 'setup source entry reappeared' ]] || return 43
      if [[ $label == directory ]]; then rm -d -- "$control/tools/scripts/setup_ssh_keys.py" || return 43
      else rm -- "$control/tools/scripts/setup_ssh_keys.py" || return 43
      fi
    fi
    printf '%s\tpass\n' "$label" >> "$proof/setup-absence-controls.tsv" || return 43
    _setup_absence_check "$control" "$expected" || return $?
  done
  # 실제 source는 건드리지 않으며 작은 synthetic control tree는 proof와 함께 job cleanup까지 유지한다.
}
