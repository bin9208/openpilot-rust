# Soundd policy controller and fixture assets

`original.json.gz` is a deterministic gzip of actual unchanged source and
independently corrected #392 reference results captured before the native
controller existed. SHA256:
`82ef895663d96cfde1f33da0e22c4785a007c05a2dcab43db0b9e76a2c4c54b1`.
It contains no candidate outputs. Only private asset/repository path strings
are replaced with explicit placeholders. All 846 state rows and 6,348 samples
retain their captured values, including negative zero.

Six scenarios cover mono/stereo PCM16 at 48 kHz, mono 12 kHz, stereo 44.1 kHz,
and shipped assets under both device volume policies. Each has 141 requests.
Samples retain exact float32-bit comparisons; volume/filter use the original
1e-12 absolute/relative budget. Other state is exact. The reviewed #392 change
corrects 1,113 sample comparisons and 27 frame rows; 5,235 samples and all
nonplayback state remain unchanged. The source is
`openpilot/selfdrive/ui/soundd.py`, SHA256
`d8abf04e9c11eba35fa262c4659afc0f90104e9dc7f3a84e1b4edea5daf6a843`.
The capture binds seven source/helper hashes, shipped WAV hashes, and an actual
original Params binding receipt with 18 source pins. Private raw captures:
`.analysis/scratch/2026-10-19-soundd-392/policy-comparison/`.

The native reference implements source arithmetic/state and both playback
contracts independently. It reads real WAV bytes and never loads this corpus
or links the runtime playback policy as its oracle. A separate regression
changes WAV samples and requests to detect canned answers. Tests decode the
unchanged corpus through the existing lossless JsonValue parser: request 11's
`12.100000000000001` has bits `4028333333333334`; default serde parsing moved
that value one ULP. No captured bytes or tolerance changed.

`cli-original.json`, SHA256
`a9fdc59513c0525b4c5d8ac0f847f10a336fbc4b49e31d443d295a86e85b3c1a`,
retains 16 actual original CLI/error captures: usage, output file/dangling
symlink, invalid or mismatched binding provenance, failed/missing candidate,
and directory creation before admission. Raw byte hashes and private capture
locations are recorded; only path prefixes are normalized. Native diagnostics
are Rust framing; exit codes and filesystem effects remain checked.

`assets-original.json`, SHA256
`8b0d9e649ca82f00ea12dfb782b7e700b9ab47db9176a94dc8d1345989a16ad1`,
comes from actual unchanged `assets()` with real NumPy, wave and filesystem.
It retains all twelve WAV byte strings for both channels at 48000, 12000 and
44100 Hz, plus root-file/existing-folder failure effects. Checker/function
hashes are recorded. No native generator supplied these expectations.
The new `create-soundd-fixture-assets ROOT RATE CHANNELS` CLI explicitly accepts
positive WAV-compatible rates and channels 1 or 2; invalid usage exits2 before
creation, filesystem errors exit1, and success emits no output. Raw roots are
tested. These CLI choices are new: the original exposed a Python function.

Missing-module/bin REDs preceded implementations. The final pre-deletion gate
passed 16 family/routing tests plus 57 shared CI checks and strict Clippy.
Five fresh artifacts were sealed. Actual native policy/helper validation ran
before and after deletion of only `rust/tools/check_soundd.py`, using the same
artifacts: six policy scenarios, 6,348 samples, and all seven scenarios through
the actual migrated daemon CLI. Its required `--fixture-assets-binary` delegates
asset creation to the native helper; optional `--portaudio-library` reuses the
sealed owned ABI during the runtime-only proof. Default fixture compilation and
all seven scenario assertions remain. Both phases' 24 policy result/stderr
files are byte-identical; only private synthetic input paths are normalized.
Original CLI controls and leader-only cancellation also passed. Source absence
is permitted only for the exact retired checker; six live inputs remain
mandatory. Changed/dangling/directory reintroductions are rejected. Historical
original hashes are separate from live source hashes.

Evidence: `.analysis/scratch/2026-10-19-soundd-policy-tools/RETIREMENT-EVIDENCE.json`.
Post-deletion family16 tests, scoped strict Clippy and fresh builds pass; all
four rebuilt tool/candidate hashes match the frozen proof. The shared CI gate
has a separate ARM-owned job-condition failure pending correction. The daemon
checker is still Python and uses existing original msgq imports. The Params
loader remains shared; neither is counted retired. The native policy proof has
zero Python executions; the daemon proof explicitly executes its retained
Python CLI. All audio uses an owned PortAudio ABI fixture, never physical audio
or a vehicle. No device or performance acceptance is claimed.
