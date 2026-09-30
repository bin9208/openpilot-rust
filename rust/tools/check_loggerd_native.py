#!/usr/bin/env python3
"""Compare continuous Rust and original loggerd using original encoder messages."""
from __future__ import annotations

import argparse
import json
from pathlib import Path

from loggerd_fixtures import capture
from loggerd_edges import fallbacks, run_edges
from loggerd_scenarios import run
from loggerd_validation import compare
import loggerd_hevc
import loggerd_diagnostics
from check_loggerd_peer import check as check_input_completion


def main() -> None:
  parser = argparse.ArgumentParser()
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--original', type=Path, required=True)
  parser.add_argument('--producer', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  parser.add_argument('--fixtures', type=Path)
  parser.add_argument('--scenario', action='append', choices=['ordinary', 'burst', 'fairness', 'video', 'audio', 'audio-queue', 'queue-restart'])
  parser.add_argument('--edges', action='store_true')
  parser.add_argument('--fallbacks', action='store_true')
  parser.add_argument('--hevc', action='store_true')
  parser.add_argument('--runner', type=Path)
  arguments = parser.parse_args()
  output = arguments.output.resolve()
  output.mkdir(parents=True, exist_ok=True)
  check_input_completion(output / 'input-completion')
  if arguments.fixtures:
    fixtures = {kind: [path.read_bytes() for path in sorted((arguments.fixtures / kind).glob('*.capnp'))] for kind in ('road', 'qroad')}
  else:
    fixtures = {kind: capture(arguments.producer.resolve(), output / 'fixtures' / kind, kind) for kind in ('road', 'qroad')}
  assert all(fixtures.values())
  results = {}
  runner = () if arguments.runner is None else (str(arguments.runner.resolve()),)
  if runner and (arguments.edges or arguments.fallbacks):
    raise ValueError('edge/fallback runner scenarios must be invoked separately')
  for scenario in arguments.scenario or ['ordinary', 'burst', 'fairness', 'video', 'audio', 'audio-queue', 'queue-restart']:
    source = run(arguments.original.resolve(), output / (scenario + '-original'), scenario, fixtures)
    candidate = run(arguments.binary.resolve(), output / (scenario + '-rust'), scenario, fixtures, runner)
    results[scenario] = compare(source, candidate)
    print(scenario, 'passed', flush=True)
    (output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
  if arguments.edges:
    results['edges'] = run_edges(arguments.original.resolve(), arguments.binary.resolve(), output / 'edges', fixtures)
    print('edges passed', flush=True)
  if arguments.hevc:
    sample = loggerd_hevc.fixture(output / 'hevc-fixture')
    results['hevc'] = compare(loggerd_hevc.run(arguments.original.resolve(), output / 'hevc-original', sample),
                              loggerd_hevc.run(arguments.binary.resolve(), output / 'hevc-rust', sample, runner))
    print('hevc passed', flush=True)
  if arguments.fallbacks:
    results['fallbacks'] = fallbacks(arguments.original.resolve(), arguments.binary.resolve(), output / 'fallbacks', fixtures['road'][0])
    print('fallbacks passed', flush=True)
  (output / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
  loggerd_diagnostics.compare_tree(output)


if __name__ == '__main__':
  main()
