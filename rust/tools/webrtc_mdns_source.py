# /// script
# requires-python = ">=3.12"
# dependencies = ["aioice==0.10.2", "dnspython==2.8.0"]
# ///
"""Capture unchanged aioice mDNS resolution against an owned UDP recipient."""

import asyncio
from dataclasses import asdict, dataclass
import json
from pathlib import Path
import socket
import sys
import time

import aioice.mdns
import dns.message
import dns.rdata
import dns.rdataset
import dns.rdatatype
import dns.rrset


@dataclass(slots=True)
class Result:
  """Accumulate actual query and resolution observations for one source case."""

  scenario: str
  results: list[str | None]
  queries: list[str]
  destinations: list[tuple[str, int]]
  replies: list[str]
  pending: int = 0
  elapsed: float = 0


class OwnedTransport:
  """Route the source's unchanged multicast send call to its owned UDP recipient."""

  def __init__(self, transport: asyncio.DatagramTransport, recipient: tuple[str, int], row: Result):
    self.transport, self.recipient, self.row = transport, recipient, row

  def sendto(self, data: bytes, addr: tuple[str, int]) -> None:
    self.row.destinations.append(addr)
    self.transport.sendto(data, self.recipient)

  def close(self) -> None:
    self.transport.close()


def response(query: bytes, address: str, rdclass: int = 0x8001, *, compressed: bool = False) -> bytes:
  question = dns.message.from_wire(query).question[0]
  packet = dns.message.QueryMessage(id=0)
  packet.flags = 0x8400
  if compressed:
    packet.question.append(question)
  ipv6 = ":" in address
  packed = socket.inet_pton(socket.AF_INET6 if ipv6 else socket.AF_INET, address)
  kind = dns.rdatatype.AAAA if ipv6 else dns.rdatatype.A
  record = dns.rdata.GenericRdata(rdclass=rdclass, rdtype=kind, data=packed)
  packet.answer.append(dns.rrset.from_rdata(question.name, 120, record))
  return packet.to_wire()


async def scenario(kind: str) -> Result:
  loop = asyncio.get_running_loop()
  row = Result(kind, [], [], [], [])
  with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as recipient:
    recipient.bind(("127.0.0.1", 0))
    recipient.setblocking(False)
    tx, _ = await loop.create_datagram_endpoint(asyncio.DatagramProtocol, local_addr=("127.0.0.1", 0))
    protocol = aioice.mdns.MDnsProtocol(OwnedTransport(tx, recipient.getsockname(), row))
    rx, _ = await loop.create_datagram_endpoint(lambda: protocol, local_addr=("127.0.0.1", 0))
    pending: list[asyncio.Task[str | None]] = []
    started = time.monotonic()
    try:
      names = ["OwNeD.local", "owned.local"] if kind in ("coalesced", "cancel") else ["OwNeD.local"]
      pending = [asyncio.create_task(protocol.resolve(name)) for name in names]
      query, _ = await asyncio.wait_for(loop.sock_recvfrom(recipient, 65536), 1)
      row.queries.append(query.hex())
      parsed = dns.message.from_wire(query)
      assert parsed.id == 0 and parsed.flags == 0 and parsed.question[0].rdtype == dns.rdatatype.A
      if kind == "cancel":
        pending[0].cancel()
        await asyncio.gather(pending[0], return_exceptions=True)
        pending = pending[1:]
      if kind == "closed":
        await protocol.close()
      elif kind != "unresolved":
        if kind == "malformed":
          notify = bytearray(response(query, "127.0.0.1"))
          notify[2] |= 0x20
          invalid = [b"\0\0\x84\0\0\0\0\x01\0\0\0\0\xc0\x0c", response(query, "127.0.0.1")[:-1], response(query, "127.0.0.1", 1), bytes(notify)]
          for packet in invalid:
            row.replies.append(packet.hex())
            await loop.sock_sendto(recipient, packet, rx.get_extra_info("sockname"))
          await asyncio.sleep(0.03)
          assert all(not task.done() for task in pending)
        packet = response(query, "::1" if kind in ("aaaa", "first") else "127.0.0.1", compressed=kind == "compressed")
        if kind == "first":
          compound = dns.message.from_wire(packet)
          compound.answer.extend(dns.message.from_wire(response(query, "127.0.0.1")).answer)
          packet = compound.to_wire()
        if kind == "flags":
          packet = b"\x12\x34\0\0" + packet[4:]
        row.replies.append(packet.hex())
        await loop.sock_sendto(recipient, packet, rx.get_extra_info("sockname"))
      row.results = await asyncio.gather(*pending)
      if kind == "uncached":
        pending = [asyncio.create_task(protocol.resolve("OwNeD.local"))]
        query, _ = await asyncio.wait_for(loop.sock_recvfrom(recipient, 65536), 1)
        row.queries.append(query.hex())
        packet = response(query, "127.0.0.1")
        row.replies.append(packet.hex())
        await loop.sock_sendto(recipient, packet, rx.get_extra_info("sockname"))
        row.results.extend(await asyncio.gather(*pending))
      row.pending = len(protocol.queries)
      row.elapsed = time.monotonic() - started
      assert row.pending == 0
      assert all(destination == ("224.0.0.251", 5353) for destination in row.destinations)
      assert len(row.queries) == (2 if kind == "uncached" else 1)
      return row
    finally:
      for task in pending:
        task.cancel()
      await asyncio.gather(*pending, return_exceptions=True)
      if kind != "closed":
        await protocol.close()


async def main(output: Path, kinds: list[str]) -> None:
  await asyncio.to_thread(output.mkdir, parents=True, exist_ok=False)
  rows = [asdict(await scenario(kind)) for kind in kinds]
  await asyncio.to_thread((output / "result.json").write_text, json.dumps(rows, indent=2) + "\n")
  print(json.dumps(rows, indent=2))


if __name__ == "__main__":
  asyncio.run(
    main(
      Path(sys.argv[1]), sys.argv[2:] or ["a", "aaaa", "coalesced", "uncached", "cancel", "closed", "malformed", "unresolved", "first", "compressed", "flags"]
    )
  )
