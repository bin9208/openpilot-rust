# dev-md per-core CPU follow-up — #348

The user reported that the previous correction reduced total device CPU use to
roughly 38%, and requested further optimization of cores 0 through 7. This work
targets `dev-md` only. Parent implementation/review is direct; unrelated Athena
changes and the separate #296 tool conversion are outside this patch.

## Current device baseline

Four consecutive recent full rlogs identify clean source
`0f8125fdab54cb906b8f2d14e0d18c3583177ef5`, branch `dev-md`, device family
`tizi`, AGNOS `19.8-carrot-bt1`. Analysis uses the complete cereal Event schema,
including procLog, deviceState, camera, model and control services. Each segment
contains 30 procLog samples covering about 58 seconds. Raw logs, vehicle identity
and analysis scratch remain private.

| Core | Range of segment mean deviceState CPU % | Highest recorded CPU % |
| --- | ---: | ---: |
| 0 | 45.5–46.3 | 80 |
| 1 | 41.5–44.8 | 76 |
| 2 | 37.6–39.4 | 85 |
| 3 | 68.2–72.2 | 83 |
| 4 | 14.4–21.0 | 41 |
| 5 | 37.8–40.2 | 49 |
| 6 | 42.9–52.0 | 60 |
| 7 | 19.0–22.9 | 32 |

Whole-core `/proc/stat` deltas include kernel/interrupt work. Process CPU uses
matched PID/start-time deltas; 100% means one logical CPU, and a multithreaded
process can exceed that. Executable names come from cmdline because Linux comm
truncates several daemon names to the same prefix. The last processor field is
only where the sampled task last ran, not the distribution of all its workers;
see [proc stat](https://www.man7.org/linux/man-pages/man5/proc_pid_stat.5.html).
These heterogeneous cores need not have equal percentages to be well scheduled.

Panda uses 44.2–44.5% of one CPU (10.4–11.9 user, 32.6–33.8 system), logger
28.5–29.0%, camera 19.8–19.9%, UI 17.9–18.3%, card 15.6–15.8%, and procLog
collection 4.0–4.2%. Htop itself uses 14.0–14.4%, mostly system time, so the
measurement tool is a material part of this observed workload. It is not stopped
or excluded to manufacture a lower product CPU number.

All three main recording indices report fullHEVC; the low-resolution stream
reports H264. Each camera has 1,200 frames per segment and no frame-ID gap.
The largest observed SOF interval is about 51.2 ms. Camera, model, pose, CAN and
control events have no invalid messages in these four segments. This baseline
does not establish acceptance for a new build or another device family.

## Bounded implementation

The original SPI code and Rust port spin throughout the 400 µs phase gap and
400 µs/1 ms transaction gaps. The Rust native clock now sleeps during the coarse
part when more than 100 µs remains, retaining a 50 µs precision tail. Every exit
rechecks CLOCK_BOOTTIME against the original start and minimum gap. Short or
already elapsed waits add no sleep. LinuxIo and the cancellation wrapper forward
the wait to the hardware clock. Virtual clocks retain the original advancing
clock loop, keeping the existing source transaction oracle meaningful.

ACK polling, transfer bytes, checksums, bus/priority/file locks, retry budgets,
500 ms recovery limit, cancellation, CAN 100 Hz cadence, affinities and realtime
priorities remain unchanged. Sleeping can still wake late under load; the
[Rust sleep contract](https://doc.rust-lang.org/std/thread/fn.sleep.html) explicitly
allows scheduler delay. This is why latency and actual vehicle timing remain
acceptance evidence, separate from a host CPU benchmark.

Procfs record reads now reserve 1 KiB before `read_to_end`. Procfs does not provide
a useful file length; starting with an empty vector caused repeated tiny reads
while its allocation grew. The same bounded reader, 2 MiB limit, metadata and
PID-reuse recheck remain. Publication and paced PSS scan semantics are unchanged.
The [kernel proc documentation](https://www.kernel.org/doc/html/latest/filesystems/proc.html)
describes the process identity and I/O counters used for validation.

Logger input buffering was investigated against 86,935 real Event records
(53,390,632 decompressed bytes). Compressed bytes remained identical, but timing
overlap between repeated buffered/unbuffered trials did not establish a reliable
gain. No logger buffering or compression-level change is included. Remaining
SPI ioctl/system time likewise needs actual transport profiling before another
polling-policy change; it is not attributed wholesale to the wait loop.

## Focused evidence

The SPI CPU-release regression first failed with zero sleeps. Deterministic
tests now cover CAN/control gaps, partly elapsed waits, early and late wakeups,
clock wraparound and short/elapsed gaps. The actual native clock also verifies
both minimum gaps. The unchanged C++/Rust oracle passes **122 scenarios,
204 operations and 7,387 ordered syscalls**, including bytes, retry/recovery,
locks, logs, timings and error events. This virtual-clock comparison does not
claim real hardware wake-latency equivalence.

Explicit optimized host benchmark, 2,000 waits per row with ABBA order:

| Gap | Busy wait CPU % | New wait CPU % | New median elapsed | New p99 elapsed |
| --- | ---: | ---: | ---: | ---: |
| 400 µs | 98.7–100.0 | 1.2–2.5 | 405.1–405.2 µs | 431.6–432.7 µs |
| 1 ms | 100.0 | 1.0–1.5 | 1,005.7–1,005.9 µs | 1,037.9–1,045.2 µs |

These are the waiting thread's host CPU/wall ratios, not device-wide savings.
No wait returned before its minimum. Scheduler outliers reached 575.7 µs for
400 µs waits and 1,132.9 µs for 1 ms waits. The benchmark is intentionally ignored
in ordinary tests and runs explicitly with
`cargo test -p openpilot-panda-spi-linux --release --lib turnaround_cpu_benchmark -- --ignored --nocapture --test-threads=1`.

The procfs syscall regression first failed with **60,004 reads for 10,000 stat
records**; the new reader performs **20,004**, including counter sampling.
An optimized 100,000-record ABBA experiment measured 0.831–0.841 s before and
0.633–0.640 s after, with 600,004 versus 200,004 reads. This is a bounded hot-path
measurement, not a 24% whole-daemon or whole-device claim. The explicit production
reader regression is
`cargo test -p openpilot-runtime-core --lib small_proc_records_do_not_repeat_tiny_probe_reads -- --ignored --nocapture`.

Focused host tests for pandad, panda-usb, panda-spi, panda-spi-linux, runtime-core
and proclogd pass **112 tests** with both benchmarks deliberately ignored and
executed separately. Strict all-target clippy passes for those packages.
Existing PID-reuse/FIFO race, snapshot, PSS expiry/pacing and sampler tests pass.
Affected test binaries also build against the genuine AGNOS ARM sysroot; **52
ARM tests** pass under QEMU, including the native clock minimum gaps. The ARM
SPI oracle passes the same **122 scenarios / 204 operations / 7,387 calls**.
CLI suites that launch child executables run on the host; they are not included
in that ARM test count. QEMU is instruction/ABI evidence, not vehicle timing.
Disk checks preceded builds; approximately 194 GiB remained free, with shared
target caches, incremental compilation disabled and two build jobs.

## Delivery and remaining acceptance

At source freeze, exact-SHA PR and post-push CI, complete source-matched AGNOS
packaging and public prerelease/installer verification remain delivery gates.
Their exact SHA, Actions links and release receipts are recorded in issue #348.
The device is not connected, installed or rebooted by this task.
Issue [#348](https://github.com/bin9208/openpilot-rust/issues/348) remains open
until the user confirms startup/drive behavior and per-core CPU, temperature and
camera/model/control timing on the new candidate.
