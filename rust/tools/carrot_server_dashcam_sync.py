#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = []
# ///
# Caller supplies the existing original-source dependencies; run from repository root.
# python -P rust/tools/carrot_server_dashcam_sync.py --binary PATH --worker PATH --output NEW_DIR
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path
import sys

import anyio
from carrot_server_dashcam_health import repository
from carrot_server_dashcam_sync_boundary import boundary
from carrot_server_dashcam_sync_concurrency import alongside_job, two_calls
from carrot_server_dashcam_sync_fixtures import Config
from carrot_server_dashcam_sync_lifetimes import lifetimes
from carrot_server_dashcam_sync_force import startup_force
from carrot_server_dashcam_sync_app import grace_expiry, packaging, smoke
from carrot_server_dashcam_upload import save


async def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--worker', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--composed', action='store_true')
    parser.add_argument('--concurrency', action='store_true')
    parser.add_argument('--lifetimes', action='store_true')
    parser.add_argument('--source-lifetimes', type=Path)
    parser.add_argument('--force-startup', action='store_true')
    parser.add_argument('--drop-startup', action='store_true')
    parser.add_argument('--app-smoke', action='store_true')
    parser.add_argument('--source-smoke', type=Path)
    parser.add_argument('--grace-expiry', action='store_true')
    args = parser.parse_args()
    output = args.output.resolve(); output.mkdir(parents=True)
    repo = output/'owned-repository'; repository(repo)
    config = Config(args.binary.resolve(), args.worker.resolve(), repo, output, args.composed)
    save(output/'invocation.json', {'command': [sys.executable, '-P', *sys.argv],
        'binary': {'path': str(config.binary), 'sha256': hashlib.sha256(config.binary.read_bytes()).hexdigest()},
        'worker': {'path': str(config.worker), 'sha256': hashlib.sha256(config.worker.read_bytes()).hexdigest()},
        'scope': 'new sync HTTP boundary; unchanged library corpora reused'})
    if args.grace_expiry:
        await grace_expiry(config)
    elif args.app_smoke:
        assert args.source_smoke is not None
        await smoke(config, args.source_smoke.resolve()); await packaging(config)
    elif args.force_startup or args.drop_startup:
        await startup_force(config, 'drop' if args.drop_startup else 'force')
    elif args.lifetimes:
        await lifetimes(config, args.source_lifetimes.resolve() if args.source_lifetimes else None)
    elif args.concurrency:
        await two_calls(config); await alongside_job(config)
    else:
        await boundary(config)
    print('sync HTTP oracle controls PASS')


if __name__ == '__main__':
    anyio.run(main)
