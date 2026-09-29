from __future__ import annotations

from collections.abc import Mapping, Sequence

from tinygrad import Tensor

from .schema import ExportError


def ordered_inputs(expected_names: Sequence[int | str], inputs: Mapping[str, Tensor]) -> tuple[Tensor, ...]:
    keywords = {name for name in expected_names if isinstance(name, str)}
    if len(expected_names) != len(inputs) or not keywords.issubset(inputs):
        raise ExportError("binding names disagree with the captured input contract")
    positional = iter(tensor for name, tensor in inputs.items() if name not in keywords)
    return tuple(inputs[name] if isinstance(name, str) else next(positional) for name in expected_names)
