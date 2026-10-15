# dev-md native UI continuation

Issue #338; upstream e8e152dbcfd9574a1791ed12026d1880e87689b3. No device validation.

## Passive driver inset

`ui-application/onroad/driver_preview` replaces the onroad driver-state icon with
final-source read-only preview on both C3 and C4 layouts. Existing standalone
camera/setup dialogs retain their separate behavior. The inset never writes
IsDriverViewEnabled and never owns touch events. It polls the existing driver
VisionIPC stream nonblocking at most10Hz, checks both message timestamps against
the monotonic clock plus validity/aliveness/start frame, and checks image SOF age
strictly below0.5s. Hidden/new-ignition transitions replace stream/texture ownership;
external EGL image binding occurs on every draw, including repeated frames.

Native source-pinned pure helper test:902 vectors (256 state combinations,
40 freshness boundaries,600 face crops,6 device rectangles), passed standalone
rustc before production wiring. Metadata in rust/upstream/dm-preview-source.json.
Production preview compiled during the root host batch. Actual VisionIPC/render
lifecycle and pixel comparisons remain pending; this is not complete UI validation.

## Impact notice

The shared controller handshake is implemented separately by selfdrived. UI now
has a global input prefilter and topmost notice render hook. Any press/hold/release
cancels the token and consumes the event before navigation/widget dispatch; held
mouse state is cleared as well. A rendered frame acknowledges visibility only
after all notice labels draw successfully; stale/invalid/full/critical selfdrive
alerts suppress the acknowledgement. Reboot is requested only by selfdrived.

Current impact policy, real render/input adapter and framework hook are UNVERIFIED
pending the next UI compile/tests and exact source comparison. Keep this gate open.

## Follow-up evidence and real render failures

UI library/tests compiled and passed, including2 impact-policy tests and the902
DM helper vectors. Exact pinned ImpactDashcamPrompt class comparison:159scenarios,
2,643transitions, exact token/shown-time/cancellation/countdown state. The native
policy result is recorded at dev-md-ui/impact-policy-host/result.json; actual
input filtering and final GPU acknowledgement still require separate validation.

Real source/native VisionIPC preview tests run in private Xvfb:139,72frames each
for both layouts, with fresh/stale/invalid data, RHD, missing face, distraction,
wheel policy, lockout, alert suppression and offroad/re-entry. Both independently
completed and never changed IsDriverViewEnabled, but the first strict RGB pixel
comparison FAILED60visibleframes/layout: native rounded-outline expands outside
source rectangle, and direct TextDraw skipped source global font scaling. The
native inset now uses the existing exact DrawRectangleLinesEx boundary and source
font scale; pending rebuild/pixel rerun. The failure was not hidden by alpha-only
image comparison or perceptual tolerance. Both inset layouts also deliberately use
the same unenhanced source onroad camera shader, independent of inset size.

The repaired preview now passes exact RGB pixel comparison for all144frames
(72per layout) and exact state traces, with no hidden tolerance. File result:
`dev-md-ui/preview-pixels/result.json` lane=compare; this compares the two captured
actual renderer runs without repeating them. Final source cameraview.py is already
byte-identical to the pin. Both face image paths use real VisionIPC buffers and
software GL in a private X server; external EGL/device-camera validation is still
user-owned. Global ImpactDashcam policy is verified; actual topmost prompt rendering
and consumed-gesture dispatch remain a separate pending integration gate.

Real global UI application input/render test now passes on both layouts: a normal
button click activates once; three cancelling press/hold/release events never
reach the underlying control; fresh visible notice acknowledges, full/critical/
stale/invalid/offroad/reboot states do not. Two14frame runs use actual navigation
stack, Canvas and software GL with captured frames; verified notice text fits the
compact display. The notice hook is used by runtime::Runtime before dispatch and
after root render. Korean/English gettext sources merged from final pin preserving
native additions; native reader consumes .po directly (no Python compilation).
Evidence: impact-render-{compact,large}/result.json. Korean image check pending.

Korean compact actualapplication render/input run also passes14frames; localized
notice capture inspected: title, countdown and cancelhint fit the panel and use
sourceKO strings. The fresh example takes an optional language argument; native
English and Korean glyphfallback both exercised without deviceaccess.

## Resumed full display behavior (2026-10-15)

Both standalone driver-state renderers now suppress onroad DM2-disabled overlays,
while retaining the offroad preview. Actual oldbinary/source RED showed6different
frames. New large and compact24frame VisionIPC/GL runs have exactpixels/traces;
compact actual invalid monitoring publications also match, stopping on onroad entry.

Per-side Xiaoge display freshness now matches the pinnedUI policy, deliberately
separate from card's aggregate-freshness gate. Source/native685cases pass on HOST
and ARM/QEMU, including malformed per-side clocks and stale/future boundaries.
New shared display helpers pass HOST+ARM:1,370radar selection cases,1,024OEM/vision
provenance cases,288signal-suspension/evidence/action cases,600elapsed-clock steps.
Actual source radar helper uses None for unsupported and empty for supported but
unusable data; renderer preserves that distinction and never falls back to stale
control tracks after supported-data errors. Only large-layout source uses these
new displaypoints; compact source retains its existing leadgroups.

New runtime wiring includes liveTracks/customReservedRawData1 subscriptions,
large+compact OEM/vision/combined barrier colors, planner suspension/assist icons,
XState6 preparation label, immediate compact ShowModelView without brightness
coupling, and elapsed-time HUD/model animation/filter behavior. Native UI library/
tests and all examples compiled; UI ARM displayexamples linked. Fullrender/revised
state tests are still ongoing; the addedwire/render behaviors are NOT all validated
merely by helpertests. Geometry324cases pass against final native_geometry fallback:
Float32outputs exact, Float64sampler/point bound1e-11. Initial fullrenderer smoke
has allpixels exact on bothlayouts; large12states exposed eagersequence initializer
None vs source-1, fixed on disk and awaiting rebuiltfullsuite.

Read-only Jetlink/delivery/recovery badges are now connected to native slow-state
refresh and sourceprecedence/font/color selection. The owner verified statusAPIs;
new UIcomposition/pixels and actual producerintegration remain separate gates.
Scratch:dev-md-ui/{dm2-visible-big,dm2-visible-small-v2,vision-side-{host,arm},
display-policy-{host,arm},geometry-final-source,model-final-smoke}.

Raw final-source HUD speed suite has an intentional baseline difference: when
nRoadLimitSpeed=0, upstream displays0 and marks over-speed red, while dev-native
b55552d already usesunknown `--`. The same source diff removes provider-prefixed
slowdown labels and owner/lifecycle-aware navigation labels. Preserve the existing
Rust fixes per the isolation instructions. Actual rawsource output retained in
hud-speed-final (120states exact; pixels differ only unknownlimitbox). An explicit
`check_ui_hud --preserve-native-fixes` oracle composition uses b55552d's existing
compute/limit/navigation methods plus all finalsource timing/signal/badge changes;
its results record the retained sourcecommit. This is not finalsource pixelidentity
and must not be described as such. No tolerance or image masking is introduced.

Rebuilt complete modelrenderer suite now GREEN: large510frames, compact318frames,
including newdisplaypoints freshness/error/empty states, OEM/vision/combinedbarriers,
radarinvalidfallback, XState6 and varyingdisplaycadence. Full source/native serialized
state, ordered drawcommands and strictRGBApixels allmatch (0differences). Initial
fixture zero/rollback messageclock failed inside original SubMasterfrequency math;
real messageclock corpus now strictlyincreases, while zero/rollback remains in the
separate600DisplayClock steps. No original runtime threshold was changed.
UIalltargetsclippyGREEN. Full HUD suite with explicitly retained baseline fixes
continues independently; status/signal composition tests are still pending.

Compact HUD full30sourcecomposed scenarios/2,520frames now exactstate+RGBA, including
new signalpause/status and all delivery/Jetlink/recovery/active badgeprecedence
cases inKO/EN. Largefirst18scenarios/2,160frames exact; deviceTPMS scenario revealed
actualpaintordering difference: finalsource experimentalbutton draws after static
HUD paint, native still drew it before TPMS, changingoverlap. Moved nativebutton
render afterdate/TPMS/badge while preserving input/render cadence; pendingrebuild
and focuseddeviceTPMS plus remaininglarge navigation/plot/finaldisplay scenarios.

Continuation: large HUD remaining16scenarios (navigation, plot modes, final signal and delivery/recovery/Jetlink badges, Korean/English) passed all exact states and RGBA frames. Combined with earlier18scenarios/2160frames and corrected TPMS2scenarios/240frames, all current large-HUD scenarios passed. Explicit preserved-native-fixes oracle remains documented above; this is not raw-source HUD identity. Compact30scenarios/2520frames and both ModelRenderer suites510+318frames passed. GPU layer cache/full runtime gates remain open.


### Active final overlay cache continuation

Final source actually owns OverlayLayer/OverlayState in both model/HUD layouts;
this supersedes the earlier audit's stale reverted-cache exclusion. Native
PaintLayer preserves the caller Canvas and actual FBO, with shared source-key
invalidation and urgent warning/blindspot/lane-change/touch direct rendering.
Model display clocks advance only when painted; HUD parameter/plot/traffic state,
input handling and speed animations keep their original per-frame cadence.

Frozen original class fixtures: 2,048 layer transitions (all private state and
FBO calls) and384 actual cereal OverlayState snapshots pass HOST and ARM/QEMU.
The native backend's actual HOST OpenGL foundation matches320 complete pixel
hashes across two dimensions, two scales and ordinary/recording targets,
including painter exceptions and nonzero caller-FBO restoration. Both model
owners with cache enabled pass510+318 frames, every command/state and pixel.
Compact HUD8 scenarios/960frames and large HUD6 scenarios/1,120frames pass with
cache enabled, including traffic expiry/numeric edges, signal changes, plot
history/modes and Korean/English. HUD references explicitly preserve the
existing b55552d absent-limit/navigation-owner fixes. Source-only GUI harness
captures the active FBO at frame start exactly as the final application does;
the unchanged working-tree application wrapper predates that final property.
The 384 state gate separately verifies actual source key/urgent decisions;
layer-owner suites inject bounded enable/urgent sequences with a stable key.

Native source compatibility review then found the ES2 raylib framebuffer stub:
rlGetActiveFramebuffer returns zero. An owned shared-library ABI fixture with
that exact stub reproduces caller FBO loss before correction. The correction
resolves glGetIntegerv once for the ES2/ES3 provider and reads
GL_FRAMEBUFFER_BINDING without per-frame allocation/lookup. The correction's
GPU/ARM gate is pending; HOST desktop GL is not device graphics evidence.
Complete pinned augmented-road owner tests with real VisionIPC are in progress.
Final runtime package, recording boundary changes and device acceptance stay open.
Scratch receipts: .analysis/scratch/2026-10-15-dev-md-resume/overlay-cache/.


ES2 correction GREEN: the zero-returning ABI fixture passes all320 actual HOST
OpenGL source hashes and restores every nonzero target. Actual ARM links and
8 native library tests pass; no ARM/device GPU invocation. Complete pinned
AugmentedRoadView classes now pass512 real VisionIPC frames (two layouts,
English/Korean, camera/alert/cluster/view-mode changes), all camera state and
RGBA exact. Large UI uses the same explicit b55552d retained navigation fixes
as the HUD corpus; raw-source absent-limit differences are separately retained.

Final application review exposed further deltas rather than closing from the
layer-only result. Dynamic recording's72 original argument lists initially
failed (capture FPS ignored the every-third-frame policy); native six-digit
formatting now matches all72 on HOST/ARM. Frame-start render target capture
initially failed an actual GL mid-render enable test. The frame snapshot is now
paired through begin/end/readback; unused recording targets release only after
presentation, retained scale/burn-in/CLI targets survive. Cluster explicit
capture retains its separate API and actual pixel regression. Bounded encoder
queue capacity is checked before GPU readback; actual native blocked-writer
fixtures pass HOST/ARM. Twenty real input frames produce60 output frames,
3.000000seconds at20FPS for the default20/3 capture rate. Both HOST and ARM
orchestrators use the HOST ffmpeg codec in this check; this is not ARM codec
performance or device evidence. HOST all-target Clippy is green. Queue fixture
scheduling was subsequently made deterministic and awaits its next rebuild.

Final-font review now also reproduces missing Jamo/common symbol codepoints;
Korean text must select the display font even in a non-Korean locale. Native
codepoint/fallback changes and pinned complete graphics-source harness are
written, with80 source policy branches and full codepoint arrays, but pending
fresh native/pixel/ARM gates. Earlier graphics receipts predate this font change.


Final fonts/labels GREEN:80 actual source fallback branches and three complete
source codepoint sets pass HOST/ARM;72 real raw-font renders/measurements
(English/Korean, Latin accents, Hangul, Jamo, Hanja, symbols and emoji) are exact.
Original final application/text_measure/text_draw now run directly from the pin
in the graphics oracle. Both cached model owners828frames and full Road512
VisionIPC frames re-pass against those final graphics owners.

Complete label review then caught whole-label font retention: mixed ASCII/Hangul
wrapping/elision/emoji splits used different fragment fonts. Three new scenes
failed with31,383/11,305/8,673 differing pixels. Measure::resolve_font now selects
one effective font for the entire label, cache key, wrap/elide and emoji drawing.
The first corrected mixed scene still had four one-channel differences: source
adds the unrounded double emoji advance to the Float32 cursor, whereas native
had rounded the advance before addition. Promoting before addition removes all
four; no pixel threshold changed. All nine shared widget scenes pass exact state
and pixels (the final rerun focuses the three changed mixed scenes and prior
emoji scene). Actual XTest keyboard/click, pause/profile and both CLI/dynamic
recording lifecycles also pass against the pinned final graphics source.
Dynamic recording now correctly yields nine encoded frames for three captured
inputs; source matched. Shared framework all-target strict Clippy passes, ARM
framework examples link and the complete UI application ARM check passes.

Measurement-cache omission is explicit: native text_layout/Renderer call the
native raylib measurement directly with typed font resources, and hold no global
text-measure cache; therefore source cache hash-key collision,2048-entry eviction
and long-text retention hazards do not exist in that path. Actual measurements,
whole-label font selection and rendering are independently compared above. This
is allocation/ownership equivalence evidence, not measured CPU or GPU savings.
The complete application/source tools and source-matched runtime package remain
open; do not extrapolate these graphics gates to vehicle acceptance.
