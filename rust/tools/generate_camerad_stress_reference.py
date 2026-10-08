import argparse
from pathlib import Path

from generate_camerad_kernel_reference import extract


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', type=Path, required=True)
  arguments = parser.parse_args()
  header = (arguments.source/'openpilot/system/camerad/cameras/spectra.h').read_text()
  method = extract(header, 'inline bool stress_test(')
  template = (arguments.source/'rust/tools/camerad_stress_source.cc.in').read_text()
  assert template.count('@METHOD@') == 1
  print(template.replace('@METHOD@', method))


if __name__ == '__main__':
  main()
