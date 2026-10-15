# dev-md per-upload file scope

Source e8e152dbcfd9574a1791ed12026d1880e87689b3, issue #338. Native request boundary
requires a JSONobject and actual Boolean includeAllFiles (defaultfalse). The flag
is carried independently through summary, asyncjob snapshot/child request and
syncupload worker. Default selects one preferred qcamera +rlog; explicitall selects
sorted regular files including zero-length auxiliaries, without followinglinks or
enteringdirectories. An rlog remains required before either selection.

RED: catalog scoped API and per-job scope missing; native tests added before port.
GREEN:63root host tests (server, dashcamupload, navigation); catalogsource comparison
1,929path cases +10defaultfile variants +9all-file variants +14URLs +36reports;
21job scenarios/2,099transitions including independent job flags. Actual asyncHTTP
45boundary response pairs pass, including invalidBoolean/missing object requests;
separate default/all-file transfer tests pass with exact recipient filenames,
bytes, SHA256s, auth and terminalbyte totals. Both use real native Manager+owned
child and fulloriginal asyncengine; allnetworkrecipients loopback. SyncHTTP4scenarios
pass, including all-file byte/hash equality, malformed scope and source error paths.

Existing web all-files selection/translations were imported and built earlier;
this change completes its native transfer path. Native rejects non-UTF8 filenames
explicitly; such names are outside the compared logged-file corpus. CandidateARM
upload/runtime checks and finalwhole-runtime gate remain open. Detailed evidence
under dev-md-resume/upload-{catalog,jobs,boundary,default,all,sync}-final.

Resumed ARM/QEMU actual socket gates: two-phone navigation owner/re-negotiation/
disconnect passed separately; all-files async upload2responsepairs and actual
recipient byte/hash totals match source using ARM server plus ARM worker (QEMU
runner scripts). Sync11pairedresponses across normal/all-files/partial/sessionerror
are exact. Artifact hashes saved arm-runners/artifacts.json; invocation wrappers
are execution adapters, not the target ELF hashes. Hostexternal FFmpeg provider is
used only by screenrecord comparisons, not upload worker validation. Uploadcatalog
pure ARM rerun attempted wrong similarly named serverexample; requires building
`openpilot-dashcam-upload --example dashcam_catalog_trace` at next root slot.
