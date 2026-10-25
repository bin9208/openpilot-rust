# Version controller contract

`version_check` and `version_collector_check` are registered regression targets.
The genuine missing-controller-API RED was observed before implementation. The
native controllers implement the dynamic original schedule and real collector IPC.
The parent gate validates current-source binaries, failure artifacts, controlled
Session/Git cleanup and independent original captures. Exact runs and remaining
commit/release gates are recorded in the #296 evidence ledger.

The controller boundary is:

```rust
// version_reference_check
pub struct Options {
    pub binary: PathBuf,
    pub runner: Vec<OsString>,
    pub output: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}
pub enum Command { Source, Compare(Options) }
pub fn parse(args: &[OsString], cwd: &Path,
             environment: &BTreeMap<OsString, OsString>) -> Result<Command>;
pub fn run(repository: &Path, options: &Options) -> Result<openpilot_log_format::JsonValue>;
pub fn source(cwd: &Path, environment: &BTreeMap<OsString, OsString>,
              input: &mut dyn BufRead, output: &mut dyn Write) -> Result<()>;

// version_collector_check
pub struct Options {
    pub probe: PathBuf,
    pub collector: PathBuf,
    pub output: PathBuf,
    pub environment: BTreeMap<OsString, OsString>,
}
pub fn parse(args: &[OsString], cwd: &Path,
             environment: &BTreeMap<OsString, OsString>) -> Result<Options>;
pub fn run(repository: &Path, options: &Options, worker: &Path)
    -> Result<serde_json::Value>;
```

`Result` is the existing native-tool error result. Parsing resolves paths without
creating files, retains each raw runner argument, and never changes caller cwd or
environment. Reference grammar is `BINARY [RUNNER ARGS...]`; only the exact single
argument `--source` selects source transport. The invocation is runner arguments
followed by the resolved binary. Collector grammar is exactly `PROBE COLLECTOR
OUTPUT`. Missing arguments fail before evidence creation. Reference evidence uses
`VERSION_EVIDENCE`, defaulting to `.omo/evidence/runtime-version-73/reference`.

The source transport owns one independent `version_source::Session`, accepts each
line including a final line without newline, and flushes each response. EOF is not
a request. Caught original errors remain response values; unknown operations fail
the transport. Neither controller may consult immutable expected fixtures or the
runtime-version candidate to produce source answers.

The reference schedule creates real Git repositories and performs all original
mutations in order. Each of its 17 scenarios owns one persistent native probe and
one persistent source session. Every one of the 171 requests is retained. Source
log expectations are matched to real ZMQ records, and unexpected remaining logs
fail the scenario. Comparison rows are flushed before a response mismatch is
reported. A failed run cannot return a success report or start later scenarios.
Expected values preserve Python-compatible JSON; comparison retains the original
recursive NaN normalization and Python equality behavior without a tolerance.

`VERSION_EMULATED_SPAWN` retains only the original two specifically bounded empty
string exceptions. It must never admit another mismatch. Its manifest reports
`EMULATION_LIMITS` whenever enabled, even if the current host has no divergences.
Native implementation metadata must say Rust; it must not invent a Python version
or imply that an interpreter executed the source policy.

The collector drives the real probe and real native collector. Its expected event
comes from an independent source session reading a fresh missing-metadata directory.
It checks both real Msgq packets, Capnp validity/service/positive monotonic time,
exact expected log-field projection, Rust context, equality of both records, clean
SIGINT exit, and exactly one disk record. Packet and process output artifacts are
retained at the original failure boundaries. No warmup log is added.

Process owners must clean up partial startup, EOF, timeout, mismatch, cancellation,
and descendants after leader exit. A child identity stays reserved until group
cleanup finishes; stale output files never confer ownership of another namespace.
Tests capture a live descendant's pidfd before allowing its leader to exit and
assert its actual termination. IPC cleanup is confined to newly owned private
namespaces and endpoints.

`VERSION_PROBE_BINARY`, `VERSION_COLLECTOR_BINARY` and `VERSION_WORKER_BINARY` are
mandatory current-source ELF paths. Fresh unchanged original checker observations
are validation evidence, not expected-policy data. The immutable source corpus
and its original provenance remain unchanged.

The shared `check_version_reference.load_source()` is still used by tombstoned and
managed-entry source QA. Keep that Python file until its actual callers close.
The collector file is retired with a separately tested narrow source-guard
transition: only its exact path may be absent. Its compiled historical hash remains
in the complete source roster; changed files, directories and dangling links fail.

The inline reference Session uses a request-local absolute ten-second deadline
and the same cancellation flag as the native probe owner. Fixture Git commands
reuse the bounded native capture with their existing thirty-second resource guard
and prompt cancellation. Source Git stderr streams into the scenario's owned
source.stderr artifact. Timeout/cancellation is a controller abort, never an
ordinary source error response. Default Session requests retain their foundation
behavior. Acknowledged slow-Git and signal/deadline regressions verify these seams,
including raw stderr visible before cancellation and retained after cleanup.

`version_reference_check::Interrupted(signal)` is returned after owned cleanup. A
CLI must restore that signal's default behavior after cleanup rather than convert
it to an ordinary comparison failure. No shared process runner is duplicated by
the reference controller; its local probe owner handles only interactive pipes.
Manifest paths and argv retain original JSON string types, including surrogateescape
codepoints for raw Unix bytes. One lossless formatter serves both stdout and file.
Source stdin applies Python's surrogateescape decoding before JSON parsing; the
strict byte-oriented Session API remains separate. Two additional original wire
captures and regressions cover these previously missing boundaries.
