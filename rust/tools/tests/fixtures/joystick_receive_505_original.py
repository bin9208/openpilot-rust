# #505 counterexample: unmodified original nested receive function.
# Source: rust/tools/check_joystickd_runtime.py:111 at b55552dbb / afed1a4.
# Original checker blob: fbf949431043b611eee0c928f0e1323e06f1e23c.
# Loaded as AST with controlled globals by the stdlib test; never import this file.
def receive() -> dict | None:
  cc, cs = None, None
  deadline = time.monotonic() + 2
  while cc is None or cs is None:
    if cc is None:
      cc = messaging.recv_one_or_none(cc_socket)
    if cs is None:
      cs = messaging.recv_one_or_none(cs_socket)
    if cc is None and cs is None and process.poll() is not None:
      return None
    assert time.monotonic() < deadline, 'publication timeout'
    if cc is None or cs is None:
      time.sleep(0.001)
  records.append({'carControl': cc.to_dict(), 'controlsState': cs.to_dict()})
  return snapshot(cc, cs)
