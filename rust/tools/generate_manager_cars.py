"""Build-time export of supported-car docs for the native manager installation."""
import argparse
import importlib
import json
from pathlib import Path


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('output', type=Path)
  parser.add_argument('--binding', type=Path)
  parser.add_argument('--check', action='store_true')
  args = parser.parse_args()
  if args.binding:
    from original_params_binding import load
    load(args.binding, 'inproc://manager-cars-build', args.output / 'unused-logs')
  args.output.mkdir(parents=True, exist_ok=True)
  for brand in ('hyundai', 'gm', 'toyota', 'mazda', 'ford', 'volkswagen', 'tesla'):
    values = importlib.import_module(f'opendbc.car.{brand}.values')
    names = [doc.name for platform in values.CAR for doc in platform.config.car_docs]
    text = json.dumps(names, ensure_ascii=False) + '\n'
    target = args.output / f'{brand}.json'
    if args.check:
      assert target.read_text() == text, f'{brand} source names changed'
    else:
      target.write_text(text)
    print(f'{brand}: {len(names)} source car documentation names')


if __name__ == '__main__':
  main()
