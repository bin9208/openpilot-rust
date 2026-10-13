#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Caller provides the existing original-source/full-cereal/zstandard dependencies.
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import sys

import zstandard as zstd
from carrot_server_dashcam_catalog import source_modules
from carrot_server_dashcam_report_source_controls import stream
from carrot_server_dashcam_upload import save


def main() -> None:
    parser = argparse.ArgumentParser(); parser.add_argument('--output', type=Path, required=True)
    root = parser.parse_args().output.resolve(); root.mkdir(parents=True)
    catalog, paths, _ = source_modules()
    from openpilot.selfdrive.carrot.server.features.dashcam import report
    route = '2026-01-02--03-04-05'; directory = root/(route+'--0'); directory.mkdir()
    payload = stream(); compressor = zstd.ZstdCompressor(write_checksum=True).compressobj()
    completed = compressor.compress(payload) + compressor.flush(zstd.COMPRESSOBJ_FLUSH_BLOCK)
    tail = compressor.compress(b'owned unfinished block'*64) + compressor.flush(zstd.COMPRESSOBJ_FLUSH_BLOCK)
    data = completed + tail[:-5]; file = directory/'rlog.zst'; file.write_bytes(data)
    decoded = report._decompress(str(file))
    catalog.DASHCAM_ROOT = paths.DASHCAM_ROOT = report.DASHCAM_ROOT = str(root)
    result = report.build_route_report(route)
    assert decoded == payload and result['hasData'] is True
    save(root/'result.json', {'file': str(file), 'input_sha256': hashlib.sha256(data).hexdigest(),
        'decompress': {'ok': True, 'bytes': len(decoded), 'sha256': hashlib.sha256(decoded).hexdigest()},
        'report': result, 'shape': {'completed_block_bytes': len(completed), 'unfinished_tail_bytes': len(tail[:-5])}})
    save(root/'invocation.json', {'command': [sys.executable, '-P', *sys.argv],
        'source_sha256': hashlib.sha256(Path(report.__file__).read_bytes()).hexdigest()})
    print(json.dumps({'decoded_bytes': len(decoded), 'hasData': result['hasData']}))


if __name__ == '__main__':
    main()
