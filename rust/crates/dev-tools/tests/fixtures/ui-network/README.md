# Independent network UI captures and reference

The unchanged checker selects10ordered scenes/67frames:scanning,list,connecting,
password,forget,advanced,panel,password-input,tether-input,apn-input. scenes.json
is captured from its actual child calls. original.json contains actual original
PNG hashes, RGBA mode/shape, NumPy i64 sample hashes and complete state histories.
No original raster is used as runtime policy.

checker-original.json records19 unchanged checker executions with only child
processes injected; real Pillow/NumPy array arithmetic and Python state equality
remain active. Cases cover process failure, malformed/missing images and state,
mode/shape arithmetic, bool/numeric equality,NaN,first mismatch and strict zip
length/type branches. Source/native child order, inherited DISPLAY/OFFSCREEN/
PYTHONPATH, merged logs, results.json and exact printed records are retained.

policy-original.json records14actual original helper branch/error cases:
saved-network selection, auth while keyboard is topmost, shortpassword,forget
confirmation,disconnect,missingauthnetwork,missingselection,emptyframes,invalid
kind/event/security,emptynetworks,duplicates and advancednoIP. The10successes and
4errors retain files/failure behavior. In particular callbacks remain queued
while the modal keyboard/dialog owns the top stack; they are not globallydrained.

environment-original.json records7actualoriginal cases. The helper calls manual
begin/end drawing rather than gui_app.render:PROFILE_RENDER,GRID andBURN_IN do
not imply the generator's overlay/earlyexit/render-target behaviors. Startup
profiling still exits before PNG/state. Invalid PROFILE_RENDER fails at import.
SHOW_TOUCHES/VSYNC remain active initialization/widget policies.

provenance.json records57source/asset hashes from actual project-module imports
and raylib asset-load observation plus5locked dependency package records. The
Wi-Fi,Params,hardware,device and prime modules are explicit helper-owned fixture
stubs. They are not production transport/device behavior. Fonts,icons,translations,
HTML/list/keyboard/scroller/filter dependencies remain real original sources.
Capture scripts, PNGs and logs are ignored scratch, not committed raw UI images.

The independent native reference uses dedicated network state, keyboard,
screen and per-release control policies, with shared independent Forms drawing,
asset, HTML and raw ABI primitives. It never imports candidate UI widgets.

`interactions-original.json`, `lazy-original.json`, `callbacks-original.json`
and `membership-original.json` preserve actual source interaction, admission,
evaluation-order and repeated-release controls. In particular, source string
membership uses substring semantics, unused wheel values remain lazy, and
root-widget touch processing retains original float-slot failures.

The actual native gate passed all 66 non-environment cases, seven environment
cases and nineteen checker contracts. It compares original exact PNG bytes,
RGBA mode/shape, decoded i64 sample hashes, full command/stack/state histories
and supported failure outcomes. Each of 57 source/asset pins is tested changed
and missing, alongside font absence, dependency drift and malformed/EOF child
cleanup. Strict scoped Clippy also passed. No expected hashes or thresholds
changed during implementation repairs.

Initial RED evidence identified incorrect gray constants, status-icon vertical
centering, omitted global font scaling, and missing C float conversions before
scroll bounce calculations. The final fractional-scroll repair retained the
original 12 differing pixels exactly. Raw captures and traces remain ignored.

The current-candidate `network_render` build passed, followed by all ten scenes
and 67 frames through the actual native checker. Every scene had zero differing
pixels and equal state histories. With `PATH=/nonexistent`, the exec trace
contains only 31 native launches (checker once; reference, raw helper and
candidate ten times each). Binary hashes and exact logs are retained in ignored
scratch alongside concise parent coordination records.

The retirement guard passed its copied-root test: only the two originals may
be absent; modified reintroduced originals are rejected. All other live pins,
font namespace checks and dependency guards remain required. Scoped strict
Clippy passed after this change.

After the parent removed both original scripts and migrated the framework
caller, the actual ten-scene/67-frame CLI proof passed again with both files
absent and `PATH=/nonexistent`. Every comparison remained exact; the process
trace again contained only 31 native launches. The focused post-deletion
copied-root/framework tests and Clippy are queued for the next compiler slot;
parent commit and workflow integration are recorded separately.
