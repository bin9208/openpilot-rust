from dataclasses import asdict
import time


def observe_owners(peer, events: list[dict], started: float) -> list:
  owners: list = []
  create = peer._RTCPeerConnection__createDtlsTransport

  def observed_create():
    dtls = create()
    index = len(owners)
    owners.append(dtls)
    ice_start, dtls_start = dtls.transport.start, dtls.start

    async def observed_ice_start(parameters):
      events.append(
        {
          'phase': 'ice-start',
          'owner': index,
          'seconds': time.monotonic() - started,
          'state': dtls.transport.state,
          'role': dtls.transport.role,
          'parameters': asdict(parameters),
        }
      )
      try:
        await ice_start(parameters)
      finally:
        events.append({'phase': 'ice-return', 'owner': index, 'seconds': time.monotonic() - started, 'state': dtls.transport.state})

    async def observed_dtls_start(parameters):
      events.append(
        {
          'phase': 'dtls-start',
          'owner': index,
          'seconds': time.monotonic() - started,
          'state': dtls.state,
          'local_role': dtls._role,
          'parameters': asdict(parameters),
        }
      )
      try:
        await dtls_start(parameters)
      finally:
        events.append({'phase': 'dtls-return', 'owner': index, 'seconds': time.monotonic() - started, 'state': dtls.state, 'local_role': dtls._role})

    dtls.transport.start = observed_ice_start
    dtls.start = observed_dtls_start
    return dtls

  peer._RTCPeerConnection__createDtlsTransport = observed_create
  return owners
