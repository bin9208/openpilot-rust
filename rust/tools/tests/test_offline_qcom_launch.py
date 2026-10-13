from __future__ import annotations

import itertools
import math
import struct
from pathlib import Path
from typing import Final

import pytest
from tinygrad.uop.ops import ProgramInfo

from model_export import ExportError
from model_export import offline_schedule


FIXTURE: Final = Path(__file__).parents[2] / "crates/model-runtime/tests/fixtures/qcom/buffer_add.bin"
SOURCE: Final = "__kernel void run(__global float *out) { out[get_global_id(0)] = 1.0f; }"


@pytest.mark.parametrize("items,groups,local", [
    ((128, 1, 1), (2, 1, 1), (64, 1, 1)),
    ((6, 12, 8), (3, 3, 1), (2, 4, 8)),
    ((3, 5, 7), (3, 5, 7), (1, 1, 1)),
    ((1, 2, 128), (1, 1, 4), (1, 2, 32)),
    ((1, 1, 1), (1, 1, 1), (1, 1, 1)),
])
def test_launch_covers_each_item_once_when_local_size_is_missing(
    items: tuple[int, int, int], groups: tuple[int, int, int], local: tuple[int, int, int],
) -> None:
    # Given global work-item counts including odd and multi-axis dimensions.
    info = ProgramInfo(global_size=items)

    # When the offline launch geometry is selected without executing the kernel.
    actual_groups, actual_local = offline_schedule.launch_dimensions(info, SOURCE, FIXTURE.read_bytes())

    # Then group/local IDs cover exactly the original item domain without padding.
    assert (actual_groups, actual_local) == (groups, local)
    visited = [tuple(group[axis] * actual_local[axis] + lane[axis] for axis in range(3))
               for group in itertools.product(*(range(size) for size in actual_groups))
               for lane in itertools.product(*(range(size) for size in actual_local))]
    expected = set(itertools.product(*(range(size) for size in items)))
    assert len(visited) == len(expected) and set(visited) == expected
    assert math.prod(actual_local) <= 64


@pytest.mark.parametrize("items,groups,local", [
    ((2**32 - 1, 2, 1), (2**32 - 1, 1, 1), (1, 2, 1)),
    ((2**32 - 2, 1, 1), (2**31 - 1, 1, 1), (2, 1, 1)),
])
def test_launch_retains_integer_bounds_when_items_are_near_u32_limit(
    items: tuple[int, int, int], groups: tuple[int, int, int], local: tuple[int, int, int],
) -> None:
    # Given dimensions at the hardware's unsigned dimension limit.
    info = ProgramInfo(global_size=items)

    # When missing local sizes are selected.
    result = offline_schedule.launch_dimensions(info, SOURCE, FIXTURE.read_bytes())

    # Then division remains exact and each hardware dimension is preserved.
    assert result == (groups, local)
    assert tuple(count * size for count, size in zip(*result, strict=True)) == items


@pytest.mark.parametrize("items", [(0, 1, 1), (-1, 1, 1), (2**32, 1, 1), (True, 1, 1), (1.0, 1, 1),
                                     (math.inf, 1, 1), (math.nan, 1, 1), (1, 1), (1, 1, 1, 1)])
def test_launch_rejects_invalid_item_dimensions_when_local_size_is_missing(items: tuple[int | float, ...]) -> None:
    # Given dimensions that cannot represent a static, bounded work-item domain.
    info = ProgramInfo(global_size=items)

    # When offline geometry is selected.
    with pytest.raises(ExportError, match="launch dimensions"):
        offline_schedule.launch_dimensions(info, SOURCE, FIXTURE.read_bytes())


@pytest.mark.parametrize("builtin", ["get_local_id", "get_group_id", "get_local_size", "get_num_groups", "barrier", "work_group_barrier"])
def test_launch_retains_single_thread_groups_when_source_depends_on_groups(builtin: str) -> None:
    # Given a kernel whose behavior depends on the original group boundaries.
    info = ProgramInfo(global_size=(128, 2, 1))
    source = SOURCE + f"\n{builtin} (0);"

    # When missing local sizes are selected.
    result = offline_schedule.launch_dimensions(info, source, FIXTURE.read_bytes())

    # Then each old work-item remains in its original single-thread group.
    assert result == ((128, 2, 1), (1, 1, 1))


@pytest.mark.parametrize("global_size,local_size", [((3.5, 2, 1), (16, 1, 1)), ((64, 2, 1), (1, 1, 1))])
def test_launch_preserves_geometry_when_local_size_is_explicit(
    global_size: tuple[int | float, ...], local_size: tuple[int, ...],
) -> None:
    # Given captured/generated group dimensions with an explicit local contract.
    info = ProgramInfo(global_size=global_size, local_size=local_size)

    # When the offline exporter asks for launch dimensions.
    result = offline_schedule.launch_dimensions(info, "barrier(0);", b"")

    # Then the complete explicit contract survives without reinterpretation.
    assert result == (global_size, local_size)


def test_launch_uses_supported_group_when_binary_has_maximum_valid_register_pressure() -> None:
    # Given the largest register counts accepted by the native A630 parser.
    binary = bytearray(FIXTURE.read_bytes())
    registers = struct.unpack_from("<I", binary, 0x34)[0]
    struct.pack_into("<II", binary, registers + 0x14, 63, 63)

    # When missing local sizes are selected for this resource contract.
    result = offline_schedule.launch_dimensions(ProgramInfo(global_size=(128, 1, 1)), SOURCE, bytes(binary))

    # Then the 64-thread group is below the independently known 128-thread limit.
    assert result == ((2, 1, 1), (64, 1, 1))


@pytest.mark.parametrize("registers", [(64, 0), (0, 64), (2**32 - 1, 0)])
def test_launch_rejects_unsupported_register_counts_when_binary_resource_contract_is_invalid(registers: tuple[int, int]) -> None:
    # Given resource counts the native A630 parser cannot accept.
    binary = bytearray(FIXTURE.read_bytes())
    offset = struct.unpack_from("<I", binary, 0x34)[0]
    struct.pack_into("<II", binary, offset + 0x14, *registers)

    # When the exporter considers regrouping this binary.
    with pytest.raises(ExportError, match="register"):
        offline_schedule.launch_dimensions(ProgramInfo(global_size=(128, 1, 1)), SOURCE, bytes(binary))


@pytest.mark.parametrize("binary", [b"", bytes(0x38), bytes(0x34) + struct.pack("<I", 2**32 - 1)])
def test_launch_rejects_missing_register_data_when_binary_is_truncated(binary: bytes) -> None:
    # Given truncated or out-of-bounds register metadata.
    info = ProgramInfo(global_size=(128, 1, 1))

    # When the exporter considers regrouping the program.
    with pytest.raises(ExportError, match="register"):
        offline_schedule.launch_dimensions(info, SOURCE, binary)
