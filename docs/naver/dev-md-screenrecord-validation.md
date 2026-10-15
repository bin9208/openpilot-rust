# dev-md screen recording continuation

Issue #338; source e8e152dbcfd9574a1791ed12026d1880e87689b3. Native additions:
filename device-local starttimestamp/label and serverwalltime, bounded ISO-BMFF
metadata parser (movie/video-track timebase, framecount/dimensions/fps), per-file
statkeyed cache, info endpoint, allowed-root deletion/cacheinvalidation, and
thumbnail1s->0s fallback for short clips. No ffprobe/Python runtime dependency.

RED: missing nativeinfo API. Focused MP4/native1test GREEN. Source differential
240synthetic/truncated/random MP4 records matched; Unicodeyear timestamp mismatch
then found (Python strptime accepts Unicode decimalyears). Parser now preserves
source field-specific digit rules and skipped/ambiguous localtime behavior; new
63timezone/Unicodecases now pass after a concrete DST fold fix: Chrono returned
ambiguous offsets in reverse UTC order, so select the minimum Unix timestamp
explicitly to match Python fold=0. Rebuilt example passes240MP4+63timestampcases
in screenrecord-info-resumed/result.json.

Full actualsource/native HTTP+catalog+FFmpeg comparison before Unicodefix:
28policy pairs,60HTTP pairs,13provider command pairs,0differences. Cases include
realMP4 info, cacheddatedinfo, invalidids/methods, shortfirstframefallback, source/
native deletion on restoredsyntheticfile, invalidatedlistcache, missingdelete,
Range/conditionalmedia/downloads, providerfailure/recovery and cachedthumbnail.
Uses verified cached x86FFmpeg externalprovider with rawYUV->H264syntheticinputs;
all files/listeners owned. Current production testdevice media never accessed.

CandidateARM metadata/cache/API tests and final
whole-runtime gate remain pending. Details dev-md-resume/screenrecord-*.

Final ARM metadata comparison passes240MP4records+63timestamps; actual ARMserver
28policy/60HTTP/13FFmpeg-provider command pairs all0differences. The native target
runs in QEMU against owned host filesystem and hostFFmpeg subprocess provider;
this is target server logic evidence, not ARMhardware codec or device validation.
Artifact SHA receipt arm-runners/artifacts.json; scratchscreenrecord-*-arm-final.
