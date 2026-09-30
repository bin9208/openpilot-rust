"""Focused original/native board-control comparison; all effects stay in fixtures."""

import argparse
import json
from pathlib import Path
import subprocess

from hardware_control_source import run


def cases():
  actions = [
    'msm_vidc',
    'i2c_geni',
    'fts_ts',
    'msm_drm',
    'spi_geni',
    'kgsl-3d0',
    'a5',
    'cci',
    'cpas_camnoc',
    'cpas-cdm',
    'csid',
    'ife',
    'csid-lite',
    'ife-lite',
  ]
  files = {'/proc/interrupts': ' CPU0 CPU1\n' + ''.join(f' {index}: 0 0 device\n' for index in range(20, 34))}
  files.update({f'/sys/kernel/irq/{index}/actions': action + '\n' for index, action in enumerate(actions, 20)})
  files['/sys/class/backlight/panel0-backlight/max_brightness'] = '4095\n'
  ordinary = [
    {'kind': 'initialize'},
    {'kind': 'power_save', 'enabled': True},
    {'kind': 'power_save', 'enabled': False},
    {'kind': 'display', 'on': False},
    {'kind': 'display', 'on': True},
    {'kind': 'brightness', 'percent': 43.7},
    {'kind': 'ir', 'percent': 37},
    {'kind': 'reset'},
    {'kind': 'recover'},
    {'kind': 'has_panda'},
    {'kind': 'booted'},
    {'kind': 'reboot'},
    {'kind': 'shutdown'},
    {'kind': 'uninstall'},
  ]

  def case(operations, **kwargs):
    return {'model': 'tici', 'files': files, 'command_text': '123\n', 'operations': operations} | kwargs

  for model in ['pc', 'tici', 'tizi', 'mici']:
    yield model, case(ordinary, model=model)
  yield 'lite-cached-exclusion', case(ordinary, lite=True)
  yield 'amp-false-continues', case(ordinary[:3], amplifier_result=False)
  yield 'amp-error-stops', case(ordinary[:1], faults={'1': 5})
  yield 'power-amp-error-stops', case([{'kind': 'power_save', 'enabled': False}], faults={'1': 5})
  yield 'cpu-write-error-stops', case([{'kind': 'power_save', 'enabled': True}], lite=True, faults={'1': 5})
  for status in [1, -15]:
    yield f'pgrep-status-{status}', case(ordinary[:1], command_status=status)
  yield 'pgrep-spawn-error', case(ordinary[:1], faults={str(len(run(case(ordinary[:1]))['events']) - 3): 2})
  for now in [119.9, 120.0]:
    yield f'booted-{now}', case([{'kind': 'booted'}], command_text='Core state: 0', now=now)
  yield 'booted-command-fails', case([{'kind': 'booted'}], command_status=1)
  yield 'display-error-swallowed', case([{'kind': 'display', 'on': True}], faults={'0': 5})
  yield 'brightness-read-error-swallowed', case([{'kind': 'brightness', 'percent': 25}], faults={'0': 5})
  yield 'brightness-write-error-swallowed', case([{'kind': 'brightness', 'percent': 25}], faults={'1': 5})
  for value in ['bad', 'NaN', 'inf']:
    yield 'brightness-' + value, case([{'kind': 'brightness', 'percent': 25}], files=files | {'/sys/class/backlight/panel0-backlight/max_brightness': value})
  yield 'brightness-unclamped', case([{'kind': 'brightness', 'percent': -0.01}, {'kind': 'brightness', 'percent': 110}])
  yield 'ir-middle-error-stops', case([{'kind': 'ir', 'percent': 43}], faults={'1': 5})
  yield 'gpio-error-continues', case([{'kind': 'recover'}], faults={'0': 13, '3': 5})
  yield 'reboot-nonzero', case([{'kind': 'reboot'}], command_status=1)
  yield 'shutdown-nonzero-ignored', case([{'kind': 'shutdown'}], command_status=1)
  yield 'uninstall-touch-fails', case([{'kind': 'uninstall'}], faults={'0': 13})
  yield 'uninstall-sync-fails', case([{'kind': 'uninstall'}], faults={'1': 5})
  for faults in [{'0': 13}, {'0': 13, '2': 13}, {'0': 13, '2': 5}, {'0': 2}]:
    yield 'sudo-' + '-'.join(f'{key}-{value}' for key, value in faults.items()), case([{'kind': 'sudo_write', 'path': '/fixture', 'value': '1'}], faults=faults)
  yield 'irq-read-fails', case([{'kind': 'affine', 'core': 6, 'action': 'a5'}], faults={'1': 13})
  yield 'irq-missing-cached', case([{'kind': 'affine', 'core': 6, 'action': 'a5'}] * 2, files={'/proc/interrupts': '22: x\nNMI: y\n'})
  yield (
    'irq-multiple-before-writes',
    case(
      [{'kind': 'affine', 'core': 6, 'action': 'a5'}],
      files={'/proc/interrupts': '22: x\n23: y\n', '/sys/kernel/irq/22/actions': 'a5,cci\n', '/sys/kernel/irq/23/actions': 'a5\n'},
    ),
  )


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('binary', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--runner', action='append', default=[])
  args = parser.parse_args()
  args.output.mkdir(parents=True)
  inputs = list(cases())
  command = [*args.runner, str(args.binary.resolve())]
  native = subprocess.run(command, input=''.join(json.dumps(config) + '\n' for _, config in inputs), capture_output=True, text=True, check=True)
  (args.output / 'native.stdout').write_text(native.stdout)
  (args.output / 'native.stderr').write_text(native.stderr or '<empty>\n')
  results = [json.loads(line) for line in native.stdout.splitlines()]
  assert len(results) == len(inputs)
  for (name, config), actual in zip(inputs, results, strict=True):
    expected = run(config)
    (args.output / (name + '.json')).write_text(json.dumps({'input': config, 'source': expected, 'native': actual}, indent=2))
    assert actual == expected, (name, expected, actual)
    print(name, 'PASS')
  (args.output / 'result.json').write_text(json.dumps({'result': 'PASS', 'cases': len(inputs), 'argv': command}, indent=2))


if __name__ == '__main__':
  main()
