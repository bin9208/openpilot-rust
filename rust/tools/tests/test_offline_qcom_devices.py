from __future__ import annotations

import pytest
from tinygrad.device import Device

from model_export import ExportError
from model_export.offline_devices import host_only_devices


def test_offline_guard_rejects_gpu_when_original_device_lookup_is_requested() -> None:
    # Given the original runtime's device-opening boundary.
    lookup = Device._Device__get_canonicalized_item

    # When offline capture accidentally requests a GPU device.
    with host_only_devices(), pytest.raises(ExportError, match="device"):
        Device["QCOM"]

    # Then no GPU opens and the original lookup implementation is restored.
    assert "QCOM" not in Device._opened_devices
    assert Device._Device__get_canonicalized_item == lookup
