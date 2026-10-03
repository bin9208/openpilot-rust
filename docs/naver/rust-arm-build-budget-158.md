# ARM workspace CI allowance (#158)

The post-merge Rust run for `2d0ce5591996e23a4c989677dc0e1634d76da776`
was cancelled by the ARM job's 25-minute limit. The
[job annotation](https://github.com/bin9208/openpilot-rust/actions/runs/36830178080/job/110264751754)
reports `The job has exceeded the maximum execution time of 25m0s`.
The GNU workspace build passed; cancellation occurred during the subsequent
static musl workspace build. Other Rust jobs passed. This outcome does not
establish a compiler or runtime defect; the cancelled validation remains recorded.

The ARM job now has a bounded 45-minute allowance, matching the host workspace
job. Both complete workspace builds, native dependencies, runtime checks and
the required aggregate remain enabled. No probe deadline or runtime timeout is
changed. Fresh exact-SHA PR and separate post-merge results are still required;
this configuration change alone does not satisfy runtime or device acceptance.
