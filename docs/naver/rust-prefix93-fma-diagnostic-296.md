# Issue 296: prefix 93 original LLVM18 numerical diagnostic

This records original-runtime diagnostics produced on the disposable
`codex/feat-296-node280-original-capture` branch. Its one-off workflows and
Python drivers are not Rust replacements and must not be merged into `dev`.
The main Rust compiler and its prefix-93 fusion fence were not changed. All
original execution below ran in GitHub-hosted Actions; local work was limited
to inspecting sources, receipts, and object code.

## Reproduction and target identity

The [original prefix-93 capture](https://github.com/bin9208/openpilot-rust/actions/runs/39488465388)
(artifact `12294054127`, source commit `0c2d4d0806b6c802f81dcfb52066c20451ea6ba8`)
used the pinned ONNX SHA-256
`dee5a294e8afaacc9295ac5d100e00733ecac278e79264b96331e40a3ede1b04`,
`CPU_COUNT=16`, and pinned LLVM18 provider SHA-256
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`.
Its fused prefix-93 LLVM IR SHA-256 is
`7ce852371c1860e4449e1da40e566f32f8485f6e6974ccfa9b4600bc31191020`;
the materialized Gelu92 IR SHA-256 is
`f2c45130cd720e831837744139a88264c8bdc174c25be8603b7e36356e559585`.
The IR has no embedded target CPU or target-feature attributes. The historical
receipt did not record the effective CPU target, so the archived object is the
codegen identity gate.

The [target probe](https://github.com/bin9208/openpilot-rust/actions/runs/39499383851)
(commit `39486e7bcaeac8f64d6a99c73e285d61859580d8`, artifact `12299865248`,
receipt SHA-256 `b916f38ab13d8d01da51dce09b234b6e24221ab8bd7944b8c5d013d54be9110a`)
found host CPU `znver3`; recompiling its sealed instrumented IR with
`LLVMCompiler("x86_64", "znver3", "")` reproduced the accepted object SHA-256
`56e5834332bff36478d208f56fdb7e470562db24353947487f96a21f4115a46a`.
Other probed targets produced different objects. An earlier unpinned run emitted
AVX-512 instructions and changed the output; its traces were rejected. Thus
subsequent comparisons explicitly pin `znver3`, rather than treating a different
GitHub runner's native target as equivalent.

The [pinned replay and stage capture](https://github.com/bin9208/openpilot-rust/actions/runs/39499924401)
(artifact `12299571657`, receipt SHA-256
`ce841feb74b5277fc086dcff427453d09d935737c3e52beb442432ab38e05b4e`)
recompiled both archived IR sources. Its uninstrumented fused object SHA-256
`e93d6b49c1bd264ce9ad09d52aa2df3076bf2d347ba04ff51f11e486cd48b13a`
and materialized object SHA-256
`180e40bedd4fe707fc668f24404452177416b0d8f9b91eaf192f571b24fdb364`
match their historical objects exactly. Running with the archived input-owner
buffers reproduced fused output SHA-256
`7307a9b9129f7650c5793a6098dfb30f1830699e24f7a94c3936d6d6c9b57ee5`
and materialized Gelu92 output SHA-256
`af9e1224e9f618475b0220047b9aae4727fc188d81e85bbcd93acdf4a70899d6`.
Every admitted instrumented trace also had to preserve its corresponding exact
historical final-output hash.

## First difference and machine-code cause

The [pre-Gelu capture](https://github.com/bin9208/openpilot-rust/actions/runs/39496577925)
(artifact `12297344527`, receipt SHA-256
`b356ea7b22f613bab7e208fc7c9b36aae3fd7cf9cf0a257e653b03d74a11a352`)
found all 92,160 Conv-to-Gelu f32 inputs bit-equal between the fused and
separately materialized paths, including the separate Conv91 result.

The [pinned Horner capture](https://github.com/bin9208/openpilot-rust/actions/runs/39500237507)
(source commit `fb27016090108ac51ead5df750e052caade9a899`, artifact
`12299476948`, receipt SHA-256
`e3f32d83d38623a80b205b33f6ecc484515d22cb297bbc15f838cf839f78b93b`)
retained both historical final-output hashes. Admitted Horner stages 00–03 and
05 were bit-equal at all 92,160 elements. Stage 07, the addition of f32
`0.24022644758224487` after the preceding Horner multiply, was admitted on
both sides and matched at 89,365 elements; 2,795 differed. The three relevant
`(channel, spatial)` positions already differ there:

| Position | Fused bits | Materialized bits |
| --- | --- | --- |
| `(46, 54)` | `3e80c2f0` | `3e80c2ef` |
| `(568, 21)` | `3e5f8ca2` | `3e5f8ca1` |
| `(704, 78)` | `3e5dacde` | `3e5dacdf` |

Both original IRs use `fmul` followed by `fadd` at this stage with LLVM fast
math flags including `contract`. The pinned materialized original object has
`vfmadd213ss` instructions for this multiply/add, with relocation to
`.LCPI0_19` (the `0.24022644758224487` constant). The pinned fused original
object instead has scalar `vmulss`, lane insertion, then `vaddps` using the
same constant from `[rsp+0x10]`; that stack slot is populated from the
`.LCPI0_19` broadcast. The relevant fused instructions are at offsets
`0xa63`/`0xa6d` and `0xb79`–`0xb90`; the materialized FMA appears at
`0x104a`, `0x112f`, `0x11f3`, and `0x12ab`. These are direct disassembly
observations of the *historical, bit-reproduced* objects, not inferred from
ULP distances. The same source arithmetic acquires a different intermediate
f32 rounding boundary from LLVM contraction choices in the two kernel shapes.

As a controlled intervention, tracing materialized Horner stage 06 forced
that intermediate to materialize. It changed the materialized final output at
exactly 46 positions, all to the fused original Gelu values, producing SHA-256
`d61fa8c59bbfe96d32d706a73702cce5fdc0577c7e72f0cfe0c9a76ad630791b`
(the independently captured fused Gelu trace from
[run 39493781996](https://github.com/bin9208/openpilot-rust/actions/runs/39493781996),
artifact `12297535837`). **This step-06 trace is not admitted as an original
intermediate oracle:** its final output changed. It is evidence from a
deliberate rounding intervention. Other output-changing traces were likewise
rejected rather than used for localization.

## Native repair scope

The discrepancy is in original LLVM18 contraction of the Gelu exp-polynomial
multiply/add, not Conv values, Gelu constants, or an unchecked CPU-target
change. A native fix should express the appropriate f32 rounding/materialization
boundary from the graph's fused dataflow while retaining separately materialized
Gelu behavior. A node-number exception, tolerance relaxation, or seeding the
native candidate with original intermediates would not establish whole-model
compiler correctness. Keep the prefix-93 native fusion fence until a generic
policy is implemented and checked against both original paths and the existing
LLVM18/20 prefix and whole-model gates. This diagnostic alone does not prove a
native fix or full compiler acceptance.

## Proposed generic compiler policy (review before implementation)

Source review of the current Rust compiler shows why changing the single C
exp2 helper would be incorrect. `compiler_onnx/render/math.rs` uses
`__builtin_fmaf` at every exp2 polynomial step. The ordinary C compiler is
invoked with `-ffp-contract=off`, so this explicit FMA is retained. The
configured LLVM direct-Gelu boundary in
`compiler_backend_cpu/activation.rs` emits `fmul` then `fadd` with `contract`
flags and lets the selected LLVM target decide which pairs contract. The
schedule selects that boundary only for a standalone, direct f32 Gelu call.
`compiler_onnx/storage.rs::admit_cpu_observable_fusion` currently rejects the
unproved unquantized Conv/Gelu/Mean ancestry before compilation. These paths
must not be conflated: replacing every polynomial FMA with separate operations
would break the independently verified materialized Gelu output.

The generic rule should be **boundary- and target-driven**:

1. At an actual scheduled storage write, preserve its dtype conversion and
   f32 rounding before a later kernel load. An unheld logical graph node or
   in-kernel temporary is not automatically such a boundary.
2. Within a candidate fused kernel, retain each original `fmul`/`fadd` in
   ordered scalar SSA with the original `nsz arcp contract afn` flags. Do not
   pre-rewrite all pairs to FMA, globally disable contraction, insert a
   node-specific round, or route through the standalone `native_exp2` C helper.
   Lower the *whole admitted closure* with its genuine consumers, reductions,
   lane grouping and source input ownership, then use the explicit LLVM
   provider/target configuration. LLVM decides where contraction survives in
   that concrete kernel shape. The historical fused and materialized objects
   demonstrate that the same arithmetic DAG can yield different contraction
   boundaries when its kernel context differs.
3. Keep separately materialized Gelu on its existing path. Introduce a
   target-owned fused closure only after source-derived graph ancestry and
   physical schedule prove the input, output and intermediate ownership. A
   failed proof leaves the current structural rejection intact. The rule is
   graph- and target-based, not based on node index 93, tensor names, or an
   observed output lookup.

The existing `compiler_backend_cpu/llvm.rs::Config` already requires explicit
provider SHA, triple, CPU, features and passes. A proposed fused artifact must
record all of these plus source/optimized-IR/object hashes. `znver3` is the
effective target of *this LLVM18 original receipt*, not a universal CPU pin:
LLVM20 and other accepted targets need their own original provenance. There is
no proven target-independent rule saying that every fused exp-polynomial
multiply must round at step 06. The one accepted original object and the
intervention identify this occurrence, not all possible graph shapes.

For implementation review, begin with a cold, true-final-output prefix93
candidate constructed from the genuine input recipe and owned original model
prefix, with no original intermediate fed to Rust. Inspect generated IR and
object for the disputed multiply/add and compare every 960 f32 output bits
against the independent fused original. Separately verify materialized Gelu92
and its following mean remain exact. Require both LLVM18 and LLVM20 original
target/provider captures before claiming a cross-provider policy; LLVM18's
`znver3` result alone cannot stand in for LLVM20. Run the already retained
earlier-prefix and later-prefix corpus, 279/280, source mutation rejection,
and the full-model gate afterward. Keep the structural fence for every closure
whose source schedule or target behavior lacks an independent exact-output
receipt. This is an evidence plan, not permission to relax any existing gate.

Parent source/data review independently rehashed every stage payload, checked
all admitted final-output hashes, counted the2,795 stage-07 differences and
verified the three listed raw-word pairs in canonical channel/spatial order.
The historical object hashes and displayed multiply/add versus FMA instructions
were also independently checked with read-only disassembly. No original or
native functional program was executed locally for that review.
