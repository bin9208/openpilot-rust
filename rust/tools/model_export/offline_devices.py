from __future__ import annotations

from collections.abc import Callable, Generator
from contextlib import contextmanager
from functools import wraps
from typing import ParamSpec, TypeVar

from tinygrad.device import Compiled, Device

from .schema import ExportError

P = ParamSpec("P")
R = TypeVar("R")


@contextmanager
def host_only_devices() -> Generator[None, None, None]:
    attribute = "_Device__get_canonicalized_item"
    was_overridden = attribute in Device.__dict__
    original = Device._Device__get_canonicalized_item

    def open_host(name: str) -> Compiled:
        if name.split(":", maxsplit=1)[0] not in {"CPU", "NPY", "PYTHON", "DISK"}:
            raise ExportError(f"offline export forbids runtime device initialization: {name}")
        return original(name)

    Device._Device__get_canonicalized_item = open_host
    try:
        yield
    finally:
        if was_overridden:
            Device._Device__get_canonicalized_item = original
        else:
            delattr(Device, attribute)


def host_build(function: Callable[P, R]) -> Callable[P, R]:
    @wraps(function)
    def guarded(*args: P.args, **kwargs: P.kwargs) -> R:
        with host_only_devices():
            return function(*args, **kwargs)
    return guarded
