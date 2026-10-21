# Cweb redirect Location encoding (#450)

The production HTTP redirect path previously built a temporary String for every
Location byte and collected them into another String. The extracted encoder now
appends the identical literal ASCII or uppercase `%HH` representation into one
owned buffer, reserving at most three bytes per input byte. Methods, URL joining,
redirect/scheme limits, response consumption, deadlines and stop polling remain.
The reserve may exceed final encoded length; this is not a peak-memory claim.

Tests preserve an independent copy of the prior expression and compare every one
of256 byte values, mixed/empty/long inputs, percent signs and punctuation. The
allocation child runs only its exact test under a30-second deadline, with kill/reap
on timeout. It measures the actual production helper and checks the output
fingerprint, allocations and reallocations without a CPU or elapsed-time claim.
Stats_alloc0.1.10 is dev-only and already source/checksum-pinned in Cargo.lock.

Hosted RED [39554193757/job119070006066](https://github.com/bin9208/openpilot-rust/actions/runs/39554193757/job/119070006066)
checked unchanged production source3e4011cc3b9f62445a04c608f84c0f6badd6e2d3,
workflow4047e6b5937a697a9c9b308a702c3b0199279ca9. Exact byte controls passed;
the required1,000-allocation gate rejected the original256,000 allocations.
The diagnostic job succeeds only when that intended Rust test fails with101 and
the expected allocation assertion and measurement record are present.

Hosted GREEN [39554813792/job119072003793](https://github.com/bin9208/openpilot-rust/actions/runs/39554813792/job/119072003793)
checked3a90dc52732b9afcde31c31ae20578bf3109e532,
workflow168db2771933b69dce64706d323010167fc08308. Complete package tests,
strict Clippy,32original policy cases/16,000frames,20actual HTTP comparisons,
six daemon modes including blocked-HTTP cancellation, and address ASan/UBSan
all passed. Parent inspected artifact12324938610,45files, valid ZIP CRC,
SHA256 `301028175c852ebbc8d851c11f8f663a6143454091d9de5462374abca0ffb7eb`.

For1,000 calls on the same256-byte input (all byte values,580encoded bytes),
allocations fell256,000→1,000 and reallocations169,000→0. Allocated-byte counters
were2,406,000→768,000; reallocated-byte counters1,988,000→0 are reported separately
and must not be added to the former. One cold call changed256→1 allocations,
169→0 reallocations and2,406→768allocated bytes. Fingerprint
6501791830646170789 is identical. This is a helper allocation fixture, not typical
URL distribution, network throughput, CPU, latency or device performance.

The maintained UI/connectivity job now requires full package tests/Clippy before
all four existing original/native cweb comparison commands. Latest full PR and
affected ARM verification remain pending. No Python cweb tool was removed here;
those still execute the original comparisons. No local functional test was run.
