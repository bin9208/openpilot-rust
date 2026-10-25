# Original UI adapter sanitizer drivers

`original.json` records 26 independent executions of the two unchanged Python
drivers against controlled compiler/probe processes and real filesystem metadata.
Their source hashes and interpreter are recorded. Captured data includes command
order/flags, selected CXX headers/libraries, successful outputs, missing inputs,
raw logs, partial failures and preexisting evidence. Equal mtimes preserve the
current filesystem enumeration order rather than freezing a capture machine's
order. `nonunicode.md` records the additional Unix byte-path evidence.

Both original and native drivers also built and ran their actual CXX ASan/UBSan
probes on private owned Xvfb displays, which were stopped and reaped afterwards.
The CXX runtime archive, both executables, PNGs, three command logs and driver
streams are byte-identical. Commands/complete receipts match after normalizing
only evidence directories. The model probe verifies 8 create/destroy cycles,
1,024 Unicode font measurements and 2,048 outlines; training verifies 4 cycles,
76 worker decodes, filtered uploads/releases and tutorial rings. PNGs are 5,626
and 208,080 bytes, respectively, and were visually inspected.

Original leak detection remains disabled for these GL probes, with ASan/UBSan
halt-on-error unchanged. Per-compiler capacity checks and owned 1,200-second build /
300-second probe deadlines are explicit execution-policy additions. Host GL
checks do not validate target-device graphics or driving behavior.
