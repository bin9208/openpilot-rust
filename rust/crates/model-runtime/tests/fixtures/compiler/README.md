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
