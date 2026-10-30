import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

from cluster_h264_compare import wire


def digest(path):
  return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, environment, output, name, timeout=60):
  process = subprocess.run(command, env=environment, capture_output=True, text=True, timeout=timeout)
  (output / f'{name}.stdout').write_text(process.stdout)
  (output / f'{name}.stderr').write_text(process.stderr)
  invocation = {
    'command': command,
    'exit_code': process.returncode,
    'environment': {
      key: environment[key]
      for key in [
        'DISPLAY',
        'OPENPILOT_PREFIX',
        'LD_LIBRARY_PATH',
        'PARAMS_ROOT',
        'STARTUP_UI_RAYLIB_LIBRARY',
        'PYTHONPATH',
        'LD_PRELOAD',
        'ENCODER_FAKE_TRACE',
      ]
      if key in environment
    },
  }
  (output / f'{name}.invocation.json').write_text(json.dumps(invocation, indent=2) + '\n')
  if process.returncode:
    raise RuntimeError(f'{name} failed: {process.returncode}; inspect captured stdout/stderr')
  return process.stdout


def fnv(data):
  value = 14695981039346656037
  for byte in data:
    value = ((value ^ byte) * 1099511628211) & ((1 << 64) - 1)
  return value


def software_source(root, directory, request, binding, library, environment):
  output = directory / 'source'
  output.mkdir()
  params = output / 'params' / 'fixture'
  params.mkdir(parents=True)
  (params / 'UsbGpuActive').write_bytes(b'1' if request['fps'] == 5 else b'0')
  case = {
    'input': str(directory / 'native' / 'frames.rgba'),
    'root': str(params.parent),
    'lock': str(output / 'bus.lock'),
    'dump': str(output / 'dump.h264'),
    'width': request['width'],
    'height': request['height'],
    'fps': request['fps'],
    'gop': 10,
    'bitrate': '800k',
    'chunk_size': 4096,
    'wait_ack': request['wait_ack'],
    'soft_ack': request['soft_ack'],
    'encoder': 'libx264',
  }
  case_path = output / 'case.json'
  case_path.write_text(json.dumps(case))
  environment.update(
    PARAMS_ROOT=str(params.parent),
    OPENPILOT_PREFIX='fixture',
    USB_FIXTURE_BYTES=str(output / 'bytes.bin'),
    USB_FIXTURE_WIRE=str(output / 'wire.txt'),
    USB_FIXTURE_LOG=str(output / 'libusb.log'),
  )
  stdout = run(
    [sys.executable, str(root / 'rust/tools/cluster_h264_source.py'), str(root), str(binding), str(library), str(case_path)], environment, output, 'source'
  )
  return json.loads(stdout.splitlines()[-1]), output


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binary', type=Path, required=True)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--display', default=':88')
  parser.add_argument('--raylib', type=Path, required=True)
  args = parser.parse_args()
  root, output, binary = args.root.resolve(), args.output.resolve(), args.binary.resolve()
  estimated = 512 * 1024**2
  available = shutil.disk_usage(root).free
  if available < estimated:
    raise RuntimeError('insufficient space for estimated rendered artifact growth')
  output.mkdir(parents=True, exist_ok=False)
  library = root / '.omo/evidence/253-cluster-resume/libusb-1.0.so.0'
  kernel = root / '.omo/evidence/253-cluster-resume/vidc-encoder-source-controls/fake-encoder.so'
  receipts = []
  for mode, width, height, render_width, render_height, fps, soft in [
    ('software', 96, 64, 1920, 480, 10, False),
    ('software', 480, 1920, 1920, 480, 5, True),
    ('hardware', 96, 64, 1920, 480, 10, False),
    ('hardware', 480, 1920, 1920, 480, 5, True),
  ]:
    directory = output / f'{mode}-{width}x{height}-{fps}'
    native = directory / 'native'
    native.mkdir(parents=True)
    request = {
      'mode': mode,
      'output': str(native),
      'renderer_width': render_width,
      'renderer_height': render_height,
      'width': width,
      'height': height,
      'frames': 24,
      'fps': fps,
      'wait_ack': True,
      'soft_ack': soft,
    }
    request_path = directory / 'request.json'
    request_path.write_text(json.dumps(request))
    with tempfile.TemporaryDirectory(prefix='msgq_rust-probe-cluster-pipeline-', dir='/dev/shm') as ipc:
      environment = {
        **os.environ,
        'DISPLAY': args.display,
        'TZ': 'UTC',
        'OPENPILOT_PREFIX': Path(ipc).name.removeprefix('msgq_'),
        'PARAMS_ROOT': str(native / 'params'),
        'LD_LIBRARY_PATH': str(library.parent),
        'STARTUP_UI_RAYLIB_LIBRARY': str(args.raylib.resolve()),
        'USB_FIXTURE_MODE': 'ok',
        'USB_FIXTURE_BYTES': str(native / 'bytes.bin'),
        'USB_FIXTURE_WIRE': str(native / 'wire.txt'),
        'USB_FIXTURE_LOG': str(native / 'libusb.log'),
      }
      environment.pop('CEREAL_FAKE', None)
      if mode == 'hardware':
        environment.update(LD_PRELOAD=str(kernel), ENCODER_FAKE_CASE='normal', ENCODER_FAKE_TRACE=str(native / 'kernel.trace'))
      run([str(binary), str(root), str(request_path)], environment, native, 'native')
      receipt = json.loads((native / 'result.json').read_text())
      stream, commands, chunks = wire(native / 'bytes.bin', 4096)
      assert receipt['submitted'] == 24 and len(receipt['states']) == 24 and receipt['error'] is None
      assert all(row['onroad'] and abs(row['speed_kph'] - (5 + row['frame']) * 3.6) < 1e-5 for row in receipt['states'])
      assert stream == (native / 'dump.h264').read_bytes()
      assert len(stream) == receipt['bytes_sent'] and chunks == receipt['chunks_sent']
      assert (native / 'params/fixture/ClusterHudConnected').read_bytes() == b'0'
      assert (native / 'libusb.log').read_text().endswith('exit clean\n')
      result = {
        'mode': mode,
        'dimensions': [width, height],
        'fps': fps,
        'input_states': len(receipt['states']),
        'sent_bytes': len(stream),
        'chunks': chunks,
        'stream_sha256': hashlib.sha256(stream).hexdigest(),
      }
      if mode == 'software':
        assert receipt['reaped']
        source, source_output = software_source(root, directory, request, args.binding.resolve(), library, dict(environment))
        source_stream, source_commands, _ = wire(source_output / 'bytes.bin', 4096)
        assert source['reaped'] and source['error'] is None and source['submitted'] == 24
        byte_exact = source_stream == stream
        if not byte_exact:
          proof = root / '.omo/evidence/253-cluster-resume/pipeline-batch-controls/result.json'
          controls = json.loads(proof.read_text())['runs']
          assert width == 480 and height == 1920
          assert controls[1]['sha256'] != controls[2]['sha256']
          for name in ['source-repeat-1', 'source-repeat-2']:
            case_path = proof.parent / name / 'case.json'
            case = json.loads(case_path.read_text())
            assert digest(Path(case['input'])) == digest(native / 'frames.rgba')
            assert [case['width'], case['height'], case['fps']] == [width, height, fps]
        assert source_commands == commands
        probe = json.loads(
          run(
            [
              '/usr/bin/ffprobe',
              '-v',
              'error',
              '-count_frames',
              '-show_entries',
              'stream=width,height,nb_read_frames',
              '-of',
              'json',
              str(native / 'dump.h264'),
            ],
            environment,
            native,
            'probe',
          )
        )['streams'][0]
        assert [probe['width'], probe['height']] == [width, height]
        assert int(probe['nb_read_frames']) == 23
        run(
          ['/usr/bin/ffmpeg', '-v', 'error', '-i', str(native / 'dump.h264'), '-frames:v', '1', str(native / 'decoded-first.png')],
          environment,
          native,
          'decode',
        )
        source_probe = json.loads(
          run(
            [
              '/usr/bin/ffprobe',
              '-v',
              'error',
              '-count_frames',
              '-show_entries',
              'stream=width,height,nb_read_frames',
              '-of',
              'json',
              str(source_output / 'dump.h264'),
            ],
            environment,
            source_output,
            'probe',
          )
        )['streams'][0]
        assert source_probe == probe
        assert source['command'] == receipt['command']
        result.update(
          source_byte_exact=byte_exact,
          decoded_frames=23,
          child_reaped=True,
          encoder_arguments_exact=True,
          source_stream_sha256=hashlib.sha256(source_stream).hexdigest(),
        )
        if not byte_exact:
          result['source_repeat_nondeterminism_proof'] = str(proof)
      else:
        assert receipt['cancelled'] == 'owned render cancellation'
        assert receipt['dropped_units'] == 0 and receipt['dropped_chunks'] == 0
        queue = [line.split() for line in (native / 'kernel.trace').read_text().splitlines() if line.startswith('QUEUE_INPUT ')]
        assert len(queue) == 24
        hashes = [fnv((native / f'frame-{frame}.nv12').read_bytes()) for frame in range(24)]
        assert [int(row[-1]) for row in queue] == hashes
        assert [int(row[3]) * 1_000_000 + int(row[4]) for row in queue] == [frame * 1_000_000 // fps for frame in range(24)]
        result.update(queued_gpu_bytes_exact=True, render_cancellation=True, actual_hardware_encoded_pixels=False, input_hashes=hashes)
      (directory / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
      receipts.append(result)
  result = {
    'available_bytes': available,
    'estimated_growth_bytes': estimated,
    'binary_sha256': digest(binary),
    'usb_provider_sha256': digest(library),
    'kernel_provider_sha256': digest(kernel),
    'raylib_provider_sha256': digest(args.raylib.resolve()),
    'scenarios': receipts,
    'scope': 'Owned Cereal, full renderer, GPU packing, USB controller and FFmpeg child; VIDC uses a fake kernel without physical encoded-pixel validation.',
  }
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(
    json.dumps(
      {
        'scenarios': len(receipts),
        'frames': sum(row['input_states'] for row in receipts),
        'source_exact_software_streams': sum(row.get('source_byte_exact', False) for row in receipts),
      }
    )
  )


if __name__ == '__main__':
  main()
