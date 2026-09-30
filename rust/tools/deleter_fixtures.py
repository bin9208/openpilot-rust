def scenarios():
  ordinary = [("route--" + str(i)).encode() for i in (10, 2, 1, 0, 3, 4, 5)]
  yield "threshold-order", ordinary + [b"boot", b"crash", b"2024-old--10", b"2024-old--2"], [
    ("cycle", 5 * 1024**3, "30"), ("cycle", 5 * 1024**3 - 1, "30"),
    ("cycle", 5 * 1024**3, "29.999999999999996"),
    *[("cycle", 0, "0")] * 12,
  ]
  yield "locks-and-preserve", ordinary + [b"boot", b"crash"], [
    ("lock", b"route--0"), ("attr", b"route--3", b"1"), ("preserved",),
    *[("cycle", 0, "0")] * 9,
  ]
  yield "cached-attributes", ordinary, [
    ("preserved",), ("attr", b"route--3", b"1"), ("preserved",),
    ("cycle", 0, "0"), ("cycle", 0, "0"),
  ]
  names = [b"route--" + str(i).encode() for i in range(20)]
  yield "five-marked-segments", names, [
    *[("attr", b"route--" + str(i).encode(), b"1") for i in (1, 4, 7, 10, 13, 16, 19)],
    ("preserved",), ("cycle", 0, "0"),
  ]
  yield "invalid-marked-counts", [b"zzz"] + [b"route--" + str(i).encode() for i in range(8)], [
    *[("attr", name, b"1") for name in [b"zzz", b"route--1", b"route--3", b"route--4", b"route--5", b"route--6"]],
    ("preserved",), ("cycle", 0, "0"),
  ]
  special = [
    b"r--+003", b"r---1", b"r---0", b"r--1_2", b"r--_1", b"r--1_", b"r--1__2",
    b"--9", b"plain", b"r--2x", b"r--", b"r--" + b"9" * 150,
    "r--  １２  ".encode(), "r--١٢".encode(), "r--²".encode(),
    b"r--\xff", b"\xff--3", b"r--\x1c4", "r--\u00854".encode(),
  ]
  for index, name in enumerate(special):
    yield f"numeric-name-{index}", [name], [("attr", name, b"1"), ("preserved",), ("cycle", 0, "0")]
  yield "attribute-value", ordinary, [
    ("attr", b"route--1", b"01"), ("attr", b"route--2", b""), ("attr", b"route--3", b"1\0"),
    ("attr", b"route--4", b"1"), ("preserved",), ("cycle", 0, "NaN"),
  ]
  yield "symlink-and-io", [b"route--1", b"route--2"], [
    ("symlink", b"route--0"), ("deny", b"route--1"), ("cycle", 0, "0"),
  ]
  yield "nan-idle", ordinary, [("cycle", 5 * 1024**3, "NaN"), ("cycle", 0, "inf")]
