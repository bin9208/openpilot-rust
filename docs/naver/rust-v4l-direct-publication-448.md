# Direct V4L publication into owned message storage (#448)

The #447 implementation reuses a scratch Vec but still copies each packet from DMA into scratch, then into Cap'n Proto. This candidate fills Cap'n Proto's owned `init_data()` storage through `Mapping::copy_into()` and removes the intermediate Vec. It exposes no Rust reference to DMA storage. The original C++ retains the dequeued capture buffer through serialization and requeues after publication; the Rust worker keeps that order, cache synchronization, size and timestamp checks, paired metadata, headers and EOS handling.

Software publication retains its slice interface. The shared message builder preserves clock call order, fields, encode IDs and length-conversion placement. Moving the DMA copy into message construction necessarily samples clocks earlier relative to that copy; it does not preserve wall-clock values across runs. No vehicle timing claim follows from fixed-clock packet equality.

An independent prior `set_data()` field construction checks complete serialized messages at fixed clocks for variable, empty, maximum and shrinking payloads. A second comparison covers all eight services and three codec types. Oversized mapping admission must fail without consuming an encode ID. The warmed benchmark uses identical message construction, clock callbacks, count resets and queue sends for both paths. Its explicit payload-copy byte accounting excludes serialization and queue copies; elapsed values are samples with no speed threshold. Test transports use unique owned queue names.

Parent source review found and repaired shared test queue names, a shifted software failure phase, and unequal work in the first timing harness. Hosted unit and complete encoder/codec/V4L/IPC comparisons, strict Clippy and affected ARM validation remain required. The prior #447 measurement is historical scratch-copy evidence, not a measurement of this direct path. No local functional execution or device test was made.


## Accepted full host proof, 2026-10-21

Sourcea6c3201fe9d6cc228bfa8b54abf60ef0e6d35d82 passed
[run39546722792/job119046445349](https://github.com/bin9208/openpilot-rust/actions/runs/39546722792/job/119046445349),
workflow1667cdf24f63f9c36a31b99b74d33f9f9eace9ad. Its wrapper retains the historical
#447 name; the checkout and actual test sources are #448. Complete wire controls,
full codec/V4L/runtime/IPC comparisons and Clippy passed after fixing only the
missing test namespace (unique runtime queues avoid global environment mutation).

Across80 warmed message constructions and queue sends, selected payload-copy
accounting was205,400→102,700bytes, allocations240→240. The elapsed samples were
880,306/854,076ns and are not a measured speedup. These are the same-builder host
fixture values, not hardware VIDC throughput or vehicle CPU/latency evidence.
The selected accounting excludes serialized-message and queue copies. Affected
ARM verification and user device acceptance remain pending.
