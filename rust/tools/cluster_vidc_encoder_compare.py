import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess


def digest(path):
  return hashlib.sha256(path.read_bytes()).hexdigest()


def build(command, output):
  free, growth = shutil.disk_usage(output.parent).free, 128 * 1024**2
  assert free >= growth
  result = subprocess.run(command, capture_output=True, timeout=30, check=False)
  output.write_bytes(result.stdout + result.stderr)
  assert result.returncode == 0, str(output)
  return {'argv': command, 'available_bytes': free, 'estimated_growth_bytes': growth}


def provider(root):
  text = (root / 'rust/tools/encoder_oracle/fake_v4l.cc').read_text()

  def replace(old, new):
    nonlocal text
    assert text.count(old) == 1, old
    text = text.replace(old, new)

  replace('custom->cmd != ION_IOC_INV_CACHES', '(custom->cmd != ION_IOC_INV_CACHES && custom->cmd != ION_IOC_CLEAN_CACHES)')
  replace('flush->length != 4096', '!flush->length')
  replace('record("ION_SYNC"); return 0;', 'record("ION_SYNC %s %u",custom->cmd==ION_IOC_INV_CACHES?"from":"to",flush->length); return 0;')
  replace(
    '  if (request == VIDIOC_S_FMT) {',
    '''  if(request==VIDIOC_ENUM_FMT) {
    auto value=static_cast<v4l2_fmtdesc *>(argument);record("ENUM %u %u",value->type,value->index);
    if(test=="empty-formats" || value->index>0)return reject();
    value->pixelformat=test=="unsupported-format"?V4L2_PIX_FMT_YUYV:V4L2_PIX_FMT_NV12;return 0;
  }
  if (request == VIDIOC_S_FMT) {''',
  )
  replace(
    '    value->fmt.pix_mp.plane_fmt[0].sizeimage = 4096;',
    '''    value->fmt.pix_mp.plane_fmt[0].sizeimage = (test=="large-input" && value->type==V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)?524288:4096;
    value->fmt.pix_mp.plane_fmt[0].bytesperline=test=="wide-driver-stride"?4096:0;
    if(test=="zero-capture" && value->type==V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)value->fmt.pix_mp.plane_fmt[0].sizeimage=0;
    if(test=="wrong-input" && value->type==V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE)value->fmt.pix_mp.pixelformat=V4L2_PIX_FMT_YUYV;
    if(test=="wrong-codec" && value->type==V4L2_BUF_TYPE_VIDEO_CAPTURE_MPLANE)value->fmt.pix_mp.pixelformat=V4L2_PIX_FMT_HEVC;''',
  )
  replace(
    '    if ((test == "slice-size-failure"',
    '''    if(test=="required-control-failure" && value->id==V4L2_CID_MPEG_VIDEO_BITRATE)return reject();
    if(test=="baseline-fallback" && value->id==V4L2_CID_MPEG_VIDEO_H264_PROFILE
        && value->value==V4L2_MPEG_VIDEO_H264_PROFILE_CONSTRAINED_BASELINE)return reject();
    if ((test == "slice-size-failure"''',
  )
  replace('value->flags != V4L2_BUF_FLAG_TIMESTAMP_COPY', 'value->flags != (value->type==V4L2_BUF_TYPE_VIDEO_OUTPUT_MPLANE?V4L2_BUF_FLAG_TIMESTAMP_COPY:0)')
  replace(
    'record("QUEUE_INPUT %u %u %ld %ld", slot.index, slot.length, slot.time.tv_sec, slot.time.tv_usec);',
    '''uint64_t hash=14695981039346656037ULL;for(unsigned i=0;i<slot.length;++i)hash=(hash^reinterpret_cast<const uint8_t *>(slot.address)[i])*1099511628211ULL;
      record("QUEUE_INPUT %u %u %ld %ld %llu", slot.index, slot.length, slot.time.tv_sec, slot.time.tv_usec,static_cast<unsigned long long>(hash));''',
  )
  replace('      if (device.returned.empty()) abort();', '      if (device.returned.empty()) {errno=EAGAIN;return -1;}')
  replace(
    '    if (device.capture.empty()) abort();',
    '    if (device.capture.empty() || (!device.header && device.inputs.empty() && !(device.stopped && !device.eos))) {errno=EAGAIN;return -1;}',
  )
  replace(' || !device.inputs.empty() || !device.returned.empty()', '')
  return text


def prepare(root, output):
  paths = [
    'msgq/visionipc/visionbuf.h',
    'msgq/visionipc/visionbuf_ion.cc',
    'openpilot/system/camerad/cameras/nv12_info.h',
    'openpilot/system/loggerd/encoder/cluster_h264_encoder.h',
    'openpilot/system/loggerd/encoder/cluster_h264_encoder.cc',
  ]
  pieces = [
    '#include <iostream>\n#include <functional>\n#include <iterator>\n#include <algorithm>\n',
    '#include <array>\n#include <deque>\n#include <mutex>\n#include <chrono>',
    (root / 'rust/tools/encoder_oracle/v4l_adapter.h').read_text(),
  ]
  for path, marker in zip(
    paths, ['enum VisionStreamType', 'struct IonFileHandle', '#include <cassert>', 'constexpr int', '#define V4L2_QCOM_BUF_FLAG_CODECCONFIG'], strict=True
  ):
    content = (root / path).read_text()
    pieces.append(content[content.index(marker) :])
  pieces.append((root / 'rust/tools/decoder_oracle/encoder_main.cc').read_text())
  (output / 'source.cc').write_text('\n'.join(pieces))
  (output / 'fake-encoder.cc').write_text(provider(root))
  includes = ['-I' + str(root), '-I' + str(root / 'openpilot'), '-I' + str(root / 'third_party/linux/include')]
  builds = [
    build(['c++', '-std=c++20', '-O1', '-g0', '-pthread', *includes, str(output / 'source.cc'), '-o', str(output / 'source')], output / 'source-build.log'),
    build(
      [
        'c++',
        '-std=c++20',
        '-O1',
        '-g0',
        '-pthread',
        '-shared',
        '-fPIC',
        *includes,
        str(output / 'fake-encoder.cc'),
        '-ldl',
        '-o',
        str(output / 'fake-encoder.so'),
      ],
      output / 'provider-build.log',
    ),
  ]
  return {
    'source_hashes': {path: digest(root / path) for path in paths},
    'builds': builds,
    'provider_sha256': digest(output / 'fake-encoder.so'),
    'source_binary_sha256': digest(output / 'source'),
  }


def normalize(rows):
  values = []
  for row in rows:
    error = re.fullmatch(r'ERROR (?:VIDIOC_[^:]+: )?(.*?) \((?:os error )?(\d+)\)', row)
    values.append({'errno': int(error[2]), 'message': error[1]} if error else row)
  return values


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  root, output = args.root.resolve(), args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  preparation = prepare(root, output)
  (output / 'preparation.json').write_text(json.dumps(preparation, indent=2) + '\n')
  if args.binary is None:
    return
  cases = [
    ('normal', mode, width, height, rate, slice_bytes, priority)
    for mode in ['normal', 'active', 'acquire', 'dmabuf', 'cancel']
    for width, height in [(96, 64), (480, 1920)]
    for rate, slice_bytes, priority in [(2, 4096, 0), (4, 0, 1)]
  ]
  cases += [
    (scenario, 'normal', 96, 64, 2, 4096, 0)
    for scenario in [
      'empty-formats',
      'unsupported-format',
      'large-input',
      'wide-driver-stride',
      'zero-capture',
      'wrong-input',
      'wrong-codec',
      'wrong-output',
      'wrong-capability',
      'required-control-failure',
      'baseline-fallback',
      'slice-size-failure',
      'slice-mode-failure',
      'compat-failure',
      'poll-eintr',
      'hold-seven',
      'input-queue-failure',
      'wrong-offset',
      'wrong-timestamp',
    ]
  ]
  cases += [('normal', 'normal', 96, 64, rate, 4096, 0) for rate in range(8)]
  cases += [('normal', 'short', 96, 64, 2, 4096, 0)]
  results = []
  for index, (scenario, *arguments) in enumerate(cases):
    values = {}
    for kind, binary in [('source', output / 'source'), ('native', args.binary.resolve())]:
      name = f'{index:03}-{scenario}-{kind}'
      command = [str(binary), *map(str, arguments)]
      env = {**os.environ, 'LD_PRELOAD': str(output / 'fake-encoder.so'), 'ENCODER_FAKE_CASE': scenario, 'ENCODER_FAKE_TRACE': str(output / (name + '.trace'))}
      result = subprocess.run(command, env=env, capture_output=True, timeout=8, check=False)
      (output / (name + '.stdout')).write_bytes(result.stdout)
      (output / (name + '.stderr')).write_bytes(result.stderr)
      assert result.returncode == 0, (name, result.returncode)
      trace = output / (name + '.trace')
      values[kind] = {
        'argv': command,
        'output': result.stdout.decode().splitlines(),
        'trace': [row for row in trace.read_text().splitlines() if row not in ['OPEN ion', 'CLOSE ion']],
      }
    results.append(
      {
        'scenario': scenario,
        'arguments': arguments,
        'trace_match': values['source']['trace'] == values['native']['trace'],
        'behavior_match': normalize(values['source']['output']) == normalize(values['native']['output']),
        'literal_match': values['source']['output'] == values['native']['output'],
        **values,
      }
    )
  receipt = {
    **preparation,
    'binary_sha256': digest(args.binary),
    'cases': len(results),
    'matched': sum(case['trace_match'] and case['behavior_match'] for case in results),
    'literal_matches': sum(case['literal_match'] for case in results),
    'results': results,
  }
  (output / 'result.json').write_text(json.dumps(receipt, indent=2) + '\n')
  print(json.dumps({key: receipt[key] for key in ['cases', 'matched', 'literal_matches']}))
  raise SystemExit(receipt['matched'] != receipt['cases'])


if __name__ == '__main__':
  main()
