#!/usr/bin/env python3
"""Compare Rust Params reads with the actual inline C++ Params::getFloat source."""
import argparse
import json
import math
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]


def check(binary: Path, output: Path) -> None:
    output.mkdir(parents=True, exist_ok=False)
    source = (ROOT / "openpilot/common/params.h").read_text()
    start = source.index("  inline float getFloat(")
    end = source.index("\n  }", start) + len("\n  }")
    method = source[start:end]
    generated = output / "source_get_float.cc"
    prefix = """#include <string>
#include <iostream>
#include <iterator>
#include <iomanip>
struct Source {
  std::string value;
  std::string get(const std::string&, bool) { return value; }
"""
    suffix = """
};
int main() {
  Source params;
  params.value = std::string(std::istreambuf_iterator<char>(std::cin), {});
  try { std::cout << std::setprecision(17) << double(params.getFloat("CameraYawTrimDeg")); }
  catch(const std::exception&) { std::cout << "ERROR"; }
}
"""
    generated.write_text(prefix + method + suffix)
    reference = output / "source-get-float"
    subprocess.run(["g++", "-std=c++17", "-O2", generated, "-o", reference], check=True)
    inputs = [b"", b"0", b"-0", b"-0.0", b"1", b"  -12.3456789tail", b"0.0001", b"0.000100000001", b"0.00010000000474974513", b"0x1.8p+2",
              b"nan", b"NAN(payload)", b"+INF", b"-infinity", b"1\x00garbage", b"\x00", b"garbage", b" ",
              b"1e99", b"1e-99", b"1e-45", b"1.17549435e-38", b"3.40282346e+38", b"3.4028236e+38", b"1e"]
    results = []
    for value in inputs:
        expected = subprocess.check_output([reference], input=value).decode().strip()
        observed = subprocess.check_output([binary], input=value).decode().strip()
        if expected == "ERROR":
            assert observed == "ERROR", (value, observed)
        else:
            expected_value, observed_value = float(expected), float(observed)
            assert observed_value == expected_value or math.isnan(expected_value) and math.isnan(observed_value), (value, observed, expected)
            if expected_value == 0:
                assert math.copysign(1., observed_value) == math.copysign(1., expected_value)
        results.append({"input_hex": value.hex(), "source": expected, "rust": observed})
    (output / "report.json").write_text(json.dumps({"result": "pass", "cases": results}, indent=2) + "\n")
    print(f"PASS: {len(results)} original getFloat cases, exact Float32 promotion and exceptions")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    check(args.binary.resolve(), args.output.resolve())
