from __future__ import annotations

import json
import os

import pytest

from model_export.offline_compiler import CompilerCommand, CompilerFailure, OfflineQcomCompiler


def compiler() -> OfflineQcomCompiler:
    command = json.loads(os.environ["QCOM_COMPILER_COMMAND"])
    assert isinstance(command, list) and all(isinstance(value, str) for value in command)
    return OfflineQcomCompiler(CompilerCommand(tuple(command), 60))


def test_real_compiler_emits_named_a630_kernel_when_source_is_valid() -> None:
    # Given a real OpenCL kernel and the genuine ARM A630 compiler provider.
    source = ("__kernel void initialized_add(__global float *out, __global const float *input, __global const float *weight) "
              + "{ uint i = get_global_id(0); out[i] = input[i] + weight[i]; }")

    # When the source is compiled without constructing a QCOM GPU device.
    binary = compiler().compile(source)

    # Then the genuine compiler emits a binary containing the declared kernel ABI.
    assert len(binary) > 256
    assert b"initialized_add\0" in binary


def test_real_compiler_reports_failure_when_source_is_invalid() -> None:
    # Given syntactically invalid OpenCL source.
    source = "__kernel void invalid( {"

    # When the real compiler rejects it, no made-up binary is returned.
    with pytest.raises(CompilerFailure):
        compiler().compile(source)
