# dev-md continuation checkpoint (2026-10-15)

This is unfinished engineering work for #338. Continue all remaining porting and
full runtime/ARM checks; do not stop at this checkpoint. No vehicle access or
main/dev integration. Local dev is still b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a.
HEAD/origin dev-md still 7bc9c45ef710d28c546dda249c4badac33c08f6c; subsequent work
is uncommitted. Preserve it. Source pin e8e152dbcfd9574a1791ed12026d1880e87689b3,
common ancestor 43203371004e035bdb70a00a8dad29a4b657c6c3.

## Active ownership

- Parent: manager/catalog/Params, Carrot server/web/settings/docs, soundd,
  native UI/cluster/S3, remaining startup/tooling and integration.
- planner_resume: planner/control-policy stopping/coasting, radard final source,
  native local radar replay bundle/adapter. RadarCAN decoders are separate.
- selfdrive_resume: DM2/selfdrived/shared impact; now Toyota/Hyundai card helpers
  and radarcan Bosch/ARS. Own exact-source fixtures and ARM comparisons.
- model_complete: continues model_resume checkpoint; MDM HCQ, background recovery,
  active Jetlink, model/provider/dev-tools staging. Read its model checkpoint.

All workers are GPT-6.1 Sol. Cargo cache is shared and builds MUST be explicitly
handed off. Parent currently owns root ARM retry; next bounded slot planner
(replay compile), then selfdrive/model as coordinated. Independent original
reference comparisons can run without Cargo.

## Parent completed checks

See manager/settings/web/sound/Panda/lateral/DM/planner/radar validation docs for
exact counts and limits. Parent host tests, clippy and source comparisons passed
before the latest retirement edits. Web assets rebuilt; 834 tests pass with
PYTHON pointing to the project venv. Settings/restore/profile/real HTTP tests pass.
Paired guides merged; native docs checker passed all 430 changed paths at that
point. Wiki 25 focused tests pass; staged generation/validation pass. Six reviewed
manual-region changes are in dev-md-wiki-manual.patch; never publish to stable
Wiki automatically. Driver-monitoring guide HUD exclusion was corrected against
actual manager policy, unlike the stale upstream sentence.

Source soundd now copied from full pin. Native PCM gain uses event identity,
clears DM gain on same-WAV handoff; 4 tests + 6 scenarios/6672 samples pass. The
source checker and manager checker gained --runner for direct ARM execution.

Previous committed CI run38681313689 finished: aarch64 build passed; overall
failed on recorded fixtures. Current dirty-tree changes have not had revised-head
CI. The issues and final runtime gate remain open.

## ARM cross-link setup in progress

Initial root6-package ARM build failed after concurrent shared edits; their
owners corrected card UTF8 conversion and usbgpu clock dependency. Next failures
were manager->loggerd libavutil/format/codec using x86 pkg-config directories.
Target FFmpeg staging is prepared at:
`.analysis/scratch/2026-10-15-dev-md-resume/ffmpeg-arm`
Its lib points to verified runtime-provider-296 ARM libraries (manifest hashes
all match); include points to matching6.1 headers. Set FFMPEG_DIR to that absolute
path for root ARM builds. Generic LIBRARY_PATH did not affect cross GCC search.
This staging is validation-only; no artifact/device deployment.

Use PATH append native-sdk/usr/bin (never prepend; ~/.local/bin wrappers provide
sysroot/libs), shared CARGO_TARGET_DIR=/home/fma1/rust/cache/cargo-target,
CARGO_INCREMENTAL=0, CARGO_BUILD_JOBS=2, DEV/TEST_DEBUG=0. ARM runner:
`/home/fma1/rust/cache/native-sdk/usr/bin/qemu-aarch64-static -L /home/fma1/rust/cache/native-sdk/usr/aarch64-linux-gnu`.
Check disk before every build/install/copy (344GiB at last check).

Current root ARM log: root-arm-build-v5.log in dev-md-resume scratch. Package set
manager/soundd/carrot-server/selfdrived/plannerd/radard, bins plus sound_trace,
controller, planner_final_trace, plannerd_owner_trace, plannerd_solver_trace,
radard_controller_trace and radard_lane_trace. Send linked examples to workers
for QEMU comparisons when verified. Manager_trace ARM and server test examples
may need an additional bounded build. No success claim from a partial build.

## Parent retirement edits currently UNVERIFIED

YouTube feature removal is in progress. Actual source catalog comparison RED
showed obsolete youtube_low entry versus expected logmessaged. Native catalog
now removes four YouTube descriptors/predicates; expected count63->59. Updated
check_manager_catalog removes retired policy cases; rust/port-status removes the
four process rows. No green rerun yet.

Server YouTube live/test implementation, binary, examples and reference tools
were removed. Application no longer constructs/registers/drains YouTube service;
terminal command removed. Retained Carrot Vision diagnostic primitives moved to
vision_test/{process,support}. Original source feature/config/intro/terminal
references updated for removal. Array lengths fixed (legacy state files5,
commands6). Needs full server examples/tests/clippy and terminal/cleanup/HTTP
regressions; existing terminal youtube-test requests intentionally test rejection.
Dev-tools dynamic provider validation no longer requires retired librtmp symbols;
2-service test expectations adjusted, pending test.

Encoder original config extracted and executed with rustc gave genuine RED:
--youtube-low returned Some(YoutubeLow) instead of None. Native four modes and
quality profiles are removed, along with YouTube V4L crop/contract branch and
publisher. Original loggerd.h/encoderd.cc/v4l_encoder.cc are final pin (the latter
delta is only YouTube removal). Encoder oracle profiles drop retired cases but
retain generic timestamp/queue/capability rejection on main. Needs tests/full
native codec+V4L/runtime comparisons. Retained Cereal YouTube service/enum is for
old log decoding, not a managed producer.

Not yet done: remove the three CarrotYouTube Params keys from canonical header,
replace manager_trace/default_edges and manager_reference seeds with other integer
keys, rebuild original Params binding, then rerun affected comparisons. Notify
workers before replacing their shared binding. Current binding location:
`.analysis/scratch/2026-10-15-dev-md-ci/params/params_pyx.cpython-312-x86_64-linux-gnu.so`.

Source fixture comparison environment usually needs LD_LIBRARY_PATH native-sdk
usr/lib/x86_64-linux-gnu; PYTHONPATH includes repo/rust/tools/opendbc_repo and
`.analysis/scratch/2026-10-15-dev-md-proclog/msgq-python` for real IPC imports.
No pip install needed; .venv already has numpy/capnp/aiohttp/brotli/pytest.

## Large remaining scopes

Native onroad UI/DM inset/impact dialog/radar+signal views, cluster scheduling/S3,
signal observer+assist producer/consumer integration, full Carrot server upload
scope/screenrecord/recovery/Jetlink support, startup failure/rebuild/crash evidence,
all vehicle/radar/model gaps in owners' docs, native tooling/replay packaging,
source coverage ledger closure and exact-head whole runtime/ARM/CI/review. Imported
web/catalog/guides describe intended final behavior; they do not prove unfinished
native producer functionality. No device-test handoff yet.


## New resumed continuation (newer than the sections above)

Parent verified retirement:72hosttests; catalog59descriptors/64configs/4372predicates/
6fatalcases;7fullmanager scenarios HOST+ARM/QEMU exact; ARM managerbin andtrace
linked via scoped cargo rustc `-C link-arg=-Wl,-rpath-link=...runtime-provider-296/extracted/rust/bin/lib`
plus AGNOS19.8 SDK targetlib path. FFMPEG_DIR alone had staleffmpeg-sys buildoutput;
scoped `cargo clean -p ffmpeg-sys-next --target aarch64-unknown-linux-gnu` fixedsearch.
NativeV4L22cases +codecs5/112packets exact. Appcleanup failureinjectiongreen.
New Params binding at dev-md-resume/params-retirement (oldbinding untouched).

UI passiveDriverPreview integrated bothlayouts (sourcefinal CameraView shaderLarge
on both, onlyrect differs). CameraView gained reset_stream/poll/discard_frame/
draw_region; EGLbind stillperdraw.902purehelpersgreen; actualVisionIPC privateXvfb
:139 RGBpixels144frames exact after fixingexpandedoutline/globalfontscale.
New Draw.rectangle_lines_ex forwards existing nativefunction. ImpactPrompt global
inputfilter and finalrender wired in runtime;159actualsourceclasscases/2643steps
exact; realApp navigationbutton14frames perlayout confirms no clickleak/visibleonly
feedback. Englishcapturesinspected; latest impact_render example adds optionalko
language, rebuild/KOcapture pending. SourceUI en/ko PO merged3way. Details UIvalidation.
Xvfb session47905 runs private :139 from scratch dev-md-ui/native/usr/bin/Xvfb.
Source VisionIPC Pythonbinding built separately at dev-md-ui/vision-python.
Hostraylib requires STARTUP_UI_RAYLIB_LIBRARY=/home/fma1/rust/cache/startup-raylib-host/lib/libopenpilot-raylib.so.

Navigation nativecontrolowner implemented Arcidentity/Weak sharedowner, successful
negotiationonly, secondphone protocolerrorreceiver_busy/1013, releaseonlyownsocket.
Source/native realWebSocket testsGREEN; oldbinary genuineRED beforeport. Nativefull
oldtests63rootbatch includednavtests. Sourcecarrot_navi.py exactpin now.

Upload perrequestincludeAllFiles implemented throughsummary/asyncjob/worker/sync
paths.63hosttestsgreen; catalog1998observations/jobs2099transitions exactsource;
HTTP45boundary pairs +default/all-files actualbyte/hash transfers; sync11pairs/
4scenariosgreen. WebUI wasalreadyimported. New docs dev-md-upload-validation.md.
SourceFiles kind nowString; scopeflag snapshotcamelcase; oldAPI wrappersdefaultfalse.

Sound ARMfresh example6scenarios/153steps/6672samples bitexact nowGREEN; noaudiohw.

CURRENT root work: screenrecord latestsource metadata MP4parser/dateprefix/cache,
info/delete endpoints, devicewallclock and thumbnail1s->0s fallback. Allcoded but
UNVERIFIED except1focused MP4testGREEN. Addedregexdependency. Sourcecatalog/routes
copiedfinalpin. Pending exactsource240binary/21timezone comparator new
rust/tools/check_screenrecord_info.py; knownpotentialDSTnonexistenttime mismatch.
Existing screenrecordHTTP checker updated frozenrouteclock and pairedproviderretry
sequencecomparison; needsfullrun and newinfo/delete/cacheregessions. Root current
Cargo session98780 builds screenrecord+impact_render examples, log
screenrecord-ui-build-v1.log. Slotnext selfdrive navigation/HUD, modelARMJetlink,
plannerpreparation; coordinateexplicitly. Root newtest/sourceinfo false claimsnone.

Agentmodel: actual nativeRust restricted HCQimport/assemble/stage nowHOST+ARM exact
fullgraph/worker metadata; ARM fixture730.89s. Use mdm-native-rust-stage-root assets
manifest8da47e1e...descriptor3a47b86c... forfinalcandidate;48files803918390bytes.
ActiveJetlink realApp/Unixowner/proxy HOST40tests+clippygreen; ARMregression pending.
Model owns narrowKO/EN Jetlinkguides+WikiManualpatch; no publish. Phone/NV12next.
Agentselfdrive: latest295/9436 wholecontrollerHOST+ARM exact withmanagercache,
43tests; Hyundai35520state+35520controller/91271CAN movingcorpus; safety348host+
11669ARMhooks; finalstopping22907transitions/9353packets HOST+ARMgreen. Navigation
640active sourceframes andDMHUD11520vectors/runtimewiring nextcompile; Bosch/ARS
stillpending. Agentplanner: radardpoll192cases HOST+ARM +actualIPCdrain noextra
publicationgreen; nativeDBC103assets45generated plusstage38outputs/repeat0green,
PythonDBCtoolsremoved+candidateCI migrated. Finalbundle deferredsourcefreeze.
Planner NOW OWNS fullsignalobserver/assist+activeplannerintegration beforeS3;
validguard/tracker/geometryHOST+ARMgreen, preparation5760activeframes pending;
malformedguardextension foundandfixed sourcefalseyclassification needsretest.
S3/cluster/display-scheduling queued AFTERsignalclosure. Root retainsremaining
UI/server/startup. All dirtychangesstilluncommitted,devunchanged.

Issue338 progresscomment sent usingissue338-resumed-update.md (ghsession42641).
Finalcoverage/whole-runtime source-matched package/exactheadCI/review stillOPEN;
do notstoporaskdevicehandoffwhilethesearepending.

## Latest continuation after interruption

Root restored existing work; localdev remains b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a.
Root screenrecord240+63DST/Unicode timestampcases now HOSTGREEN; ARMfullserver
examples linked with target FFMPEG_DIR +scopedrpath-link after cleaningonly target
ffmpeg-sys cache. ARMserverexamples screenrecord/catalog/uploadHTTP/sync finalv2
built. ARM info/socket/upload comparisons ongoing. Model owner changed canonical
OnnxBsdThreshold45->94 after actualHOST/ARM producerchecks; sourceParamsbinding
must be rebuilt before fullmanager finalsource comparison (oldbindingretained).

New UIwork listed dev-md-ui-validation: DM2onroad gate pixels48frames exact,
per-side685HOST+ARM exact, display helper1370+1024+288+600HOST+ARM exact,
geometry324GREEN. New HUD/modelelapsedclock/sourceframe refresh, sourcepoint
selection/provenancecolors/signalicons/ShowModelView and modelstatusbadges compiled;
fullrender/runtimeclippy/revisedARM gates still ongoing. Large modelsmoke12states
found None/-1eagersequence difference, fixed pendingrebuild. Do not claim complete.
Root currentlyownsCargo (ARMnav/upload then brief UIbuild/clippy); queued planner
observer, selfdrive ARSprovider prerequisites, modeldelivery/producer followup.

Additional rootpendingserver scope identified: egpu_model.rs needs final recovery/
delivery/error/retry/diagnostics response and downloadendpoint; agentmodelowns native
background --retry-network delivery andproducerstatuses. Native startup retryargv
belongs runtime-launcher/bootstrap/services.rs, notmanagercatalog. Add argv only
when agent confirms CLIcompiled/tested. WholeprojectPython toolremoval/fullruntime/
Panda firmware/finalledger/exactheadCI/sourcefreeze remain open.

Root newest: fullmodelrenderer source/native large510+compact318frames allstate/
orderedcommands/strictRGBA GREEN; UIalltargetsclippyGREEN. HUDfullsuite ongoing with
explicit --preserve-native-fixes sourcecomposition (baseb55552d unknownlimit `--`,
provider/ownerlabels). RawfinalHUD has intentionallyretainedbasefix differences;
recorded without masking. CurrentlargeHUD firstseveral120frame KO/EN suitesGREEN.
New eGPUserver finalstatus/recovery/currentboot/historydiagnostics implemented;
oldbinary26/27RED, newhost22/27GREEN fivefailures onlynestedrecoveryJSONfieldorder.
Native now preserves originalorderedJSON directly; needsrebuild+27rerun+ARM. New
telemetry root/clock/bootpath injection keeps fixturesowned. Sourcechecker loads
finalpinmodules inprocess; no productionfiles/device accessed. Hostserver library
and3nativeEGPUtestsGREEN before fieldorderfix. Modelworkerprogress typedwrapper
latest code compiled as serverdependency HOST+ARM; ownerunitgate stillpending.

CanonicalParams94newbindingbuilt at dev-md-resume/params-final-94 (SHAc6038e7e...),
oldbinding preserved. Native launcher now passes --retry-network (modelowner real
HTTP503->30sretry HOST+ARMGREEN); launcher integration verification pending. Root
Cargo now ARMdashcam_catalog_trace+runtime-launcher after eGPU ARMlinked; needs
briefpostfixhost/ARMserverrelink+clippy thenrelease selfdrive, per queue.

Latestrootgates: egpu-final-host-v2 +egpu-final-arm each27HTTPcasesexact; full106has
102exact/4explicit legacyPythonfake-readiness differences. Nativepositive fixture
now7HOST+7ARMstatesGREEN using actual immutable800MBMDM assets copied once into
ownedregularcache (initialsymlinkfixturecorrectlyrejected). Diagnostics nowmatches
native-provider identityviaowner's publicpackaged::identity onlywheninstalled.json
absent; malformedlegacydoesnotfallback. root-native-package-clippyGREEN forUI/server.
Native launcher8HOST+8ARMownedscenariosGREEN with--retry-network; no actualdevice.
CompactHUD30scenarios2520framesallstate/RGBAexact inclfinalsignals+badges. Large18
scenarios2160framesexact, deviceTPMS paintorderingfixverified additional240frames.
RemaininglargeHUDnavigation/plots/finaldisplay reruns pending. NoactiveCargo root;
slotselfdrive thenmodel. Newrootencoder deadlinefix/test on disk UNCOMPILED:
encoderd/native/v4l/primitives/events.rs preserves original30ms deadline under
realrepeatedSIGUSR1 for encoderonly; decoder unchanged. Test isolatedchild supports
OPENPILOT_POLL_TEST_RUNNER forARM. Needs host/ARM/newsourceC++deadline evidence.

RootCargo was releasedplanner->selfdrive->model; rootcurrentlynoCargo. Native
package diagnostics7HOST+7ARMGREEN (regularfileownedcopy; sourceassetsunchanged);
updated helperIdentityfallback compiledandalltargetsclippyGREEN. Full106eGPU raw
source102exact/4intentionalPythonplaceholder readinesscontractdifferences. Explicit
--native-readiness providerseam composed106suite nowrunning, pairedwith7positive
nativepackage tests. RootbigHUDremaining16scenarios running (navigation/plots/final
signals+badges), compact2520framesGREEN and bigTPMSrepair240framesGREEN.
New encoder poll deadline code/test pendingCargo; original pinned C++3cases pass
HOST+ARM independently from scratchpoll-deadline-source. Startup audit identified
selectedmodel blocking --prepare-for-startup+retry+spinner+lock beforemodelboot;
modelowner nativeCLIwrittenbutunvalidated, rootlauncherwiring pendingowner gate.
Otherrootpending: sharedboundedrebootchime, startup recovery/cacherebuild/native
crashevidence/tmux/clockfloor sourcefeatures; allprojecttools/Pythonremoval; UIcache
optimization disposition/reuse orport; fullruntimeassembly+sourcefreeze/CI/review.

Latest root: encoder deadline realSIGUSR1 storm test passes HOST+ARM (native30msdeadline vs500msstorm); unchanged pinned C++deadline3tests alsoHOST+ARM. DynamicFFmpegARMheader staging needed libswscale link; no runtime headers changed. eGPU composed106native-admission HOST+ARMgreen; positive7realnativepackage and27rawfinalfeatures separatelygreen. Large remaining16HUDscenarios exact allRGBA/state, compact30scenarios2520frames alreadygreen. Root startup model delivery module/tests written UNCOMPILED: --present, sharedblockinglock, selecteddelivery retry, nonblocking2attemptspinner, terminalfailureflag to boot/manager, stopcancels. Owner model adding --present/failuregate/current-selectedmanifest guards; testhooks require next coupledgate. Cargo currentlyselfdrive afterplanner, thenroot startup.

Root latest after resumedturn: startup13ownedHOST+ARM passed, includingselected
model lock/retry/spinner/fallback/cancel/reap. Source --present andfailure/selection
nowverified bymodel35HOST40ARM+750source cases. Fixtureparallelcollision fixed by
copying providerinodes; concurrentHOST+ARM13eachpassed. FullbigHUDremaining16passed;
eGPU106composed+7actualnative+27rawfeatures HOST+ARM passed. EncoderEINTRdeadline
host/ARMpassed. dev remainsb55552d.

Newroot process-crash source/store/nativecollector/sender registered. HOSTsource106
exactmetadata/text/callorder +5store/provider/socket/cancel +actualHTTP2destinations
+18supervisionlib+3lifecycle testsGREEN; clippyGREEN beforelasthelperedit. ARM
source/store/socket+sender+actualHTTPGREEN butmissingexecutable classificationfailed
(QEMUposix_spawn execveENOENT->exit127,stracesaved). Nativecapture nowusesexisting
CapturedCommand errnohandshake; UNCOMPILED followupneedsrootnextCargo. Hosttest
helperdefault target/debug/openpilot-process-child; ARMunit set
OPENPILOT_CRASH_CAPTURE_LAUNCHER=/home/fma1/rust/cache/cargo-target/debug/openpilot-process-child.
HTTPARMchild needs OPENPILOT_CRASH_TEST_RUNNER=qemu-aarch64-static -L SDKroot.
Artifactmap process-crash-arm-artifacts.json. rootCargo releasedmodel->planner,
planner nowboundedrealwire/NumPy sort thenROOTbriefcapturefollowup, thenSELFDRIVE.
Noledgercrashclosureyet. Docsdev-md-process-crash-validation.md scopes it.
