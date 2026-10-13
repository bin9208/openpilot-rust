# Screenrecord unhandled-error connection lifecycle (#244)

During #242's timeout correction, the native Internal-error response omitted
the original aiohttp unhandled-error connection closure. A short owned FFmpeg
strict-UTF8 failure reproduced the difference through the actual Application:
the original returned 500 with `Connection: close`, then EOF and no response
to a follow-up request. Native returned 500 without that header and accepted
the follow-up request with 200.

An explicitly handled FFmpeg nonzero-result 500 is a different source path:
both implementations kept the connection open and accepted a follow-up 200.
That control must continue to pass. The correction is confined to
`Failure::Internal` in the Screenrecord adapter, adding the close header and
Hyper's `CloseAfterResponse` marker. Global HTTP error policy is unchanged.

Raw responses and socket observations are retained under
`.omo/evidence/carrot-server-225-resume/dashcam-media/capture-consumers/`.
The corrected persistent-socket observation passes in the adjacent
`capture-socket-green/` directory: unhandled 500 now matches the source close
header, EOF and absence of a follow-up response; the handled HTTP500 control
still retains its original connection behavior. Three command pairs also
match, and the native example exits normally. The final example SHA256 is
`ecbaa971f93266fd6202dc011852243ef104817f7400cce3c3ebd9f9eff4a6f6`.
Strict Clippy, formatting and diff checks pass. #242's separate 90-second
timeout proof is reused for this response-only change. No real recordings,
devices or external recipients were used. Complete server integration remains
open.
