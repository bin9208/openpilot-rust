"""Compare original debug methods against the actual native runtime packet boundaries."""

import argparse
import ast
from contextlib import redirect_stdout
import hashlib
import io
import json
from pathlib import Path
import re
from types import SimpleNamespace

from cluster_sps_compare import original


def main():
  cli = argparse.ArgumentParser()
  cli.add_argument('root', type=Path)
  cli.add_argument('evidence', type=Path)
  args = cli.parse_args()
  path = args.root / 'openpilot/selfdrive/carrot/cluster/cluster_h264_pipeline.py'
  scope = original(args.root)
  nodes = []
  methods = []
  for node in ast.parse(path.read_text()).body:
    if isinstance(node, ast.Assign) and any(
      isinstance(target, ast.Name) and target.id.startswith(('H264_DEBUG_', 'H264_NAL_NAMES', 'V4L2_')) for target in node.targets
    ):
      nodes.append(node)
    if isinstance(node, ast.FunctionDef) and node.name == '_bytes_head':
      nodes.append(node)
    if isinstance(node, ast.ClassDef) and node.name == 'H264UsbPipeline':
      methods = [
        method for method in node.body if isinstance(method, ast.FunctionDef) and (method.name.startswith('_debug_') or method.name == '_send_h264_chunk')
      ]
  nodes.append(ast.ClassDef(name='DebugControl', bases=[], keywords=[], body=methods, decorator_list=[]))
  scope['time'] = SimpleNamespace(perf_counter=lambda: 1.0)
  exec(compile(ast.fix_missing_locations(ast.Module(body=nodes, type_ignores=[])), str(path), 'exec'), scope)
  results = []
  for mode, name in [('ffmpeg', 'software'), ('native', 'hardware')]:
    case = args.evidence / name
    text = (case / 'stdout').read_text()
    lines = [
      line
      for line in text.splitlines()
      if line.startswith(('H264 stdout read ', 'H264 encoder packet ', 'H264 packetize ', 'H264 chunk ', 'H264 debug summary:'))
    ]
    packet_sizes = [int(match[1]) for match in re.finditer(r'H264 packetize \d+: .*? input=(\d+)', text)]
    chunk_limit = int(re.search(r'chunk_limit=(\d+)', text)[1])
    data = (case / 'dump.h264').read_bytes()
    owner = scope['DebugControl']()
    for key in [
      'encoder_packets',
      'encoder_bytes',
      'stdout_reads',
      'stdout_bytes',
      'packetize_events',
      'usb_bytes',
      'max_packet_bytes',
      'max_chunk_bytes',
      'dropped_access_units',
      'dropped_chunks',
    ]:
      setattr(owner, '_debug_' + key, 0)
    owner.debug = True
    owner.backend_name = mode
    owner.wait_for_ack = False
    owner.soft_ack = False
    owner._chunks_sent = 0
    owner._add_sample_value = lambda *values: None
    owner._record_h264_send = lambda *values: None
    owner.usb_display = SimpleNamespace(send_h264_chunk=lambda *values, **options: None)
    output = io.StringIO()
    offset = 0
    with redirect_stdout(output):
      for index, length in enumerate(packet_sizes, 1):
        packet = data[offset : offset + length]
        offset += length
        owner._debug_max_packet_bytes = max(owner._debug_max_packet_bytes, len(packet))
        if mode == 'ffmpeg':
          owner._debug_stdout_reads += 1
          owner._debug_stdout_bytes += len(packet)
          owner._debug_log_stdout_read(index, mode, packet)
        else:
          owner._debug_encoder_packets += 1
          owner._debug_encoder_bytes += len(packet)
          match = re.search(rf'H264 encoder packet {index}: native/raw .*?flags=0x([0-9A-F]+).*?ts=(\d+) codec_config=(\d) keyframe=(\d)', text)
          values = {'flags': int(match[1], 16), 'timestamp_us': int(match[2]), 'codec_config': bool(int(match[3])), 'keyframe': bool(int(match[4]))}
          owner._debug_log_encoder_packet(index, mode, 'raw', packet, **values)
          owner._debug_log_encoder_packet(index, mode, 'patched', packet, **values)
        chunks = [packet[start : start + chunk_limit] for start in range(0, len(packet), chunk_limit)]
        owner._debug_log_packetize(mode, index, packet, chunks, chunk_limit)
        for chunk in chunks:
          owner._send_h264_chunk(chunk, chunk_limit, source=mode)
      elapsed = float(re.search(r'H264 debug summary: .*? elapsed=([0-9.]+)s', text)[1])
      owner._debug_started_at = 1.0 - elapsed
      owner._debug_log_close_summary()
    expected = output.getvalue().splitlines()
    # Native sender and encoder logs may interleave; each line retains its Source counters.
    assert sorted(expected) == sorted(lines), (name, set(expected) - set(lines), set(lines) - set(expected))
    assert offset == len(data)
    (case / 'source-debug.txt').write_text(output.getvalue())
    results.append({'name': name, 'exact': True, 'packets': len(packet_sizes), 'debug_lines': len(lines), 'stream_bytes': len(data)})
  (args.evidence / 'debug-result.json').write_text(
    json.dumps(
      {
        'source_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
        'scenarios': results,
        'scope': 'Original methods with actual native packet boundaries; elapsed normalization only. Native fake-kernel packets have no NAL/SPS mutation.',
      },
      indent=2,
    )
    + '\n'
  )
  print(results)


if __name__ == '__main__':
  main()
