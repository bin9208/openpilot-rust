# timed source shutdown phase (#162)

The [support-runtime job](https://github.com/bin9208/openpilot-rust/actions/runs/36838817152/job/110292658457)
at `bf07aa4a` recorded a Python GPS/SIGINT exit after 10.044684s rather than the
required two seconds. Its traceback ends at the unchanged source's ten-second
sleep. Native GPS termination took about 31ms and the HTTP cases passed.
Artifact 11150716277 retains that failure; it has not been discarded or relabeled.

The checker waited for a fake date command to append its arguments, which does
not prove the parent source process is sleeping. Thirty local observations at
signal delivery found 18 main threads in `hrtimer_nanosleep`, 11 in `do_wait`,
and one running. All 30 local exits passed, so the CI ten-second delay itself
was not reproduced and its underlying signal-delivery cause remains unproved.
The observed defect is an unreliable readiness condition in a test explicitly
intended to signal during GPS sleep.

The source GPS case now waits at most five seconds for its main thread's actual
`/proc/<pid>/wchan` to report `hrtimer_nanosleep`, retaining syscall/readiness
observations before signaling. Phase evidence is required for that case to pass.
No source/runtime code, signal policy, two-second exit limit, HTTP-progress
check or native shutdown case is changed. Linux `/proc` visibility is required;
missing phase evidence fails instead of being skipped.

Thirty follow-up original-source trials all observed sleep and exited within
66.3ms. The complete six-case source/native GPS/HTTP suite passed, including
both native termination signals. A regression uses an owned child that first
announces readiness while blocked on stdin, then enters sleep; it verifies the
earlier marker cannot satisfy sleep readiness and the subsequent SIGINT exits
within the unchanged bound. Fresh exact-SHA CI and post-merge results remain
required. Local evidence is under
`.analysis/scratch/2026-10-01-rust-timed-shutdown-phase/`.
