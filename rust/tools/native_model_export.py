"""Freeze selected model generations into one runtime input, not the cache store."""
from pathlib import Path
import os

from native_model_cache import cached
from native_model_inputs import InputError, add, encoded, read_bytes, sha, stage, verify, write


def export(cache_root, store, results, requests, validate, revalidate):
  # The complete two-kind roster is mandatory, including on a warm export.
  if set(results)!={'driving','driver'} or set(requests)!={'driving','driver'}:
    raise InputError('model export requires both complete catalogs')
  entries, files, metadata, pipelines = {}, {}, {}, {}
  def meta(name,data):
    add(entries,name,dict(kind='file',mode=0o444,bytes=len(data),sha256=sha(data)))
    metadata[name]=data
  for kind in ('driving','driver'):
    if len(requests[kind])!=2: raise InputError('model export requires two cameras per kind')
    catalog=kind+'-models'
    bundles=[]
    for contract,bound,_ in requests[kind]:
      hit=cached(cache_root/catalog/'.model-cache',contract,bound)
      if hit is None: raise InputError('model export input was not produced')
      path,receipt=hit
      tag=receipt['entry']['directory']
      bundles.append(receipt['entry'])
      raw=read_bytes(path/'receipt.json',16*1024**2)
      if sha(raw)!=path.name: raise InputError('model export receipt changed')
      pipelines[tag]=sha(raw)
      meta(f'model-inputs/{tag}.json',raw)
      for name,entry in receipt['tree']['entries'].items():
        target=f'{catalog}/{results[kind]["generation"]}/{tag}/{name}'
        add(entries,target,entry)
        if entry['kind']=='file': files[target]=path/'bundle'/name
    index=encoded(dict(version=1,bundles=bundles))
    if sha(index)!=results[kind]['generation']: raise InputError('model export catalog selection changed')
    meta(f'{catalog}/{results[kind]["generation"]}/index.json',index)
    meta(f'{catalog}/current.json',encoded(dict(version=1,generation=results[kind]['generation'])))
  contract=encoded(dict(schema=1,kind='openpilot-native-model-inputs-v1',pipelines=pipelines))
  meta('model-inputs/source-contract.json',contract)
  tree=dict(schema=1,entries=entries)
  tree_bytes=encoded(tree)
  if len(tree_bytes)>16*1024**2 or len(entries)>100000: raise InputError('model export tree exceeds its bound')
  identity=sha(contract)
  # Hard links reuse data blocks. Admission covers metadata files/receipts,
  # directory entries and directories, not a fixed free-space floor.
  growth=(sum(((len(data)+4095)//4096)*4096 for data in metadata.values())+2*len(tree_bytes)
          +sum(4096 if e['kind']=='directory' else 128+len(n.encode()) for n,e in entries.items())+65536)
  linked=copied=0
  with stage(store,identity,tree,growth) as (root,destination):
    if destination is not None:
      for name,entry in sorted(entries.items()):
        target=destination/name
        if entry['kind']=='directory': target.mkdir(parents=True,exist_ok=True)
        elif name in metadata:
          write(target,metadata[name]); copied+=len(metadata[name])
        else:
          os.link(files[name],target,follow_symlinks=False); linked+=entry['bytes']
    verify(tree,root)
    validation=validate(root)
    revalidate()
    # Independently expected tree comes from the checked producer receipts,
    # not a new snapshot of this potentially corrupted export directory.
    verify(tree,root)
  root=store/identity/'generation/tree'
  receipt=root.parent/'tree.json'
  return dict(schema=1,identity=identity,root=str(root),tree_receipt=str(receipt),tree_sha256=sha(tree_bytes),
      copied_bytes=copied,linked_bytes=linked,expected_additional_bytes=growth if destination is not None else 0,
      files={f'rust/bin/{name}':name for name,e in entries.items() if e['kind']=='file'},
      runtime_inputs=dict(files={str(receipt):sha(tree_bytes)},trees={str(root):str(receipt)}),
      consumer=validation,runtime_activated=False)
