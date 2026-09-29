from __future__ import annotations

import hashlib
import json
from pathlib import Path

import numpy as np
import pytest
from tinygrad import Tensor, TinyJit

from model_export import Bindings, ExportError
from model_export.original import Metadata, OriginalPipeline
from model_export.pipeline import Stage, compose
from model_export.publish import BundleSpec, publish


def pipeline() -> OriginalPipeline:
    inputs = Tensor(np.zeros(16, dtype=np.float32)).realize()
    jit = TinyJit(lambda x: (x + 2).realize())
    jit(inputs)
    output = jit(inputs)
    bindings = Bindings({"x":inputs}, {"model":output})
    stages = (Stage("prepare", jit, {"0":inputs}),)
    metadata = Metadata("fixture", {"x":(16,)}, {"outputs":(16,)}, {"all":(0,16)})
    return OriginalPipeline(metadata, stages, bindings, compose(stages, bindings))


def test_publisher_reuses_immutable_generation_and_preserves_pointer_on_failure(tmp_path: Path) -> None:
    original = pipeline()
    spec = BundleSpec("driving", (1344,760), original, {"model":"12" * 32})
    publish(tmp_path, (spec,), "cpu")
    pointer = (tmp_path / "current.json").read_bytes()
    generation = tmp_path / json.loads(pointer)["generation"]
    index = (generation / "index.json").read_bytes()
    assert hashlib.sha256(index).hexdigest() == generation.name
    entry = json.loads(index)["bundles"][0]
    bundle = generation / entry["directory"]
    assert hashlib.sha256((bundle / "graph.json").read_bytes()).hexdigest() == entry["graph_sha256"]
    assert hashlib.sha256((bundle / "pipeline.json").read_bytes()).hexdigest() == entry["pipeline_sha256"]
    publish(tmp_path, (spec,), "cpu")
    assert (tmp_path / "current.json").read_bytes() == pointer
    assert [path.name for path in tmp_path.iterdir() if path.is_dir()] == [generation.name]
    with pytest.raises(ExportError, match="duplicate"):
        publish(tmp_path, (spec, spec), "cpu")
    assert (tmp_path / "current.json").read_bytes() == pointer
    (bundle / "weights.bin").write_bytes(b"corruption")
    with pytest.raises(ExportError, match="existing generation"):
        publish(tmp_path, (spec,), "cpu")
    assert (tmp_path / "current.json").read_bytes() == pointer
