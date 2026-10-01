from pathlib import Path
import argparse
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def build(output: Path, zmq_root: Path) -> None:
    include = output / "include/cereal"
    include.mkdir(parents=True, exist_ok=True)
    with (include / "services.h").open("wb") as stream:
        subprocess.run(["python3", str(ROOT / "openpilot/cereal/services.py")], stdout=stream, check=True)
    files = [ROOT / "openpilot/cereal/messaging" / name for name in ("bridge.cc", "bridge_zmq.cc", "msgq_to_zmq.cc")]
    files += [ROOT / "msgq_repo/msgq" / name for name in ("ipc.cc", "event.cc", "impl_msgq.cc", "impl_fake.cc", "msgq.cc")]
    subprocess.run(["g++", "-std=c++17", "-pthread", "-O2", "-I", str(output / "include"),
                    "-I", str(ROOT / "openpilot"), "-I", str(ROOT / "msgq_repo"),
                    "-I", str(zmq_root / "source/include"), *map(str, files), str(zmq_root / "lib/libzmq.a"),
                    "-o", str(output / "bridge-original")], check=True, timeout=120)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--zmq-root", type=Path)
    parser.add_argument("--target", type=Path, default=ROOT / "rust/target")
    args = parser.parse_args()
    root = args.zmq_root
    if root is None:
        root = next((path for path in sorted((args.target / "debug/build").glob("zmq-sys-*/out"))
                     if (path / "lib/libzmq.a").is_file() and (path / "source/include/zmq.h").is_file()), None)
    if root is None:
        parser.error("build openpilot-bridge first or provide --zmq-root containing lib/libzmq.a and source/include/zmq.h")
    build(args.output.resolve(), root.resolve())
