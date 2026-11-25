#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Author a passive quadruped contact fixture in metres and kilograms."""
import sys
from pathlib import Path
from make_pedestrian import write_fragment


def write(path):
    shoulder = (0, 0.64, 0.32)
    hip = (0, 0.64, -0.32)
    neck = (0, 0.76, 0.52)
    # Small joint ranges retain the standing shape for the drop and lift tests.
    # These are authored mechanical parameters, not anatomical measurements.
    joint = (2, 2, 2)
    parts = [
        ("trunk", None, hip, shoulder, .16, 10., None),
        ("neck", 0, shoulder, neck, .075, 1., joint),
        ("muzzle", 1, neck, (0, .78, .74), .085, 2., joint),
        ("tail", 0, hip, (0, .73, -.72), .045, .6, (15, 15, 15)),
    ]
    for end,z in (("front",.30),("rear",-.30)):
        for side,x in (("left",.15),("right",-.15)):
            limb = f"{end}_{side}"
            leg = len(parts)
            knee = (x,.34,z-.025)
            ankle = (x,.065,z)
            toe = (x,.065,z+.09)
            parts.extend([
                (limb+"_upper", 0, (x,.63,z), knee, .052, .9, joint),
                (limb+"_lower", leg, knee, ankle, .038, .6, joint),
                (limb+"_paw", leg+1, ankle, toe, .065, .35, joint),
            ])
    write_fragment(path,parts,"osage_quadruped",((-.23,-.86,0),(.23,.8,.9)),(0,0,.45),1.1,1.1)


if __name__ == "__main__":
    write(Path(sys.argv[1]))
