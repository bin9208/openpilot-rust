"""Actual final display_tracks evidence mapping, including mixed labels and caps."""
import argparse
import json
from pathlib import Path
from signal_source import classes


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  args.output.mkdir(parents=True, exist_ok=False)
  display = classes('openpilot/selfdrive/modeld/signal_neural_shadow.py', ['display_tracks'])['display_tracks']
  labels = ('green', 'left,red', 'red', 'yellow', 'unknown', 'green,red', 'green,yellow', '', 'red,up_left')
  scores = (0., .499999999, .5, .8, .9, 1., 1.01, -1.)
  cases, expected = [], []
  for index in range(128):
    tracks = [dict(id=j, box=[600., 200., 620., 240.], label=labels[(index+j) % len(labels)], score=scores[(index+j) % len(scores)], source=('tracked', 'detected')[j % 2], neural_age=(j % 4)*.2) for j in range(index % 32)]
    cases.append(tracks)
    expected.append(display(tracks))
  (args.output / 'input.json').write_text(json.dumps(cases))
  (args.output / 'expected.json').write_text(json.dumps(expected))
  print(json.dumps(dict(cases=len(cases), outputs=sum(map(len, expected)), native_compared=False)))


if __name__ == '__main__':
  main()
