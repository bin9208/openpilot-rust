#!/usr/bin/env python3
"""Build carrot-man's lane-count file (NLK1) from the Korean standard node-link
(ITS 표준노드링크, MOCT_LINK.shp).

    tools/navigation/nodelink_nlk.py <folder with MOCT_LINK.shp> nodelink_kr.nlk

Copy the result to /data/navigation/nodelink/nodelink_kr.nlk on the device.
Needs numpy, pyshp and pyproj on the build machine (not on the device).

Format (little endian):
  b"NLK1", u32 tile count, u32 0
  tiles sorted by (ilat, ilon): i32 ilat, i32 ilon, u32 offset, u32 length
  tile = 0.01 deg cell [ilat/100, ilat/100+0.01) x [ilon/100, ...):
    u32 chunk count, then per chunk u8 lanes, u8 road rank x10, u8 max speed
    (km/h), u8 point count, points as (i16 north, i16 east) in 0.25 m from the
    cell's south-west corner, east scaled by cos(cell centre latitude).
  A chunk is a run of one directed link's segments whose midpoints fall in
  the cell; LANES is the lane count in the link's direction.
"""
import glob
import math
import os
import struct
import sys
from collections import defaultdict

import numpy as np

TILE = 0.01
UNIT = 0.25
METERS = 111320.0
SIMPLIFY_M = 1.0
MAX_SEGMENT_M = 400.0
RANK = {'101': 10, '102': 9, '103': 7, '104': 6, '105': 6, '106': 4, '107': 3, '108': 2}


def simplify(xy, tolerance):
    keep = np.zeros(len(xy), bool)
    keep[[0, -1]] = True
    stack = [(0, len(xy) - 1)]
    while stack:
        a, b = stack.pop()
        if b <= a + 1:
            continue
        p, q = xy[a], xy[b]
        d = q - p
        n = math.hypot(*d)
        pts = xy[a + 1:b] - p
        dist = np.abs(pts[:, 0] * d[1] - pts[:, 1] * d[0]) / n if n > 1e-9 else np.hypot(pts[:, 0], pts[:, 1])
        i = int(np.argmax(dist))
        if dist[i] > tolerance:
            keep[a + 1 + i] = True
            stack += [(a, a + 1 + i), (a + 1 + i, b)]
    return keep


def links(src):
    import pyproj
    import shapefile
    shp = sorted(glob.glob(f'{src}/**/MOCT_LINK.shp', recursive=True))[0]
    prj = shp[:-4] + '.prj'
    crs = pyproj.CRS.from_wkt(open(prj).read()) if os.path.exists(prj) else pyproj.CRS.from_epsg(5186)
    transform = pyproj.Transformer.from_crs(crs, 4326, always_xy=True)
    reader = shapefile.Reader(shp, encoding='cp949')
    names = [f[0] for f in reader.fields[1:]]
    for record in reader.iterShapeRecords():
        fields = dict(zip(names, record.record))
        xy = np.array(record.shape.points, np.float64)
        if len(xy) < 2:
            continue
        lon, lat = transform.transform(xy[:, 0], xy[:, 1])
        yield (np.stack([lat, lon], 1), int(fields.get('LANES') or 0),
               RANK.get(str(fields.get('ROAD_RANK', '')), 0), int(fields.get('MAX_SPD') or 0))


def build(src, out):
    tiles = defaultdict(list)
    count = 0
    for latlon, lanes, rank, speed in links(src):
        count += 1
        scale = math.cos(math.radians(latlon[0, 0]))
        local = (latlon - latlon[0]) * [METERS, METERS * scale]
        latlon = latlon[simplify(local, SIMPLIFY_M)]
        dense = [latlon[0]]
        for a, b in zip(latlon[:-1], latlon[1:]):
            length = math.hypot((b[0] - a[0]) * METERS, (b[1] - a[1]) * METERS * scale)
            pieces = max(1, math.ceil(length / MAX_SEGMENT_M))
            dense += [a + (b - a) * k / pieces for k in range(1, pieces + 1)]
        dense = np.array(dense)
        mid = (dense[:-1] + dense[1:]) / 2
        keys = np.floor(mid / TILE).astype(np.int64)
        start = 0
        for i in range(1, len(keys) + 1):
            if i == len(keys) or (keys[i] != keys[start]).any() or i - start >= 254:
                tiles[(int(keys[start, 0]), int(keys[start, 1]))].append(
                    (min(lanes, 255), rank, min(max(speed, 0), 255), dense[start:i + 1]))
                start = i
    directory, blobs, offset = [], [], 12 + 16 * len(tiles)
    for key in sorted(tiles):
        lat0, lon0 = key[0] * TILE, key[1] * TILE
        scale = math.cos(math.radians(lat0 + TILE / 2))
        parts = [struct.pack('<I', len(tiles[key]))]
        for lanes, rank, speed, pts in tiles[key]:
            q = np.round(np.stack([(pts[:, 0] - lat0) * METERS, (pts[:, 1] - lon0) * METERS * scale], 1) / UNIT)
            parts += [struct.pack('<BBBB', lanes, rank, speed, len(pts)),
                      np.clip(q, -32768, 32767).astype('<i2').tobytes()]
        blob = b''.join(parts)
        directory.append(struct.pack('<iiII', key[0], key[1], offset, len(blob)))
        blobs.append(blob)
        offset += len(blob)
    with open(out, 'wb') as f:
        f.write(b'NLK1' + struct.pack('<II', len(tiles), 0))
        f.write(b''.join(directory))
        f.write(b''.join(blobs))
    print(f'{out}: links {count}, tiles {len(tiles)}, {offset / 1e6:.1f} MB')


if __name__ == '__main__':
    build(sys.argv[1], sys.argv[2])
