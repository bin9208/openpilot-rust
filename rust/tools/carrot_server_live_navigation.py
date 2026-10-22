"""Issue #254: allow only the explicitly checked navigation snapshot difference."""
from __future__ import annotations

import copy

from carrot_server_live_wire import normalize


def assert_navigation_response(source, native, navigation) -> None:
    # Assert the inherited defect rather than silently normalizing arbitrary fields.
    assert source['services']['navInstructionCarrot'] == {
        'mainText': None, 'distanceText': None, 'turnType': None,
    }
    assert native['services']['navInstructionCarrot'] == navigation
    expected = copy.deepcopy(source)
    expected['services']['navInstructionCarrot'] = navigation
    assert normalize(expected) == normalize(native)
