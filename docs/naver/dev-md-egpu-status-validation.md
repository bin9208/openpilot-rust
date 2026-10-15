# dev-md native model status and diagnostics

Issue338, sourcepin e8e152dbcfd9574a1791ed12026d1880e87689b3. No deviceaccess.

Native server now handles all11persistent model-delivery phases, retry/error fields,
actual runtime active/loading/failure precedence, and fresh recovery statuses.
Rejected packages and active recovery suppress compile-restart. Currentfailure UI
requires currentbootidentity and matching installedpicklehash; diagnostics download
allows historical matchingboot records but exports only sourceallowlisted fields.
Reads are bounded to32,768Unicodecharacters/128KiBUTF8. Filepath inputs remainfixed
under the validated active modelhash. Response includes Jetlink health from shared
source-equivalent native API. Diagnostics endpoint sends no-store and attachment
headers, with exact404 on absent/mismatched reports.

Actual originalsource/newnative HTTP27focusedcases: oldbinary26RED; finalhost27/27
exact (status, headers, rawJSONbytes and Params/filesystemeffects). Firstnative run
had5recovery JSON-key-order differences; preserve orderedJSON directly rather than
round-tripping through sorted serde_json maps. All27nowGREEN. CurrentARMexample
linked and actualARMHTTP27/27alsoexact. Hostserver library/nativeegpu testsGREEN;
server+runtime-launcher alltargetsclippyGREEN after owner's newworker lintfix.

Fullsource106case suite has102exact and4explicit runtime-artifact contract
mismatches: compiled-local, compiled-relative, compiled-installed, restart-compiled.
These fixtures fabricate Python chunkmanifest/runtime files; existing native
readiness deliberately rejects those as evidence of executable Rust model assets.
Do not weaken readiness or report all106source-exact. Existing native_egpu tests
prove rejection. Positive final-native-package serverfixture passed HOST+ARM as recorded below;
modelowner actualMDM immutablepackage/worker source-match proof is separatelyrecorded.

Native bootstrap now includes --retry-network after modelowner verified actual
HTTP503 ->30s retry/cachedmodel retention onHOST+ARM. Real launcher8owned scenarios
HOSTGREEN. ARMfirstfixture incorrectlyoverrode expectedLD_LIBRARY_PATH inQEMUwrapper;
removed the unnecessaryoverride for launcheronly; all8ARMscenariosnowGREEN. No testprovider
was installed in a candidate or device. Fullstartup/deviceacceptance remainsopen.

Nativepackage integration audit found that Rust's valid installedpackage uses
`native-provider.json` instead of legacy `installed.json`. Diagnostics matching
now uses the shared validated packaged::identity fallback only when installed.json
is absent; existing malformedlegacyidentity cannot fall back. Coupled change compiled and passed alltargets clippy. Positive immutable800MBnativeMDM assets HTTPfixture
uses an owned regular-file copy and ownedmetadata; covers compiled/installed/restartrefusal,
currentfailure/historyexport/rejection/identitymismatch. No GPU or sourcepackage
mutation. HOST+ARM positive gate results are recorded below.

The firstpositive fixture correctly failed regular-file admission on modelsymlinks.
Preserve the productionguard; fixture now copies one ownedregularfile with a disk
spacecheck and hardlinks only its second ownedcache name. Sourceassetsuntouched.

Positive actualnativepackage7HTTPstates now GREEN onHOST+ARM: validcompiled and
installed, restart409, currentfailure USBclassification/stage, diagnosticsdownload,
rejected suppression and changedprovideridentity refusal. Assetsmanifest8da47e1e…
and fullmodel SHA256f2178366479a2… receipts in egpu-package-{host,arm}/native-package.json.
No sourceassetcontentchanged; ownedregularmodelcopy removed with temporaryfixture.

The explicit `--native-readiness` sourceoracle seam treats all ordinarysynthetic
Pythonartifactfixtures as notnativecompiled. This is the required independent
nativeadmission contract, not a relaxed runtimecheck. It must be paired with the
positive actualnativepackage gate above. Raw106source results remain retained
(102exact +4defined differences). Composed106nativeadmission suite passed HOST+ARM (106each);
new27features already27/27HOST+ARMrawsourceexact. Do not call thecomposed106rawsource
identity. The distinction is recorded in each JSONreceipt.

Selected-model startup integration: native launcher now probes native --present,
serializes on the shared big_model_update.lock (cancellable wait), runs
--prepare-for-startup --retry-network before --boot/manager, and provides bounded
nonblocking spinner updates with at most one restart. Network status/progress labels
follow the pinned reporter. Terminal delivery failure sets
CARROT_BIG_MODEL_STARTUP_FAILED=1 for admission and manager; inherited stale flags
are cleared for a new boot. Model owner's selection/admission guards are a coupled
gate still in progress; do not claim final full-runtime startup from launcher tests.

Old launcher fails the new stale-flag regression. New launcher passes13 owned
HOST+ARM scenarios: prior8startup cases plus eGPU successful delivery, terminal
failure/internal fallback propagation, shared lock serialization, cancellation
while locked and cancellation during delivery. Tests verify ordering, lock state,
spinner network text, no manager on cancellation, and child/spinner reaping.
Launcher library2tests and alltargetsclippy pass. Providers are isolated host
fixtures even for the ARM launcher; actual model CLI HOST+ARM is independently
validated by the model owner. No USB/GPU/audio/reboot/network-to-device was used.

A final host rerun overlapped the ARM launcher fixture and skipped the background
check: both artificial repositories hardlinked the same provider executable inode,
so actual native service reconciliation correctly recognized the other test's model
process as the same runtime. The fixture now copies each provider into its owned
repository, with an estimated disk-growth guard. Host/ARM concurrent isolated-inode
reruns both pass all13scenarios; no service identity check or production behavior was weakened.

Model owner subsequently verified coupled native presence/selection/failure gates:
35HOST and40ARM tests incl750 actual source selection cases and four actual startup
CLI/binding cases, strict alltarget clippy GREEN. Native --present requires only
--devices and runs before model path/config lookup. Native --boot honors the
per-boot failure marker before admission; runtime selection refuses stale or
unbound model identities. No vehicle acceptance is implied.
