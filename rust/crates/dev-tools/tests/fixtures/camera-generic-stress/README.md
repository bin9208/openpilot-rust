# Independent original standalone camera stress controls

Original `rust/tools/check_camerad_stress.py` SHA256:
`05407fbd24e73550d8236c79d28213021493138d6436eb5b559e3bf0e10803aa`.
The34 policy scenarios are the same independent originals used by the continuous
runtime checker, but this CLI has a distinct contract: strict policy stderr
comparison, arbitrary runner arguments/profile and inherited real-clock child
environment. Twenty-three executions preserve original assertions and serializer
while replacing only child/clock boundaries with captured synthetic replies.
Four positive executions reach all54cases/151observations; failure cases retain
partial artifacts. Nineteen actual CLI captures preserve nargs-star runner rules,
repeat/abbreviation/error ordering and output effects, using mechanical shell
peers only. No device data, runtime fixture lookup or Python wrapper is added.
The actual current runtime fixture's logging differs under this stricter checker;
that original failure must remain a failure. Native regression and exact live failure/positive checker comparisons pass;
the positive lanes use mechanical adapters to original C++ and the native clock.
Three further actual original CLI controls preserve raw Unix runner bytes.
