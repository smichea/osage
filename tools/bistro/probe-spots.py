# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Where the centre probe of each cube stands, from the walk's grid.

    python3 tools/bistro/probe-spots.py STREET 'X,Y,Z;X,Y,Z…'

STREET holds `walk.json` and `walk.bin` (the walk's grid, as `export.py`
writes it). Prints the cubes as `cube-probes.py` takes them,
`X,Y,Z:x,y,z;…`: x,y,z are the eyes of a walker standing on the grid's cell
nearest the cube's middle, with the cells round it walked too (a wall is not
at arm's length) and 30 cm inside the cube; failing that, on the nearest
cell of the cube anybody stands on. A cube's middle is often inside a wall
or under a table while its pavement is walked. Cubes nobody stands in are
left out, and named on the standard error.
"""
import json
import math
import pathlib
import struct
import sys

CUBE = 3.0
EYES = 1.65


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    street = pathlib.Path(sys.argv[1])
    header = json.loads((street / "walk.json").read_text())
    raw = (street / "walk.bin").read_bytes()
    columns, rows, cell = header["columns"], header["rows"], header["cell"]
    ox, oz = header["origin"]

    def height(c, r):
        if c < 0 or r < 0 or c >= columns or r >= rows:
            return None
        v = struct.unpack_from("<f", raw, 4 * (r * columns + c))[0]
        return None if math.isnan(v) else v

    out = []
    for one in sys.argv[2].split(";"):
        cube = [int(v) for v in one.split(",")]
        lo_x, lo_z = cube[0] * CUBE, cube[2] * CUBE
        best = None
        for c in range(int((lo_x - ox) / cell), int((lo_x + CUBE - ox) / cell) + 1):
            for r in range(int((lo_z - oz) / cell), int((lo_z + CUBE - oz) / cell) + 1):
                x, z = ox + (c + 0.5) * cell, oz + (r + 0.5) * cell
                h = height(c, r)
                if h is None or not (lo_x <= x < lo_x + CUBE and lo_z <= z < lo_z + CUBE):
                    continue
                roomy = (lo_x + 0.3 <= x <= lo_x + CUBE - 0.3 and lo_z + 0.3 <= z <= lo_z + CUBE - 0.3
                         and all(height(c + a, r + b) is not None for a in (-1, 0, 1) for b in (-1, 0, 1)))
                rank = (not roomy, (x - lo_x - CUBE / 2) ** 2 + (z - lo_z - CUBE / 2) ** 2)
                if best is None or rank < best[0]:
                    best = (rank, x, h + EYES, z)
        if best is None:
            print(f"probe-spots: nobody stands in {one}", file=sys.stderr)
            continue
        out.append(f"{one}:{best[1]:.3f},{best[2]:.3f},{best[3]:.3f}")
    print(";".join(out))


if __name__ == "__main__":
    main()
