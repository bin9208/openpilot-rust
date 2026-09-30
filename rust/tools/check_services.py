import argparse
from pathlib import Path

from openpilot.cereal.services import SERVICE_LIST
from openpilot.cereal import log


def generated():
    lines = ["pub const SERVICES: &[Service] = &["]
    for name, value in SERVICE_LIST.items():
        decimation = "None" if value.decimation is None else f"Some({int(value.decimation)})"
        frequency_range = "None" if value.frequency_range is None else f"Some(({value.frequency_range[0]!r}, {value.frequency_range[1]!r}))"
        lines.extend(["    Service {", f'        name: "{name}",', f"        should_log: {str(value.should_log).lower()},",
                      f"        frequency: {value.frequency!r},", f"        decimation: {decimation},", f"        queue_size: {int(value.queue_size)},",
                      f"        frequency_range: {frequency_range},", "    },"])
    return "\n".join([*lines, "];", ""])


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--emit-rust", action="store_true")
    args = parser.parse_args()
    source = generated()
    if args.emit_rust:
        print(source, end="")
    else:
        path = Path(__file__).resolve().parents[1] / "crates/messaging/src/services_generated.rs"
        assert path.read_text() == source, "Rust service catalog differs from source; regenerate with --emit-rust"
        assert [name for name in SERVICE_LIST if name not in log.Event.schema.fields] == [
            "navModel", "customReservedRawData1", "customReservedRawData2",
        ], "source schema availability changed; update the explicit Rust default-payload parity cases"
        print(f"{len(SERVICE_LIST)} service definitions match original source")


if __name__ == "__main__":
    main()
