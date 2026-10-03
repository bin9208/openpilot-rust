# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy", "pycapnp"]
# ///
# Run with the existing oracle Python: PYTHONPATH=.:opendbc_repo:rust/tools python rust/tools/card_hyundai_source.py OUTPUT
from __future__ import annotations

import contextlib
import hashlib
import io
import json
from dataclasses import asdict, dataclass
from pathlib import Path
import sys
from typing import Final

from can_source import ROOT, load

VARIANTS: Final = (
    (0, 0, 0, False), (0, 1, 0, True), (1, 1, 1, False),
    (2, 0, -2, False), (3, 1, 1, True), (0, 0, -1, True),
)


class Settings:
    def __init__(self, camera: int = 0, hda2: int = 0, radar: int = 0) -> None:
        self.values = {"HyundaiCameraSCC": camera, "CanfdHDA2": hda2, "EnableRadarTracks": radar}

    def get_int(self, key: str) -> int:
        return self.values.get(key, 0)

    def get_bool(self, key: str) -> bool:
        return bool(self.values.get(key, 0))


@dataclass(frozen=True, slots=True)
class DetectCase:
    candidate: str
    fingerprints: list[tuple[int, list[tuple[int, int]]]]
    flags: int
    has_radar_dbc: bool
    camera_scc: int
    hda2: int
    radar_tracks: int
    alpha_long: bool
    result_flags: int
    ext_flags: int
    bsm: bool
    radar_unavailable: bool
    longitudinal: bool
    safety_model: int
    safety_param: int
    bus: tuple[int, int, int]
    diagnostics:list[str]


def detection_cases(target: Path) -> tuple[list[DetectCase], list[str]]:
    load()
    from opendbc.car import Bus
    from opendbc.car import structs
    from opendbc.car.hyundai import interface, hyundaicanfd
    from opendbc.car.hyundai.values import CAR
    from opendbc.car import interfaces
    from opendbc.car.interfaces import CarInterfaceBase

    results = []
    missing_tunes = []
    source_params = target / "source-params"
    source_params.mkdir(exist_ok=True)
    for candidate in CAR:
        try:
            CarInterfaceBase.get_std_params(candidate)
        except KeyError:
            missing_tunes.append(str(candidate))
            continue
        for offset in (0, 4):
            for variant_index, (camera, hda2, radar, alpha) in enumerate(VARIANTS):
                settings = Settings(camera, hda2, radar)
                interface.Params = hyundaicanfd.Params = interfaces.Params = lambda: settings
                fp = {i: {} for i in range(8)}
                ecan = offset + (1 if hda2 and camera == 0 else 0)
                acan = offset + (0 if hda2 and camera == 0 else 1)
                fp[ecan] = {a: 32 for a in (0x105, 0x1ba, 0xfa, 0x230, 0x1fa)}
                fp[acan] = {a: 32 for a in range(0x235, 0x249)}
                fp[acan].update({a: 32 for a in range(0x180, 0x185)})
                fp[acan].update({0x210: 32, 0x400: 32, 0x41d: 32, 0x110: 32})
                fp[offset + 2] = {0xcb: 24, 0x110: 32}
                fp[0].update({0x58b: 8, 0x485: 8, 0x38d: 8, 1348: 8, 1157: 8})
                fp[1][0x500] = 8
                gear_address = (0x40, 69, 112, 0x130, 0x999, 0x40)[variant_index]
                fp[ecan][gear_address] = 8
                if variant_index % 2:
                    fp[ecan][0x1cf] = 8
                    fp[offset + 2][0x2a4] = 32
                    fp[acan].pop(0x210)
                platform = candidate.config
                ret = CarInterfaceBase.get_std_params(candidate)
                ret.flags = int(platform.flags)
                ret.wheelbase = platform.specs.wheelbase
                with contextlib.redirect_stdout(io.StringIO()):
                    actual = interface.CarInterface._get_params(ret, candidate, fp, [], alpha, False, False)
                printed=io.StringIO()
                with contextlib.redirect_stdout(printed):
                    full = interface.CarInterface.get_params(candidate, fp, [], alpha, False, False)
                (source_params / f"{len(results)}.bin").write_bytes(full.to_bytes())
                can = hyundaicanfd.CanBus(None, fp, bool(hda2))
                results.append(DetectCase(str(candidate), list((b, list(f.items())) for b, f in fp.items()),
                    int(platform.flags), Bus.radar in platform.dbc_dict, camera, hda2, radar, alpha,
                    actual.flags, actual.extFlags, actual.enableBsm, actual.radarUnavailable,
                    actual.openpilotLongitudinalControl, actual.safetyConfigs[-1].safetyModel.raw,
                    actual.safetyConfigs[-1].safetyParam, (can.ECAN, can.ACAN, can.CAM),printed.getvalue().splitlines()))
    return results, missing_tunes


def main() -> None:
    if sys.argv[1] == "--baseline-inventory":
        load()
        from opendbc.car.interfaces import CarInterfaceBase
        from opendbc.car.values import PLATFORMS
        failures = []
        for candidate in PLATFORMS:
            try:
                CarInterfaceBase.get_std_params(candidate)
            except KeyError as error:
                failures.append(dict(candidate=str(candidate),error=str(error)))
        result = dict(identities=len(PLATFORMS),source_missing_torque=failures)
        Path(sys.argv[2]).write_text(json.dumps(result,indent=2) + "\n")
        print(json.dumps(result))
        return
    if sys.argv[1] == "--baseline-defect":
        load()
        from opendbc.car.interfaces import CarInterfaceBase
        CarInterfaceBase.get_std_params("KIA_K5_DL3_24_HEV")
        return
    target = Path(sys.argv[1])
    target.mkdir(parents=True, exist_ok=True)
    cases, missing_tunes = detection_cases(target)
    (target / "detection.json").write_text(json.dumps([asdict(case) for case in cases]) + "\n")
    from opendbc.car.hyundai.stopping import CanfdStopping
    scenarios = []
    for scenario in range(10):
        controller = CanfdStopping()
        steps = []
        for tick in range(500):
            speed = (0.3, max(0., 0.6 - tick * 0.008), 0.07, 4., 0.,
                0.02 if tick < 100 else 0.3, 0.3 if tick % 50 == 0 else 0.02,
                max(0., 0.8 - tick * 0.002), 0.12, 0.3)[scenario]
            input_values = dict(active=scenario != 9 or tick < 200,
                requested=scenario != 8 or tick < 300, speed=speed, held=scenario in (4, 5, 6),
                accel=-2. if scenario == 3 else -0.5, value=-0.3,
                previous_value=controller.last_value, jerk_u=2., jerk_l=1.)
            command = controller.update(**input_values)
            steps.append(dict(input=input_values, expected=dict(controller=vars(controller).copy(),
                command=asdict(command) if command is not None else None)))
        scenarios.append(steps)
    (target / "stopping.json").write_text(json.dumps(scenarios) + "\n")
    files = [ROOT / f"opendbc_repo/opendbc/car/hyundai/{name}.py" for name in (
        "values", "interface", "carstate", "carcontroller", "hyundaican", "hyundaicanfd", "stopping")]
    files.extend((ROOT / "opendbc_repo/opendbc/car/interfaces.py", ROOT / "opendbc_repo/opendbc/car/__init__.py"))
    (target / "provenance.json").write_text(json.dumps({
        "runtime_python": False, "source_sha256": {
            str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in files},
        "detection_cases": len(cases), "source_parameter_errors": missing_tunes}, indent=2) + "\n")
    print(f"Generated {len(cases)} exact-source Hyundai capability cases")


if __name__ == "__main__":
    main()
