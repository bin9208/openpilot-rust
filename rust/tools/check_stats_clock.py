# /// script
# requires-python = ">=3.12,<3.13"
# dependencies = []
# ///
# Run: python rust/tools/check_stats_clock.py EXAMPLE OUTPUT
"""Independent datetime double-rounding oracle across the supported calendar."""
import ctypes
import struct
import hashlib
import json
import random
import subprocess
import sys
from datetime import datetime, UTC
from pathlib import Path


def main():
  binary,output=[Path(value).resolve() for value in sys.argv[1:]]
  rng=random.Random(7500)
  times=[(seconds,nanos) for seconds in [-62135596800,-2147483648,-1,0,1,1723456789,2147483647,253402300799]
         for nanos in [0,1,999,1000,123456789,999999999]]
  times.extend((rng.randrange(-62135596800,253402300800),rng.randrange(1000000000)) for _ in range(5000))
  expected=[int(datetime.fromtimestamp(seconds,UTC).replace(microsecond=nanos//1000).timestamp()*1e9) for seconds,nanos in times]
  child=subprocess.run([str(binary)],input=''.join(json.dumps(value)+'\n' for value in times),text=True,capture_output=True,check=True)
  actual=[int(value) for value in child.stdout.splitlines()]
  differences=[{'input':pair,'source':source,'native':native} for pair,source,native in zip(times,expected,actual,strict=True) if source!=native]
  nanos=[0, 1, 999999999, 1000000000, 1000000001, 2**53-1, 2**53, 2**53+1, 2**63-1]
  nanos.extend(rng.randrange(2**63) for _ in range(5000))
  function=ctypes.pythonapi._PyTime_AsSecondsDouble
  function.argtypes=[ctypes.c_int64]
  function.restype=ctypes.c_double
  expected_mono=[struct.unpack('Q',struct.pack('d',function(value)))[0] for value in nanos]
  child=subprocess.run([str(binary),'--monotonic'],input=''.join(json.dumps(divmod(value,1000000000))+'\n' for value in nanos),text=True,capture_output=True,check=True)
  actual_mono=[int(value) for value in child.stdout.splitlines()]
  mono_differences=[{'input':value,'source_bits':source,'native_bits':native} for value,source,native in zip(nanos,expected_mono,actual_mono,strict=True) if source!=native]
  boundaries=[]
  for seconds in [-62135596801,253402300800]:
    try:
      datetime.fromtimestamp(seconds,UTC)
    except ValueError as error:
      source_error=type(error).__name__
    else:
      raise AssertionError('calendar boundary accepted by source')
    child=subprocess.run([str(binary)],input=json.dumps([seconds,0])+'\n',text=True,capture_output=True)
    assert child.returncode!=0
    boundaries.append({'seconds':seconds,'source_error':source_error,'native_exit':child.returncode,'native_stderr':child.stderr})
  output.write_text(json.dumps({'calendar_errors':boundaries,'cases':len(times),'differences':differences,'monotonic_cases':len(nanos),'monotonic_differences':mono_differences,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()},indent=2))
  print(f'{len(times)} datetime timestamp cases, {len(differences)} differences')
  print(f'{len(nanos)} CPython monotonic conversion cases, {len(mono_differences)} differences')
  assert not differences and not mono_differences


if __name__=='__main__':
  main()
