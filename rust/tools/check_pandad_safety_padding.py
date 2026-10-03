import argparse
import json
from pathlib import Path

from check_pandad_safety import invoke, operation


def check(source_command, native_command, output):
  output.mkdir(parents=True, exist_ok=False)
  wire = invoke(source_command, [{'op': 'car_params', 'alternative': 32767, 'configs': [[3, 65535], [17, 65435]]}], output, 'car-params')[0]
  rows = []
  for truncated in (False, True):
    for fill in (0x11, 0x22):
      step = operation(CarParams=wire[:-8] if truncated else wire, FirmwareQueryDone=b'1', ControlsReady=b'1')
      step['allocation_fill'] = fill
      rows.append({'op': 'safety', 'label': f'{truncated}-{fill}', 'pandas': 2, 'operations': [step]})
  source = invoke(source_command, [row | {'params_root': str(output.resolve() / 'source-params')} for row in rows], output, 'source')
  native = invoke(native_command, [row | {'params_root': str(output.resolve() / 'native-params')} for row in rows], output, 'native')
  for index in (0, 1):
    expected = source[index]['results'][0].copy()
    assert expected.pop('filled_allocations') >= 1
    assert expected == native[index]['results'][0]
  assert source[0]['results'][0]['commands'] == source[1]['results'][0]['commands']
  final_commands = []
  for index, fill in ((2, 0x11), (3, 0x22)):
    actual = source[index]['results'][0]
    assert actual['filled_allocations'] >= 1 and not actual['failed'] and actual['state']['safety_configured'], actual
    final = [command for command in actual['commands'] if command['panda'] == 1 and command['request'] == 0xdc][-1]
    assert final['value'] == 17 and final['index'] == fill * 257, (index, final)
    final_commands.append(final)
    rejected = native[index]['results'][0]
    assert rejected['failed'] and not rejected['state']['safety_configured'], rejected
    assert rejected['commands'] == [{'panda': panda, 'request': 0xdc, 'value': 3, 'index': 1} for panda in (0, 1)], rejected
  assert final_commands[0] != final_commands[1]
  report = {'result': 'pass', 'full_message_bytes': len(wire), 'truncated_message_bytes': len(wire) - 8,
            'valid_input_exact_with_both_allocator_fills': True, 'source_truncated_final_commands': final_commands,
            'native_rejected_both_truncated_inputs': True,
            'scope': 'controlled original AlignedBuffer missing-word read; deliberate native rejection, not malformed-input parity or device evidence'}
  (output / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
  return report


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source', type=Path, required=True)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args()
  print(json.dumps(check([args.source.resolve()], [args.binary.resolve()], args.output)))


if __name__ == '__main__':
  main()
