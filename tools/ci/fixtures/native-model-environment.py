#!/usr/bin/env python3
"""Exercise actual Python/native preparation dependencies without opening GPUs."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import sys


def digest(path):
  with path.open('rb') as stream: return hashlib.file_digest(stream, 'sha256').hexdigest()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('--source',type=Path,required=True)
  parser.add_argument('--wheels',type=Path,required=True)
  parser.add_argument('--compiler',type=Path,required=True)
  parser.add_argument('--output',type=Path,required=True)
  args = parser.parse_args()
  assert tuple(sys.version_info[:3]) == (3,12,16) and platform.machine() == 'aarch64'
  assert sys.flags.isolated and sys.flags.no_site and sys.flags.dont_write_bytecode
  source = args.source.resolve()
  wheels = json.loads(args.wheels.read_text())
  tree = Path(wheels['tree_receipt'])
  assert digest(tree) == wheels['tree_sha256']
  packages = Path(wheels['site_packages']).resolve()
  compilers = json.loads((source/'rust/native-model-compilers.json').read_text())
  assert compilers['factory_image_sha256'] == '375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44'
  for value in (compilers['cpu_llvm'],compilers['qcom']):
    path = Path(value['path'])
    assert path.stat().st_size == value['bytes'] and digest(path) == value['sha256']
  os.environ.update(DEV='CPU:LLVM',CPU_COUNT='1',LLVM_PATH=compilers['cpu_llvm']['path'],
      OPENBLAS_NUM_THREADS='1',OMP_NUM_THREADS='1',PYTHON_ZSTANDARD_IMPORT_POLICY='cext',CACHELEVEL='0',
      CACHEDB=str(args.output.with_suffix('.sqlite')))
  sys.path[:0] = [str(packages),str(source/'rust/tools'),str(source/'tinygrad_repo'),str(source)]
  import native_model_inputs
  native_model_inputs.verify(native_model_inputs.read_json(tree,16*1024**2),Path(wheels['root']))
  import numpy as np
  import zstandard
  assert np.__version__ == '2.5.3' and zstandard.__version__ == '0.25.0' and zstandard.backend == 'cext'
  assert Path(np.__file__).resolve().is_relative_to(packages) and Path(zstandard.__file__).resolve().is_relative_to(packages)
  value = np.array([[3.,0.],[0.,2.]],dtype=np.float64)
  np.testing.assert_array_equal(np.linalg.svd(value,compute_uv=False),[3.,2.])
  np.testing.assert_array_equal(np.linalg.inv(value)@value,np.eye(2))
  data = b'owned model preparation zstandard roundtrip\x00'*100
  assert zstandard.ZstdDecompressor().decompress(zstandard.ZstdCompressor().compress(data)) == data
  from tinygrad import Tensor
  from model_export.offline_devices import host_only_devices
  from model_export.offline_compiler import CompilerCommand,CompilerFailure,OfflineQcomCompiler
  from model_export import offline_pipeline
  from openpilot.system.camerad.cameras.nv12_info import get_nv12_info
  assert {offline_pipeline.DM_INPUT_SIZE,offline_pipeline.MEDMODEL_INPUT_SIZE} == {(1440,960),(512,256)}
  cameras = {str(camera):get_nv12_info(*camera) for camera in ((1928,1208),(1344,760))}
  with host_only_devices():
    tensor = Tensor([1.,2.,3.,4.],device='CPU:LLVM')
    np.testing.assert_array_equal(((tensor+1)*2).numpy(),[4.,6.,8.,10.])
    compiler = OfflineQcomCompiler(CompilerCommand((str(args.compiler),'--library',compilers['qcom']['path']),60))
    source_cl = ('__kernel void initialized_add(__global float *out, __global const float *input, '
                 '__global const float *weight) { uint i = get_global_id(0); out[i] = input[i] + weight[i]; }')
    binary = compiler.compile(source_cl)
    assert len(binary) > 256 and b'initialized_add\x00' in binary
    try:
      compiler.compile('__kernel void invalid( {')
    except CompilerFailure:
      pass
    else:
      raise AssertionError('genuine QCOM compiler accepted invalid source')
  args.output.with_suffix('.kernel.bin').write_bytes(binary)
  with Path('/proc/self/maps').open('rb') as stream: maps = stream.read(4*1024**2+1)
  assert len(maps) <= 4*1024**2 and b'libLLVM.so.20.1' in maps
  args.output.with_suffix('.maps.txt').write_bytes(maps)
  native_model_inputs.verify(native_model_inputs.read_json(tree,16*1024**2),Path(wheels['root']))
  result = dict(schema=1,python_version=platform.python_version(),architecture=platform.machine(),
      wheels_tree_sha256=wheels['tree_sha256'],numpy_version=np.__version__,zstandard_version=zstandard.__version__,
      zstandard_backend=zstandard.backend,cpu_llvm=compilers['cpu_llvm'],qcom=compilers['qcom'],
      compiler_executable_sha256=digest(args.compiler),qcom_kernel_sha256=hashlib.sha256(binary).hexdigest(),
      qcom_kernel_bytes=len(binary),invalid_qcom_source_rejected=True,original_offline_pipeline_imported=True,
      cpu_llvm_tensor_matches=True,numerics_and_zstandard_match=True,cameras=cameras,
      full_model_generation=False,full_model_regression=False,full_runtime_build=False,gpu_executed=False,
      physical_device_execution=False)
  args.output.write_text(json.dumps(result,indent=2)+'\n')


if __name__ == '__main__':
  main()
