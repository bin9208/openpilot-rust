# Native FFmpeg captured-pipe deadline (#242)

The original POSIX screenrecord FFmpeg call uses `subprocess.run(timeout=90)`.
Its deadline includes both direct-child completion and stdout/stderr EOF. The
initial native runner waited for the child first, then unconditionally joined
two pipe-reader threads. A child that exits while its descendant retains the
pipes can therefore leave the native request waiting beyond the source timeout.

An owned source/native HTTP comparison on 2026-10-08 reproduced the difference:
the original returned 500 after 90.0945 seconds, while native returned 200 after
95.0451 seconds. Both used an owned FFmpeg executable whose direct child exited
promptly and whose descendant retained the captured pipes for 95 seconds.
The native candidate SHA256 was
`5ffe0693d3c303db67b0a9e8235909ec554ee0f53483defb324fad3f360e1f63`.
Responses, invocation, process exit and owned-child cleanup are retained under
`.omo/evidence/carrot-server-225-resume/dashcam-media/screenrecord-timeout-red/`.

A separate unchanged GitConfig source control, which uses the same POSIX
`subprocess.run` boundary, returned after 15.015966 seconds at its 15-second
deadline while an owned descendant still held the pipes. This confirms the
target timeout policy: kill/wait for the direct child and close captured pipes;
do not perform the Windows-only post-kill `communicate()` drain.
That receipt is `.omo/evidence/225-git-config/source-timeout/result.json`.

The corrected process-supervision capture boundary passes the same real
90-second comparison: source returned 500 after 90.094759 seconds and native
after 90.002133 seconds, with matching 55-byte error bodies. The native process
exited normally; the fixture cleaned only its two recorded descendant PIDs.
The receipt is under the same evidence root in `screenrecord-timeout-green/`.
Six focused capture tests cover separate raw output, nonzero/signal status,
failed launch, a descendant holding pipes, a live child with closed pipes and
missing-pipe setup cleanup. The borrowed-lock launch has one new actual
separate-stream/non-session check and three retained lock-FD checks.

Screenrecord and active Dashcam media retain direct FFmpeg launch,
strict UTF-8 decoding and original command/result policies. GitConfig retains
its own borrowed-lock launch and replacement-decoding behavior. The shared
primitive must return raw stdout/stderr and process status under one deadline.
Nine focused consumer response pairs and nine command pairs pass, including
cache bypass and recovery. The timeout comparison also exposed an independent
unhandled-error connection mismatch, tracked separately in #244. Two additional
actual FFmpeg success/cache responses and their command pair match using an
owned synthetic video. The final process-supervision and Carrot server strict
Clippy/format/diff checks pass. Source identities and criterion-to-artifact
mapping are in `dashcam-media/capture/receipt.json`; after the workstation
restart, the nine recorded source hashes and retained final example SHA256
`ecbaa971f93266fd6202dc011852243ef104817f7400cce3c3ebd9f9eff4a6f6`
were verified without repeating the completed scenarios. Full server integration
remains open. No device, NAS, real recording or production process selection was
involved.
