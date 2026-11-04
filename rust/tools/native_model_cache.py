"""Immutable, per-pipeline generated artifacts with an atomic catalog pointer.

Unlike a downloaded wheel, generated output has no upstream expected digest.
Its receipt records the trusted producer's output and original source, and is
content addressed. This is an integrity cache, not a signature from a third party.
"""
from __future__ import annotations

import fcntl
import os
from pathlib import Path
import re
import shutil
import stat
import tempfile

from native_model_inputs import (InputError, digest, directory, encoded, normal,
                                publish as rename, read_bytes, read_json, sha,
                                sync_directories, verify, write)


def snapshot(root, maximum_bytes, *, freeze=False):
  entries, total, pending = {}, 0, [Path()]
  if not stat.S_ISDIR(root.lstat().st_mode): raise InputError('generated bundle root is redirected')
  while pending:
    parent = pending.pop()
    for child in sorted((root/parent).iterdir()):
      path = (parent/child.name).as_posix()
      if not normal(path) or len(entries) >= 20000: raise InputError('unsafe or excessive generated bundle members')
      mode = child.lstat().st_mode
      if stat.S_ISDIR(mode):
        entries[path] = dict(kind='directory')
        pending.append(parent/child.name)
      elif stat.S_ISREG(mode):
        size = child.lstat().st_size
        total += size
        if total > maximum_bytes: raise InputError('generated model exceeds its admitted output bound')
        if freeze: child.chmod(0o444)
        if stat.S_IMODE(child.lstat().st_mode) != 0o444: raise InputError('generated model file is mutable')
        entries[path] = dict(kind='file', bytes=size, sha256=digest(child, size), mode=0o444)
        if freeze:
          with child.open('rb') as stream: os.fsync(stream.fileno())
      else:
        raise InputError('generated model contains a link or special file')
  tree = dict(schema=1, entries=entries)
  if len(encoded(tree)) > 16*1024**2: raise InputError('generated model tree exceeds its metadata bound')
  return tree


def entry_for(contract, tree):
  kind, camera = contract['kind'], contract['camera']
  if kind not in ('driving', 'driver') or camera not in ([1928, 1208], [1344, 760]):
    raise InputError('invalid generated model selection')
  return dict(kind=kind, camera=camera, directory=f'{kind}-{camera[0]}x{camera[1]}',
              graph_sha256=tree['entries']['graph.json']['sha256'],
              pipeline_sha256=tree['entries']['pipeline.json']['sha256'])


def check_result(result, contract, maximum_bytes):
  if not stat.S_ISDIR(result.lstat().st_mode) or not re.fullmatch('[0-9a-f]{64}', result.name):
    raise InputError('generated result is redirected or lacks a content identity')
  raw = read_bytes(result/'receipt.json', 16*1024**2)
  if sha(raw) != result.name: raise InputError('generated receipt differs from its content identity')
  receipt = read_json(result/'receipt.json', 16*1024**2)
  if (set(receipt) != {'schema', 'input', 'origin', 'entry', 'tree'} or receipt['schema'] != 1
      or receipt['input'] != contract or not re.fullmatch('[0-9a-f]{40}', receipt['origin']['commit'])
      or not re.fullmatch('[0-9a-f]{40}', receipt['origin']['tree'])):
    raise InputError('generated receipt differs from the requested inputs or original source')
  if {p.name for p in result.iterdir()} != {'receipt.json', 'bundle'}:
    raise InputError('generated result has an unexpected member')
  tree = receipt['tree']
  if tree != snapshot(result/'bundle', maximum_bytes): raise InputError('generated model differs from its frozen output tree')
  if receipt['entry'] != entry_for(contract, tree): raise InputError('generated catalog entry differs from output bytes')
  descriptor = read_json(result/'bundle/pipeline.json')
  if (descriptor['kind'] != contract['kind'] or descriptor['camera'] != contract['camera']
      or descriptor['sources'] != {'onnx': contract['model']['sha256']}
      or descriptor['nv12'] != contract['nv12']):
    raise InputError('generated descriptor differs from its exact model and camera inputs')
  return receipt


def cached(cache, contract, maximum_bytes):
  parent = cache/sha(encoded(contract))
  if not parent.exists() and not parent.is_symlink(): return None
  if not stat.S_ISDIR(parent.lstat().st_mode): raise InputError('generated input cache is redirected')
  children = list(parent.iterdir())
  if len(children) != 1: raise InputError('generated input cache is incomplete or ambiguous')
  return children[0], check_result(children[0], contract, maximum_bytes)


def link_bundle(source, destination, tree):
  destination.mkdir()
  linked = 0
  for name, entry in sorted(tree['entries'].items()):
    if entry['kind'] == 'directory':
      (destination/name).mkdir()
    else:
      os.link(source/name, destination/name, follow_symlinks=False)
      linked += entry['bytes']
  verify(tree, destination)
  return linked


def publish(root, requests, origin, validate, revalidate):
  """requests are (complete input contract, output bound, trusted producer).

  validate checks the assembled catalog with the real runtime consumer. Neither
  a cache entry nor current.json replaces that validation or input revalidation.
  A failed/cancelled operation never deletes an existing generation or log.
  """
  if not requests or len(requests) > 4: raise InputError('invalid generated model request count')
  if any(type(bound) is not int or not 0 < bound <= 2*1024**3 for _,bound,_ in requests):
    raise InputError('invalid generated model output allowance')
  identities = [sha(encoded(c)) for c, _, _ in requests]
  if len(set(identities)) != len(identities): raise InputError('duplicate generated model inputs')
  selections = [(c['kind'], tuple(c['camera'])) for c, _, _ in requests]
  if len(set(selections)) != len(selections): raise InputError('duplicate generated model selection')
  directory(root)
  lock = os.open(root/'lock', os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | os.O_NONBLOCK, 0o600)
  try:
    if not stat.S_ISREG(os.fstat(lock).st_mode): raise InputError('generated model lock is not regular')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    current = root/'current.json'
    if current.exists() or current.is_symlink():
      pointer = read_json(current, 1024)
      if (set(pointer) != {'version', 'generation'} or type(pointer['version']) is not int
          or pointer['version'] != 1 or not isinstance(pointer['generation'], str)
          or not re.fullmatch('[0-9a-f]{64}', pointer['generation'])):
        raise InputError('existing model pointer is invalid; it is not repaired implicitly')
      previous = root/pointer['generation']
      if not stat.S_ISDIR(previous.lstat().st_mode) or sha(read_bytes(previous/'index.json', 1024**2)) != pointer['generation']:
        raise InputError('existing model pointer does not identify its immutable catalog')
    cache = root/'.model-cache'
    directory(cache)
    found = [cached(cache, contract, bound) for contract, bound, _ in requests]
    growth = sum(bound for (_, bound, _), hit in zip(requests, found, strict=True) if hit is None)
    # Hard links do not duplicate output data. Admit missing producer outputs
    # plus bounded receipts/catalog directory entries; no fixed disk floor.
    growth = growth+8*1024**2 if growth else 65536
    if shutil.disk_usage(root).free < growth: raise InputError('insufficient capacity for this model generation operation')
    generated, generated_bytes, linked, pending = 0, 0, 0, []
    with tempfile.TemporaryDirectory(prefix='.build-', dir=root) as temporary:
      stage = Path(temporary)
      results = []
      for (contract, bound, produce), hit in zip(requests, found, strict=True):
        if hit is None:
          parent = stage/sha(encoded(contract))
          parent.mkdir()
          output = parent/'output'
          output.mkdir()
          produce(output/'bundle')
          tree = snapshot(output/'bundle', bound, freeze=True)
          receipt = dict(schema=1, input=contract, origin=origin, entry=entry_for(contract, tree), tree=tree)
          raw = encoded(receipt)
          if len(raw) > 16*1024**2: raise InputError('generated receipt exceeds its byte bound')
          write(output/'receipt.json', raw)
          result = parent/sha(raw)
          output.rename(result)
          sync_directories(parent)
          hit = result, check_result(result, contract, bound)
          pending.append((parent, cache/parent.name))
          generated += 1
          generated_bytes += sum(e['bytes'] for e in tree['entries'].values() if e['kind']=='file')
        results.append(hit)
      entries = [receipt['entry'] for _, receipt in results]
      index = encoded(dict(version=1, bundles=entries))
      generation_id = sha(index)
      destination = root/generation_id
      expected = {'index.json': dict(kind='file', bytes=len(index), sha256=generation_id, mode=0o444)}
      for _, receipt in results:
        name = receipt['entry']['directory']
        expected[name] = dict(kind='directory')
        expected.update({f'{name}/{p}': e for p, e in receipt['tree']['entries'].items()})
      tree = dict(schema=1, entries=expected)
      if destination.exists() or destination.is_symlink():
        verify(tree, destination)
        pointer = encoded(dict(version=1, generation=generation_id))
        if (root/'current.json').exists() and read_bytes(root/'current.json', 1024) == pointer:
          catalog_root = root
          validate(root)
        else:
          catalog_root = stage/'check'
          catalog_root.mkdir()
          write(catalog_root/'current.json', pointer)
          check_generation = catalog_root/generation_id
          check_generation.mkdir()
          write(check_generation/'index.json', index)
          for path, receipt in results:
            linked += link_bundle(path/'bundle', check_generation/receipt['entry']['directory'], receipt['tree'])
          validate(catalog_root)
      else:
        catalog_root = stage/'catalog'
        catalog_root.mkdir()
        generation = catalog_root/generation_id
        generation.mkdir()
        write(generation/'index.json', index)
        for path, receipt in results:
          linked += link_bundle(path/'bundle', generation/receipt['entry']['directory'], receipt['tree'])
        verify(tree, generation)
        write(catalog_root/'current.json', encoded(dict(version=1, generation=generation_id)))
        validate(catalog_root)
        sync_directories(generation)
      # Detect source, model, environment, compiler or output mutations while
      # the producer/consumer ran, before any current pointer is replaced.
      revalidate()
      for (contract, bound, _), (path, _) in zip(requests, results, strict=True):
        check_result(path, contract, bound)
      for parent, target in pending: rename(parent, target)
      if not destination.exists(): rename(catalog_root/generation_id, destination)
      verify(tree, destination)
      pointer = encoded(dict(version=1, generation=generation_id))
      current = root/'current.json'
      if current.exists() or current.is_symlink():
        same = read_bytes(current, 1024) == pointer
      else:
        same = False
      if not same:
        write(stage/'current.json', pointer)
        os.replace(stage/'current.json', current)
        fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try: os.fsync(fd)
        finally: os.close(fd)
      return dict(generation=generation_id, generated_pipelines=generated,
                  published_generated_output_bytes=generated_bytes,
                  reused_pipelines=len(requests)-generated, linked_output_bytes=linked,
                  expected_additional_bytes=growth, origins=[receipt['origin'] for _, receipt in results],
                  inputs=identities, consumer_validated=True, runtime_activated=False)
  finally:
    os.close(lock)
