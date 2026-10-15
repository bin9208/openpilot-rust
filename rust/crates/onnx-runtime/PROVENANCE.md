# Lazy external ONNX Runtime boundary

The policy and model interpretation belong to Rust callers. This crate only
loads the exact external ORT1.23.2 provider, checks named Float32 tensor contracts,
invokes its synchronous C ABI and copies owned output tensors. CPU sessions use
one intra-op thread, one inter-op thread and disabled intra-op spinning, matching
`tools/signal_analysis/neural_signal.py` at
`e8e152dbcfd9574a1791ed12026d1880e87689b3`.

`rust/upstream/signal-runtime.json` records the official MIT-licensed ORT source
revision, release archive identities, every extracted file hash and notices.
Builds verify the exact C declarations; runtime provider verification checks the
entire selected architecture tree before `dlopen`. Model bytes must match the
caller's expected source identity. The library is loaded per owning session and
released only after all session/environment objects. No ORT library is linked
with `DT_NEEDED`; the provider is optional for classical signal processing.

The CXX adapter is an external ABI boundary, not a second neural policy runtime.
Both Rust and C++ independently check input shape products and names. CXX
translates exceptions and owns copied output vectors; input bytes remain borrowed
only during synchronous execution. Sessions are confined to their creating Rust
thread. No Python runtime, downloaded pickle execution, implicit accelerator
selection or automatic provider fallback is implemented.

Validation and outstanding producer/provider/tooling limits are maintained in
`docs/naver/dev-md-signal-validation.md`. In particular, model tensor execution is
separate from signal preprocessing, box sorting, temporal association, worker
ownership and real producer-to-planner subscription validation.
