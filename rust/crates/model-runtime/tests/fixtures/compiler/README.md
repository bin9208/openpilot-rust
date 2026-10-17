# Independent compiler migration evidence

`compiler_corpus.rs` consumes captured data, not the original implementation.

- `original-launch.json`: 4,411 original launch inputs/results, including every Unicode general-category transition representative and genuine A630 binary metadata.
- `original-lowering.json`: original protobuf inputs and output bytes/rejections. Generic cases use unmodified `OnnxRunner` with `DEV=PYTHON`. Offline image cases additionally use `IMAGE=1 FLOAT16=1 NOLOCALS=1 OPENPILOT_HACKS=1 NOOPT=1`; these are reference semantic captures. `NOOPT=1` avoids the original optimized Python image program's failed local-size probing. They do not establish optimized backend equivalence. Runtime-input cases retain the input dtype/shape/bytes. Provenance identifies source hashes and pinned real kernel selections.
- `original-{driver,driving}-import.json`: complete original parser/metadata trees; initializer records retain SHA256 of all original bytes instead of copying the model weights.
- `native-rendered-a630-proof.json`: the native typed float32 Add renderer reproduces independent source and genuine A630 compiled binary byte-for-byte. The current initial renderer also matches42 original output captures when its actual arithmetic/index source is host compiled with only the OpenCL work-item/address-space ABI adapted. This is bounded elementwise coverage, not a complete graph backend.
- `original-memory.json` and `*-memory.json`: 109 original lifetime/TLSF cases plus both actual offline scheduled model graphs. Native tests compare every arena byte count, lane and buffer offset. Includes copy holding periods, held inputs, host offset capability, unsupported devices, zero-size rejection and disabled planning. The actual graph captures do not prove that native lowering reproduces the original kernel fusion/schedule.
- `*-types.json`: every actual node output's original dtype/shape, in generic mode and the complete offline IMAGE/FLOAT16 context. Source model hashes are checked before native comparison.

`full-driver-numeric-unresolved.json` is **failure evidence**, not an accepted numerical gate. It retains exact fixed input identities, model/source hashes, flags, original/native output bytes and measured differences. The first mismatch was localized after nine byte-exact preprocessing nodes to the original IMAGE convolution dispatch. The initial native run used generic convolution semantics, which are insufficient for offline export. Native offline mode subsequently gained explicit image storage/float32 arithmetic semantics; a repeated full comparison still fails (553/553 values, maximum absolute difference 0.004296779632568359). Both runs are retained, with no tolerance acceptance.

`original-offline-image-llvm-unresolved.json` retains the original compiled LLVM results for the small image corpus. Real-weight kernels exhibit floating-point reduction differences from the scalar reference; no tolerance has been accepted. Native reference tests use the separate original interpreter semantic captures above.

Full model numerical parity, backend scheduling/rendering, image packing and fusion, training/nested graphs, all other tinygrad development/test responsibilities and consumer migration remain pending. Native offline mode currently rejects boolean image operands explicitly; the original often rejects these through Invalid padding, while potentially aligned boolean-image cases still need exact admission/packing coverage. None of this evidence authorizes removing the original Python compiler or tinygrad source.

The initial `compiler_backend_cpu` now exercises ONNX import, conservative typed-plan scheduling, native C compilation/linking, real object/library emission, and existing `CpuModel`/`model-run` execution for the supported float32 elementwise/view subset. Independent Add, symbolic/half external binding and three-node Transpose/Add/Mul captures match byte-for-byte with planning enabled and disabled. Pinned source SHA256 is checked before parsing the same bytes and retained in emitted source. Native compiler exit/timeout/empty output/fake ELF/oversized diagnostics and unsupported IR publish no destination; existing destinations are preserved. This is not a complete CPU model compiler: the full image/reduction/fusion and numerical gates above remain open.

`original-typed-preprocess.json` retains thirteen independent unmodified original
CPU:LLVM compiled cases for uint8/float32/half casts and half Add/Sub/Mul/Div,
including every uint8 value, all 65,536 half storage patterns, and half subnormal/tie/overflow boundaries.
The CPU backend explicitly reproduces original CPU:LLVM half emulation: float32
ALU, denormal-zero numeric conversion, and late division rewriting for both
float32 and half. Pure half Identity/same-type Cast/Reshape/Transpose preserve
raw storage bits, including subnormals and signaling NaNs; these copies do not
perform numeric conversion. Dynamic float32 division probes include 7/3 rounding
and reciprocal-overflow with a finite quotient. The OpenCL renderer
retains separate native-half storage/ALU semantics; no device comparison is implied. It also
contains the exact pinned driver's first nine ONNX nodes, their required
original constants, deterministic repeating 0..255 image input and calibration
half ties, and original SHA256s of every intermediate after float32 export.
The original runner realizes each node separately to retain observable storage
rounding. Its backend flags and source hashes are recorded in the fixture.
These preprocessing comparisons do not establish full model or fused IMAGE
convolution equivalence; the full-driver numerical failure remains open.

`original-reduction-schedule.json` retains 28 optimized CPU:LLVM ReduceMean
programs, 14 small/pinned-weight IMAGE convolution programs, and the pinned
actual driver model's first padded convolution. Generated LLVM source and linear
UOps describe original loop ranges, accumulator/register layout, reduction order
and arithmetic contract flags; machine FMA instructions corroborate compiled
fusion. These are independent fixture data, never native compiler inputs.
The native backend compares exact output bytes for all small cases and all
2,764,800 bytes of the actual first convolution by SHA256. Deterministic inputs,
original source/host feature identities and the capture recipe are included.

CPU convolution rendering uses explicit IMAGE half storage round trips, guarded
padded loads, sequential per-output f32 FMA reductions and a final separate bias
Add, matching this observed original target. Standard native libm supplies fmaf
when the host compiler requires it. CPU reductions reject empty/nested/excessive
work and unprofiled product reductions (including IMAGE MatMul); GPU reduction,
image packing, general fusion and full model parity remain pending. Bias is
loaded early in the original kernel but accumulators start at zero; it is not
seeded into the reduction. No Python source or caller is removed by this unit.

`original-math.json` retains original optimized CPU:LLVM Exp2, Sign, contracted
multiply/add, Gelu and Sigmoid programs with raw dynamic float32 inputs. The
proven CPU subset compares 14,752 output patterns including signed zeros,
subnormals, overflow/underflow boundaries, infinities and signed signaling/quiet
NaN payloads. The exact first driver Gelu (`approximate=tanh`) also compares all
2,764,800 output bytes by SHA256 after the verified first convolution. Native
Exp2 uses the original polynomial, FMA and range-scaling order, not libm exp2.
The original CPU renderer rejects an explicit Ops.MULACC; the native IR primitive
here reproduces accepted original `a*b+c` contraction, not that UOp admission.

Gelu `approximate=none` and its default remain explicitly rejected by the CPU
source admission before compilation or publication. Native finite/infinite
agreement does not cover its unresolved NaN sign/payload behavior. Twelve
original ONNX cases and separate byte-export Tensor cases preserve the failure
evidence: realizing float32 output first and fusing a byte export can produce
different original NaN operand selection in unrolled layouts. No universal NaN
guard, payload normalization or tolerance was accepted. General importing and
reference lowering remain available; half activation math, GPU transcendental
rendering and full-model numerical parity remain pending.

`original-storage-alias.json` captures original CPU:LLVM IMAGE/FLOAT16 driver
prefixes 9..59 in two explicitly separate contexts: final-output-only compilation
and per-node forced-realization diagnostics. Prefixes 9..29 agree across contexts;
prefix 30 first differs (33,097/33,120 f32 values, maximum absolute difference
0.0038442611694335938). Its residual reads the earlier computed tensor through
packed half storage. Independently rounding the diagnostic skip tensor through
half storage, then adding the diagnostic Conv output, exactly reproduces original
prefix 30 SHA256 `9cc51a37e9a64eb55709871df15ef0873ab112b9eb23d0f634e605445376b45d`.
The fixture embeds exact prefix 30/59 ONNX bytes and original output hashes, plus
36 small original ONNX graphs covering computed versus external input, ordinary
channel padding versus invertible packing, and jointly realized intermediate
outputs. An intermediate graph output retains f32 bytes while its ALU consumer
may use the half alias. Original source hashes and capture/input recipes accompany
all cases. Four original shared-view captures remain explicit negative backend
admission cases until inverse movement alias maps are implemented. The production
CPU pass handles live direct computed f32 IMAGE inputs only; it preserves generic
lowering, graph-output storage, and explicit existing conversion boundaries.

Native final-output-only prefixes 30 and 59 now match their original hashes;
forced-intermediate-only parity is not used as their acceptance criterion.
This establishes bounded prefix equivalence, not full-driver/driving-model,
GPU scheduling, arbitrary view/fusion, or complete compiler equivalence. Historical
full-model numerical mismatch evidence remains unresolved. No Python originals
are retired by this unit.

Review captures extend this evidence with same-shape Reshape, nontrivial
channel/spatial permutation, and chained movement whose Conv consumes the view
while the residual consumes the underlying computed tensor. Before repair, the
same-shape Reshape case compiled and disagreed with the independent original
48-value capture. Admission now traces every pure-load view ancestor and rejects
arithmetic consumers of any affected ancestor before publication. A computed
Conv-weight consumer is independently captured and remains explicitly rejected;
this partial CPU backend accepts constant IMAGE weights only. A positive original
case with `auto_pad=VALID` and ignored negative `pads` verifies alias eligibility
uses effective padding. These additions retain 37 positive small graphs, eight
negative view/weight graphs, and the two actual driver-prefix gates.

`original-matmul-cpu.json` retains 115 independent original CPU:LLVM IMAGE
MatMul/Gemm captures: 32 finite dynamic-input cases across K=1..1024, rectangular
outputs, transposed weights and bias; 80 dynamic signed-zero, subnormal,
half-rounding/overflow, infinity and signed signaling/quiet NaN input cases.
Each graph/input/expected output is embedded; finite schedules carry original
LLVM and binary identities. Three original computed-weight/shared-operand cases and two nonunit Gemm
scale cases remain negative publication gates until their storage/fusion
semantics are proved. Explicit unit scales and both transpose flags are captured.
The bounded native CPU domain currently requires rank-two f32/half operands,
constant matrix weights, unit Gemm alpha/beta, and no unproved arithmetic
reuse of packed operands.
Generic reference lowering/import support is unchanged.

The actual first driver Gemm prefix (node 97, output f32[1,512]) is reproduced
from the mandatory pinned original LFS model using lossless protobuf field
selection. Both the original source SHA and independently captured prefix SHA
must match before native compilation, avoiding another copy of the large
weights. Native artifact/runtime output matches original SHA256
`339ef10ef2ad5ed8f898065b24f9726a68cd49ab13266f6c49d1d17730da80d1`.

The retained true-final-output prefix survey covers every node 60..96. All match
except node 93: original optimized Conv91+Gelu92+ReduceMean93 context differs in
3/960 f32 values, maximum absolute difference 7.450580596923828e-9. Original
ReduceMean on materialized original prefix92 matches native prefix93 exactly;
this isolates an unresolved fusion/layout boundary rather than proving an
incorrect standalone reduction order. Prefix94's half-storage staging erases
that difference, so later prefix parity does not accept node93. Original full
f32 export output SHA `28468af72225b086839c4ae5de6fa07a1d01e843d02fd915d67f810e438549d3`
is captured, but full native model execution remains incomplete. No tolerance,
cached original-output substitution, or Python retirement follows from these
bounded gates.

Reverse-view review adds six original Conv/MatMul descendant Identity, Reshape,
and same-dtype Cast cases. Before repair, four Identity/Reshape graphs were
accepted with every output value wrong; a held same-dtype Cast after computed
Conv input also returned half-rounded bytes instead of the original raw f32
output. Original and pre-fix native bytes/statuses and native source hashes are
retained. The admission pass now follows view identity in both directions and
rejects arithmetic reuse through downstream views before publication. Pure
view outputs remain raw storage copies, including two independently captured
jointly realized held-Cast/Conv-or-MatMul output graphs. This conservatively
rejects unproved view-mediated arithmetic without altering Concat's separate
padding barrier or accepting general inverse movement maps. The extended storage
fixture has 39 positive small graphs and 14 negative admission graphs, alongside
the two actual driver-prefix gates.

`original-logical-half.json` confirms a separate CPU storage limitation using
five minimal explicit half Cast/Sub/Mul/view-to-f32 graphs. The ONNX output and
lowered native output both have f32 dtype; original final output is cast to f32
before realization, matching the offline export contract. Thus the calibration
prefix 0..2 discrepancy is not a comparison against physical F16 output. For
input `[1+2^-11,-0,0.3]`, original prefix2 is
`[760.93896484375,-0.9574055671691895,12.65090560913086]`, while premature native
half stores produced `[760.5,-0.95751953125,12.6484375]`. The original prefix98
calibration tail matches the former values; image-only prefixes prune this path.

The bounded CPU physical storage profile now separates the logical ONNX graph
from its buffer projection. Unheld computed half Cast/static views/Add/Sub/Mul/
Div use f32 scratch; real F16 inputs/constants/selected outputs and their view
ancestors remain physical half. Semantic IMAGE alias analysis uses the logical
graph first. Schedule byte counts, renderer loads/stores and generated wrapper
pointer types all use the same physical projection; manifest I/O stays unchanged.
The original cast clamp, storage flush, signed-zero and NaN payload rules remain
separate. Thirteen independently compiled held/unheld cases cover Identity,
Sub/Mul/Add/Div and selected Identity/Reshape/same-half-Cast view ancestors;
exposing the half output demonstrably changes downstream float bits. The five
formerly rejected true-export graphs now compare every byte in both binding and
memory-planning modes. The existing four physical/storage positives, all 13
earlier genuine F16 cases and actual image Conv/prefix30/59/97 gates still pass.
At that checkpoint four original half Relu/Concat contexts remained rejected;
this profile does not admit general conditionals, half reductions or math.

The earlier all-nine-node driver hash gate used per-node forced realization,
which inserts physical F16 stores. Those nine diagnostic hashes/input recipes
remain in `original-typed-preprocess.json` with the context correction. A new
independent capture casts all nine selected outputs to f32 and jointly realizes
them once; the native gate now matches all nine true-export hashes exactly.
The scalar initializer divisor255 revealed a formerly hidden FDIV versus
constant-reciprocal multiplication difference. Normal finite scalar initializer
reciprocals now fold like the original. A subsequent independent same-context
infinity probe found that both scalar initializer and literal ±Inf division
simplify to positive zero, including NaN/Inf/signed-zero numerators. The former
native FDIV or numerator-times-zero output was not equivalent. Eight ONNX cases
in `original-infinite-divisors.json` cover f32/physical-F16 and both bindings;
two direct literal cases cover the source scalar-IR rule. Every byte now agrees
in both memory-planning modes. Dynamic denominators retain FDIV behavior;
shared/standalone constant-infinity reciprocal forms reject until their context
is proved. This is an original-backed constant-expression rule, not payload
normalization or a universal NaN guard.

Fresh original/native true-final prefixes0/1/2/8/9 are exact. The known prefix93
optimized Conv/Gelu/Mean fusion discrepancy retains the same hashes and 3/960
bit differences; no tolerance was changed. Prefix30/59/97 remain positive in the
full package gate. Validation: all 134 tests passed in the final full-package run,
along with strict all-target Clippy and fresh current-renderer genuine A630 Add
source/binary equality. Full model parity,
general scheduling/fusion/storage profiles, conditional rendering, other backend
compilation, training/development functionality and Python retirement remain open.


`original-conditional-cpu.json` retains 20 independently compiled pure-f32 ONNX
Concat captures: distinct/repeated inputs, held source outputs, signed zeros,
signed quiet/signaling NaNs, axes0/1/-1 and four nonuniform three-input/rank1..3
graphs. Five further positives cover f32 Relu boundary bits and computed-matrix
Concat with K1/3/4/8. The four paired direct-Add matrix graphs remain rejected;
Concat's original PAD barrier does not authorize direct arithmetic alias reuse.
The native CPU renderer evaluates only a selected branch and intersects exact
IndexExpr subtree intervals within each IndexLess branch. Negative and oversized
load maps still reject; no unchecked load or speculative pointer fallback is
introduced. Branches that would hoist reduction statements reject. Logical graph
admission is checked before physical storage projection and permits only f32
Concat/Relu inputs and outputs; mixed/half conditionals, arbitrary Where and GPU
conditional lowering remain unsupported and reject before publication.

Fresh true-final driver prefixes99/100/101/105/112 match the independent original
export hashes exactly. Prefix100 Relu is a portable regression gate constructed
from the mandatory pinned source and independently captured prefix identity.
Prefix98 combines f32 GEMM with logical-half calibration and is explicitly tested
to reject before publication; full prefix200 has the same unproved mixed-dtype
boundary. Prefix93's optimized Conv/Gelu/Mean discrepancy remains open. Validation:
all 140 tests passed in the final full-package run, including both additional
actual-prefix gates,
strict all-target Clippy passed, and fresh current-renderer A630 Add source and
2144byte binary remain exactly equal to original. This is a bounded compiler
unit; complete model parity/backends and all remaining Python functionality are
not retired.


The following mixed f32-output Concat unit is captured in
`original-mixed-conditional-cpu.json`: 96 original graphs distinguish computed
Cast/SubMul virtual half from physical half inputs, physical versus offline
bindings, selected half output barriers, both operand orders, four shape/axis
layouts, signed subnormals/min-normal/ties/max-normal, zeros and quiet/signaling
NaN payloads. Native outputs match every byte in both memory-planning modes.
Admission permits only logical float/half inputs with a float32 Concat output;
all-half Concat outputs and half Relu remain outside this unit. Four independently
captured half matrix-operand reuse cases remain rejected before publication.

Prefix98 now has a positive true-final export gate. More significantly, the
untouched pinned whole `dmonitoring_model.onnx` source
`dee5a294e8afaacc9295ac5d100e00733ecac278e79264b96331e40a3ede1b04`
compiles and executes through the native CPU artifact/runtime with all 553 float
outputs (2212 bytes) exactly equal to the independently compiled original:
`28468af72225b086839c4ae5de6fa07a1d01e843d02fd915d67f810e438549d3`.
The fixture retains every original output byte, exact names/shapes/dtypes, source
identity, backend flags and input recipe. Both sides use the same untouched
source; every original requested output is cast to f32 before final-only joint
realization. The bounded input is repeating0..255 image bytes and calibration
`[1+2^-11,-0,0.3]`. Native execution uses 406 generated calls, with no Python
fallback or original-output lookup. This one host input is not all-input,
driving-model, device, GPU or complete compiler acceptance.

`original-fusion-boundary.json` retains the unresolved prefix93 optimized fusion
failure: 3/960 float patterns differ at indices46/568/704. The original final
kernel `r_15_4_16_128_24_4` uses UNROLL4/THREAD16, fuses 128-product Conv, tanh-Gelu
and 96-value mean, and has LLVM hash
`7ce852371c1860e4449e1da40e566f32f8485f6e6974ccfa9b4600bc31191020`.
Its schedule/program identities and reproducible capture recipe are retained.
A structural prepublication fence now rejects observable unquantized f32
Conv/Gelu/Mean ancestry. It follows scalar dependencies across views and ALU,
rather than matching node adjacency, names, constants or prefix numbers.
The 23 independent source contexts include 15 rejected direct/view/arithmetic/
no-op/interposed/multi-output graphs and eight accepted held/materialized/dead/
explicit-storage controls. Holding Identity/Reshape/Transpose/partial-Slice/full-
Slice outputs independently makes the original mean materialized, and those
positive cases remain supported. A separate mean on the original materialized
Gelu input also stays exact. The explicit storage boundary preserves the current
whole-model positive; it is not an analytic proof that quantization hides fusion
differences for every input. The actual optimized fusion implementation remains
pending; no tolerance or output normalization was introduced.

Previous checkpoint validation: all 146 model-runtime package tests passed, including
the untouched whole-model byte gate and all review controls. Fresh current-library
A630 Add source and genuine2144byte binary remain exactly equal to original;
strict all-target Clippy also passed on the final reviewed source.
All model Python and broader compiler/training/development functionality remain
pending until genuinely replaced.


`original-half-concat-cpu.json` records the next partial compiler unit. Its 295
independently compiled original Concat graphs distinguish true F16 outputs from
f32 exports, physical versus virtual half, held source/result/view outputs,
repeated/distinct operands, 1/2/3 operands, nonuniform axes and raw IEEE edges.
197 supported cases compare every byte in both memory modes; 98 original-valid
ambiguous physical/held contexts are separate prepublication rejection gates.
Original physical-half PAD+ADD elimination depends on layout and shared-source
context: a universal additive-zero rule changed -0 and signaling NaN bits and
was discarded. These rejection cases are not equivalence positives. Admission
is limited to independently proved virtual and exact uint8-derived half paths.
The existing mixed/f32 Concat PAD barrier and matrix-alias fences remain.

The same fixture pins true final-output-only driving-model prefixes 6/7/8/9,
using exact source `driving_supercombo.onnx` SHA256
`f73a9e535523d5e9acb9e642c64e33d631825dc8ba74123757d107cedd047bb5`.
Inputs are uint8 [1,12,128,256]: img repeats0..255 1536 times, big_img repeats
255..0 1536 times; other offline inputs are zero f32. Only each final prefix
output is exported as f32 and realized once. Prefix source identities,
output names/dtypes/shapes/byte counts and independent hashes are retained.
The first Conv executes 113,246,208 products and its f32 [1,64,64,128] output
hash is `621e5b7b9d987aa11a8c2902d918c9553cc49ab095c99ba4ccb82ee8ab92a435`.
This fixed-input host preprocessing proof does not establish full driving-model,
all-input, GPU, device or complete compiler equivalence.

Prefix8 independently exposed constant-vector reciprocal folding before late
FDIV: 387,072/786,432 f32 patterns differed. The failure hashes and first bits
remain recorded. Normal finite direct initializer vectors (at most4096 values)
now fold before FDIV; ten original f32/half cases compare every byte in both
memory modes. Thirty-two additional positive controls cover equal-shape dynamic
bindings, single normal C+w, shared divisors/multiple requested outputs and
non-growing transpose/reshape/slice/reverse views. Unrelated live NaN/raw inputs
remain exported byte-exact. The 120 reciprocal rejection controls separately
retain special vectors, computed constants, neutral/canonicalized arithmetic,
view aliases, broadcast reuse and unsupported movement producers. Eight of
those use Neg, which is outside the original offline loader's27-op contract.
No output is published for unsupported cases.

Dynamic divisor admission is deliberately partial: direct runtime bindings
through source-tagged Identity/Cast/Reshape/Transpose/Slice/Squeeze/Unsqueeze/
Expand movements, or the independently proved single Add(normal finite nonzero
initializer, such a binding). Every view must be non-growing and have at least
the output cardinality. A growing Expand followed by a shrinking Slice retained
one-byte original/native differences despite equal endpoint sizes; that proof
is retained and the path rejects. This is not a general injectivity proof.
Other Load-producing operators and deeper computed divisors remain unsupported
even when the original accepts them. There is no optimizer-identity enumeration,
fixture/name/dimension allowlist, Python fallback or output normalization.
Embedded graph/input/output data, original source hashes and the joint-scheduling
recipe make the independent small-graph corpus portable after source retirement.
Full fusion/scheduling, remaining driving policy operators and broader tinygrad
training/development functionality remain pending; no model Python is removed.

Final reviewed source validation: all151 model-runtime package tests passed,
zero failed/ignored, including the untouched553-f32 driver output gate, all
actual driving preprocessing prefixes and all218 Concat/reciprocal rejection
controls (including the eight operators outside the offline27-op contract).
Strict all-target Clippy passed with warnings denied. Fresh current-library
A630 Add source and genuine2144-byte binary remain byte-identical to original;
all seven renderer/backend/lowering/storage source hashes match the final files.
The earlier151-test run is explicitly pre-review-repair evidence; only the
reviewed-final run establishes this checkpoint. Full compiler/Python retirement
remains pending.
