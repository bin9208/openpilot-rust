# current15 전용 source-authored exec 정책. 전체 PID/cwd/FD/DSO closure가 아니다.
# $binding은 원문 exec가 아니라 현재 held provider/fixture receipt/고정 command 배열이다.
def need($ok; $why): if $ok then . else error($why) end;
# decoder Vec<u8>와 비교한다. jq explode의 Unicode scalar를 byte로 오인하지 않는다(한글 ctx).
def bytes:
  [explode[] | . as $c |
   if $c<128 then $c
   elif $c<2048 then (192+($c/64|floor)),(128+$c%64)
   elif $c<65536 then (224+($c/4096|floor)),(128+(($c/64|floor)%64)),(128+$c%64)
   else (240+($c/262144|floor)),(128+(($c/4096|floor)%64)),(128+(($c/64|floor)%64)),(128+$c%64) end];
def vector: map(.bytes);
def clock: .source[0].timestamp | tonumber;
def detail_ops: ["fork","vfork","clone","clone3","setpgid","setsid","unshare","setns"];
def spawn_ops: ["fork","vfork","clone","clone3"];
def is_detail: .operation as $op | (detail_ops|index($op))!=null;
def is_spawn: .operation as $op | (spawn_ops|index($op))!=null;
def count_keys:
  reduce .[] as $row ({}; ($row.kind+":"+($row.operation//"-")+":"+($row.outcome//"-")) as $k |
    .[$k]=((.[$k]//0)+1));
def exact($want):
  .operation=="execve" and .outcome=="exec_succeeded" and .opaque_payload==false and
  .exec.program.bytes==($want.program|bytes) and
  (.exec.argv|vector)==($want.argv|map(bytes)) and
  .exec.environment.complete==true and .exec.environment.representation=="vector" and
  (.exec.environment.entries|vector|sort)==($want.env|map(bytes)|sort) and
  (.exec.environment.entries|length)==($want.env|length) and
  ($want.pid==null or .pid==$want.pid);
def exited($n): "+++ exited with \($n) +++";
def killed($s): "+++ killed by \($s) +++";
def terminal($events; $pid; $allowed):
  [$events[]|select(.pid==$pid and (.kind|startswith("terminal_")))] as $terms |
  ($terms|length)==1 and ($allowed|index($terms[0].result|implode))!=null;
def command($program; $argv; $env; $pid; $end; $parent; $role):
  {program:$program,argv:$argv,env:$env,pid:$pid,end:$end,parent:$parent,role:$role};
def checker_argv($b; $root):
  [$b.checker,"retained","--root",$b.source,"--binary",$b.probe,"--context",$b.context,
   "--output",$root+"/output","--source-commit",$b.commit,"--binary-sha256",$b.binary_sha];
def fixture_argv($b):
  [$b.fixture,"--ignored","--exact","retained::"+$prefix,"--test-threads=1","--color=never"];
def lifecycle_recipe:
  if $prefix=="first_int" then {hold:"first",code:130,foreign:false,escaped:false}
  elif $prefix=="first_term" then {hold:"first",code:143,foreign:false,escaped:false}
  elif $prefix=="first_eof" then {hold:"first",code:1,foreign:false,escaped:false}
  elif $prefix=="runtime_directory_int" then {hold:"runtime",code:130,foreign:true,escaped:false}
  elif $prefix=="runtime_directory_term" then {hold:"runtime",code:143,foreign:true,escaped:false}
  elif $prefix=="escaped_descendant_int" then {hold:"first",code:130,foreign:false,escaped:true}
  elif $prefix=="escaped_descendant_term" then {hold:"first",code:143,foreign:false,escaped:true}
  elif $prefix=="escaped_descendant_eof" then {hold:"first",code:1,foreign:false,escaped:true}
  else error("unsupported lifecycle recipe") end;
def console_env($env; $i):
  [null,"","debug","info","warning","error","DEBUG"] as $modes |
  $env + (if $modes[$i]==null then [] else ["LOGPRINT="+$modes[$i]] end) +
  (if $i%2==1 then ["DONGLE_ID=","GIT_ORIGIN=fixture-origin","GIT_BRANCH=한글/branch",
    "GIT_COMMIT=fixture-commit","MANAGER_DAEMON=fixture-daemon","CLEAN="] else [] end) +
  (if $i==2 then ["CLEAN=0"] else [] end);
def observation($row; $provider; $uid):
  $row.live_identity as $id |
  $row.sha256==$provider.sha256 and $row.executable.dev==$provider.dev and
  $row.executable.ino==$provider.ino and $row.executable.size==$provider.size and
  $row.executable.uid==$uid and $row.executable.mode==$provider.mode and
  $id.pid>0 and $id.parent>0 and $id.group>0 and $id.session>0 and $id.start_ticks>0 and
  $id.uids==[$uid,$uid,$uid,$uid] and $row.pidfd_receipt.pid==$id.pid and
  $row.pidfd_receipt.namespace_pids[0]==$id.pid;
def git_pair($b; $env; $parent):
  [command($b.git;["git","rev-parse","HEAD"];$env;null;[exited(0)];$parent;"git-head"),
   command($b.git;["git","status","--porcelain","--untracked-files=normal"];$env;null;[exited(0)];$parent;"git-status")];
def roles: ["console-0/rust","console-1/rust","console-2/rust","console-3/rust",
  "console-4/rust","console-5/rust","console-6/rust","rate/rust",
  "transport/rust/backpressure","transport/rust/threads","transport/rust/console-failure",
  "transport/rust/SIGINT","transport/rust/SIGTERM","transport/runtime-prefix"];
def invocations_ok($row; $b; $full):
  (if $full then roles else ["console-0/rust"] end) as $roles |
  ($row.invocations|map(.role))==$roles and
  ($row.invocations|map(.value.pid)|unique|length)==($roles|length) and
  all($row.invocations[]; .role as $role | .value as $v |
    $v.original==false and $v.cwd==$b.source and $v.source_commit==$b.commit and $v.source_tree=="clean" and
    $v.binary_sha256==$b.binary_sha and ($v.pid|type)=="number" and $v.pid>0 and
    ($v.binary|test("^/proc/self/fd/[0-9]+$")) and ($v.started|type)=="number" and
    (if .role=="transport/runtime-prefix" then $v.endpoint=="runtime"
     elif (["transport/rust/backpressure","transport/rust/SIGINT","transport/rust/SIGTERM"]|index($role))!=null
     then ($v.endpoint|test("^ipc:///tmp/native-log-absent-[A-Za-z0-9]{6}/missing$"))
     else ($v.endpoint|test("^ipc:///tmp/native-log-[A-Za-z0-9]{6}/socket$")) end)) and
  ($row.invocations|map(.value.binary)|unique|length)==1 and
  ($row.provider_before|length)==1 and
  ($row.provider_before[0]|{path,dev,ino,size,mode,sha256})==($b.providers.probe|{path,dev,ino,size,mode,sha256}) and
  (if $full then
    ([$row.invocations[]|select(.role=="transport/rust/backpressure" or .role=="transport/rust/SIGINT" or .role=="transport/rust/SIGTERM")|.value.endpoint]|unique|length)==1 and
    ($row.runtime_endpoint.private_marker|test("^/tmp/native-runtime-log-[A-Za-z0-9]{6}$")) and
    $row.runtime_endpoint.path==("/tmp/logmessage-"+($row.runtime_endpoint.private_marker|split("/")[-1])) and
    $row.runtime_endpoint.ownership=="prebound FD + exact SO_COOKIE/UNIX_DIAG_VFS"
   else $row.runtime_endpoint==null end);
def peer($row; $role; $env; $parent; $ends):
  ([$row.invocations[]|select(.role==$role)|.value]|.[0]) as $v |
  command($v.binary;[$v.binary,$v.endpoint];$env+["NATIVE_LOG_ENDPOINT="+$v.endpoint];$v.pid;$ends;$parent;$role);
def native_sequence($row; $b; $env; $parent; $full; $held):
  ($row.invocations[0].value.binary) as $fd |
  if $full then
    # source 순서:7console/rate/backpressure/threads/failure/invalid2/INT/TERM/runtime.
    [range(0;7) as $i | (git_pair($b;$env;$parent) +
      [peer($row;"console-\($i)/rust";console_env($env;$i);$parent;[exited(0)])])[]] +
    (["rate/rust","transport/rust/backpressure","transport/rust/threads","transport/rust/console-failure"] |
      map(. as $role | git_pair($b;$env;$parent)+[peer($row;$role;$env;$parent;[exited(0)])])|add) +
    [command($fd;[$fd,"invalid://endpoint"];$env+["NATIVE_LOG_ENDPOINT=invalid://endpoint"];null;[exited(0)];$parent;"invalid-empty"),
     command($fd;[$fd,"invalid://endpoint"];$env+["NATIVE_LOG_ENDPOINT=invalid://endpoint"];null;[exited(1)];$parent;"invalid-nonempty")] +
    (["SIGINT","SIGTERM"]|map(. as $sig | git_pair($b;$env;$parent)+
      [peer($row;"transport/rust/"+$sig;$env;$parent;[killed($sig)])])|add) +
    git_pair($b;$env;$parent)+[peer($row;"transport/runtime-prefix";
      $env+["OPENPILOT_PREFIX=-"+($row.runtime_endpoint.private_marker|split("/")[-1])];$parent;
      if $held then [killed("SIGKILL")] else [exited(0)] end)]
  else git_pair($b;$env;$parent)+[peer($row;"console-0/rust";$env;$parent;[killed("SIGKILL")])] end;
# creation edges만 사용한다. clone flag/thread/FD/cwd를 닫았다는 뜻이 아니다.
def reachable($edges; $from; $to; $depth):
  $from==$to or ($depth>0 and any($edges[]|select(.parent==$from); reachable($edges;.child;$to;$depth-1)));

need(($binding|length)==1;"one source binding required") |
$binding[0] as $b | . as $trace | .events as $events |
need($b.schema==1 and $b.providers.probe.path==$b.probe and $b.providers.probe.sha256==$b.binary_sha and
  $b.providers.checker.path==$b.checker and $b.providers.probe.uid==$b.uid and $b.providers.checker.uid==$b.uid;
  "current held provider binding differs") |
need(.schema==2 and .prefix==$prefix and .directory==$b.proof+"/trace" and
  .raw_immutable==true and .requires_exit_status_zero==true and .full_event_index==false and
  .lexically_checked_all_events==true and .detailed_operations==detail_ops and
  .closed_pid==false and .closed_cwd==false and .closed_fd==false and .closed_dso==false and
  .full_acceptance==false and .retirement_credit==false;"summary scope differs") |
need((.event_counts|to_entries|map(.value)|add)==.event_count and .event_count>0 and .event_count<=1000000 and
  .physical_lines>=.event_count and .physical_lines<=2000000 and
  (.raw_files|map(.pid)|unique|length)==(.raw_files|length) and
  (.raw_files|map(.name)|sort)==(.directory_roster|map(select(.name|startswith($prefix+".")))|map(.name)|sort);"summary counts/roster differ") |
([$events[]|select(is_detail)]|count_keys) as $detailed |
(.event_counts|with_entries(select((.key|split(":")[1]) as $op | (detail_ops|index($op))!=null))) as $counts |
need($detailed==$counts;"process/session detail cardinality differs") |
[$events[]|select(.exec!=null)] as $execs |
need(([$events[]|select(.operation=="execve" or .operation=="execveat")]|length)==($execs|length) and
  ([$execs[]]|count_keys)==(.event_counts|with_entries(select(.key|test("^[^:]+:execve(at)?:")))) and
  all($execs[]; .operation=="execve" and .outcome=="exec_succeeded");"unsupported exec attempt") |
[$events[]|select(is_spawn)] as $spawn |
need(all($spawn[]; .kind=="syscall" and (.outcome=="returned" or .outcome=="failed"));"unsupported aborted spawn") |
[$spawn[]|select(.outcome=="returned") |
  need((.result|implode|test("^[1-9][0-9]*$"));"spawn child result differs") |
  {parent:.pid,child:(.result|implode|tonumber),time:clock}] as $edges |
need(($edges|map(.child)|unique|length)==($edges|length) and
  all($edges[]; .child as $child | any($trace.raw_files[]; .pid==$child));"spawn child raw attribution differs") |
need(all($events[]|select(.operation=="unshare" or .operation=="setns"); false);"unsupported namespace attempt") |
([$execs[]|select(.exec.program.bytes==($b.fixture|bytes))]) as $fixtures |
need(if $prefix=="retained-once" then ($fixtures|length)==0 else ($fixtures|length)==1 end;"fixture count differs") |
($fixtures[0].pid//null) as $owner |
need(if $owner==null then true else
  ($fixtures[0]|exact(command($b.fixture;fixture_argv($b);$b.fixture_env;$owner;[];null;"fixture"))) and
  terminal($events;$owner;[exited(0)]) end;"fixture argv/env/terminal differs") |
([$execs[]|select(.exec.program.bytes==($b.checker|bytes))]) as $checks |
(if $prefix=="retained-once" then [$b.native+{root:($b.proof+"/native"),receipt:null}]
 elif $prefix=="forbidden_executable_options" then
   ["builder","reference","source-hook"]|map(. as $key|$b.cases[]|select(.receipt.case_key=="retained-reject-"+$key))
 else [$b.cases[]|select(.receipt.case_key=="retained-"+($prefix|gsub("_";"-"))) | . as $row |
   select(.receipt.held==null or any($checks[]; .pid==$row.receipt.held.checker_observation.live_identity.pid))] end) as $rows |
need(($rows|length)==(if $prefix=="forbidden_executable_options" then 3 else 1 end) and
  ($checks|length)==($rows|length);"case/checker attribution differs") |
(reduce range(0;($rows|length)) as $i
  ({wanted:[],escaped:[],checker_ids:[],receipt_keys:[]};
   $rows[$i] as $row | $row.receipt as $r |
   ($prefix=="retained-once") as $normal |
   ($normal or $r.hold=="runtime") as $full |
   ($normal or $r.held!=null) as $positive |
   ($r.escaped_descendants_reaped==true) as $escaped |
   ($r.case_key//""|startswith("retained-reject-")) as $forbid |
   (if $normal then $b.direct_env else $b.checker_env end) +
     (if $positive and $normal==false then ["NATIVE_LOGGING_QA_HOLD="+$r.hold,"NATIVE_LOGGING_QA_RECORDS="+$row.root+"/records"]
        + (if $escaped then ["NATIVE_LOGGING_QA_ESCAPED=1"] else [] end)
      elif $positive==false and $forbid==false then ["NATIVE_LOGGING_QA_HOLD=first","NATIVE_LOGGING_QA_RECORDS="+$row.root+"/unexpected-child"] else [] end) as $env |
   (if $normal then [$b.checker,"retained","--root",$b.source,"--binary",$b.probe,"--context",$b.context,
      "--output",$b.proof+"/native","--source-commit",$b.commit,"--binary-sha256",$b.binary_sha]
    else checker_argv($b;$row.root)+(if $forbid then ["--"+($r.case_key|ltrimstr("retained-reject-")),$b.probe] else [] end) end) as $argv |
   [$checks[]|select(.exec.argv!=null and (.exec.argv|vector)==($argv|map(bytes)))] as $match |
   need(($match|length)==1;"checker source argv differs") |
   $match[0].pid as $pid |
   need($normal or ($r.cli_mode=="retained" and $r.R4_acceptance==false and $r.lifecycle_reuse_granted==false and
     $r.controller_argv==$argv);"fixture receipt source binding differs") |
   need($normal or reachable($edges;$owner;$pid;8);"fixture/checker creation edge missing") |
   need(if $positive and $normal==false then
     lifecycle_recipe as $recipe |
     $r.hold==$recipe.hold and $r.returncode==$recipe.code and $r.foreign_directory==$recipe.foreign and
     $r.escaped_descendants_reaped==$recipe.escaped and
     $r.source_commit==$b.commit and $r.native_sha256==$b.binary_sha and $r.context_index_sha256==$b.context_sha and
     $r.actual_producer_reaped==true and $r.owned_directory_removed==true and $r.shutdown_seconds<20 and
     observation($r.held.checker_observation;$b.providers.checker;$b.uid) and
     observation($r.held.peer_observation;$b.providers.probe;$b.uid) and
     $r.held.checker_observation.live_identity.pid==$pid and $r.held.checker_observation.live_identity.parent==$owner and
     $r.held.peer_observation.live_identity.pid==$r.held.pid and $r.held.peer_observation.live_identity.parent==$pid and
     $r.held.peer_observation.live_identity.group==$r.held.pid and
     $r.held.peer_observation.live_identity.session==$r.held.checker_observation.live_identity.session and
     $r.held.peer_live_identity==$r.held.peer_observation.live_identity and
     $r.held.output==$row.root+"/output/"+(if $full then "transport/runtime-prefix" else "console-0/rust" end) and
     $r.held.endpoint==($row.invocations[]|select(.role==(if $full then "transport/runtime-prefix" else "console-0/rust" end))|.value.endpoint) and
     $r.foreign_before==$r.foreign_after and
     $r.held.pid==($row.invocations[]|select(.role==(if $full then "transport/runtime-prefix" else "console-0/rust" end))|.value.pid)
    elif $positive==false then
     $r.zero_child_exec_requires_external_trace==true and
     (if $forbid then $r.returncode==2 else $r.returncode==1 and $r.kind==($prefix|ltrimstr("output_")|gsub("_";"-")) and
       $r.before==$r.after and $r.producer_barrier_absent==true end) and
     all($events[]|select(.pid==$pid); is_detail|not)
    else $b.native_report.result=="pass: retained native comparison only" and $b.native_report.source_commit==$b.commit and
      $b.native_report.native_binary_sha256==$b.binary_sha and $b.native_report.native_exec_path==$row.invocations[0].value.binary
    end;"fixture pidfd/reap or zero-child receipt differs") |
   need($positive==false or invocations_ok($row;$b;$full);"native invocation source binding differs") |
   (if $normal then 0 else $r.returncode end) as $exit |
   .wanted += [command($b.checker;$argv;$env;$pid;[exited($exit)];null;"checker")] |
   .checker_ids += [$pid] | .receipt_keys += [if $normal then "retained-once" else $r.case_key end] |
   if $positive then
     .wanted += git_pair($b;$env;$pid)+native_sequence($row;$b;$env;$pid;$full;($normal|not)) +
       (if $normal or $r.returncode==1 then git_pair($b;$env;$pid) else [] end) |
     if $escaped then
       need(($r.escaped_descendants|map(.role))==["escaped","leaf"];"escaped role roster differs") |
       ($row.invocations[0].value) as $first |
       reduce range(0;2) as $n (. ; $r.escaped_descendants[$n] as $child |
         (if $n==0 then $first.pid else $r.escaped_descendants[0].pid end) as $parent |
         need(observation($child.observation;$b.providers.probe;$b.uid) and
           $child.live_identity==$child.observation.live_identity and $child.pid==$child.live_identity.pid and
           $child.parent==$parent and $child.live_identity.parent==$parent and
           $child.session==$r.escaped_descendants[0].pid and $child.group==$r.escaped_descendants[0].pid and
           ($n!=0 or ($child.session!=$r.held.peer_live_identity.session and $child.group!=$r.held.peer_live_identity.group));"escaped actual identity differs") |
         .escaped += [command($b.probe;[$b.probe,(if $n==0 then "--qa-logging-escaped" else "--qa-logging-leaf" end)];
           $env+["NATIVE_LOG_ENDPOINT="+$first.endpoint];$child.pid;[killed("SIGTERM"),killed("SIGKILL")];$parent;$child.role)])
     else need($normal or ($r.escaped_descendants==[] and $r.escaped_descendants_reaped==false);"unexpected escaped receipt") end
   else . end)) as $policy |
[$execs[]|. as $actual | select(.exec.program.bytes!=($b.fixture|bytes) and
  all($policy.escaped[]; .pid!=$actual.pid))] | sort_by(clock) as $ordered |
need(($ordered|length)==($policy.wanted|length);"unexpected/missing exec attempt") |
need(all(range(0;($ordered|length)); . as $i | $policy.wanted[$i] as $want | $ordered[$i] as $actual |
  ($actual|exact($want)) and terminal($events;$actual.pid;$want.end) and
  ($want.parent==null or any($edges[]; .parent==$want.parent and .child==$actual.pid and .time<=($actual|clock))));"ordered exec/argv/env/terminal/creation differs") |
[$execs[]|select(.exec.program.bytes==($b.probe|bytes))] as $escaped_exec |
need(($escaped_exec|length)==($policy.escaped|length) and
  all($policy.escaped[]; . as $want |
    [$escaped_exec[]|select(exact($want))] as $match |
    ($match|length)==1 and terminal($events;$want.pid;$want.end) and
    any($edges[]; .parent==$want.parent and .child==$want.pid and .time<=($match[0]|clock)));"escaped exact exec/creation differs") |
need(all($events[]|select(.operation=="setpgid"); . as $call |
  .kind=="syscall" and .outcome=="returned" and (.result|implode)=="0" and
  any($ordered[]; .pid as $pid | $pid==$call.pid and ($policy.checker_ids|index($pid))==null and
    $call.source[0].line<.source[0].line));"unsupported process-group attempt") |
need(all($events[]|select(.operation=="setsid"); . as $call |
  .kind=="syscall" and .outcome=="returned" and (.result|implode)==(.pid|tostring) and
  any($policy.escaped[]; .role=="escaped" and .pid==$call.pid));"unsupported session attempt") |
need(all($policy.escaped[]|select(.role=="escaped"); .pid as $pid |
  ([$events[]|select(.pid==$pid and .operation=="setsid")]|length)==1);"escaped setsid missing") |
{schema:1,prefix:$prefix,source_commit:$b.commit,summary_schema:2,raw_immutable:true,
 selected_exec_attempts:($execs|length),source_expected_exec_attempts:(($policy.wanted|length)+($policy.escaped|length)+($fixtures|length)),
 checker_attempts:($checks|length),case_keys:$policy.receipt_keys,exact_exec_argv_env:true,
 structural_detail_counts_equal:true,observed_creation_edges:($edges|length),
 fixture_pidfd_reap_receipts_bound:($prefix!="retained-once" and ($prefix|startswith("output_")|not) and $prefix!="forbidden_executable_options"),
 python_exec_attempts:0,cpp_exec_attempts:0,zero_child_checker_attempts:(if $prefix|startswith("output_") then 1 elif $prefix=="forbidden_executable_options" then 3 else 0 end),
 full_event_index:false,closed_pid:false,closed_cwd:false,closed_fd:false,closed_dso:false,
 all_adopted_reaped:false,old27_accepted:false,pending_runtime2_accepted:false,R4_acceptance:false,retirement_credit:0}
