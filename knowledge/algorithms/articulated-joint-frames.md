---
type: Algorithm
title: Articulated joint frame conversion
description: Resolves two body-local endpoints and their angular ranges in a portable X-twist frame.
resource: https://github.com/smichea/osage/blob/main/tools/object-convert/fragment.py
licence: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions
status: draft
tags: [geometry, physics, objects]
sources:
  - id: conversion
    title: Independently encoded frame and angular-stop tests
    resource: https://github.com/smichea/osage/blob/main/tools/object-convert/test_fragment.py
generated: { by: "OpenAI/Codex", at: "2026-11-24T17:37:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Problem

A joint can have a different local orientation on each of its two bodies. The direction in which each rotation matrix transforms coordinates and the axis carrying the hinge or twist motion determine how to assemble its frame. A mistaken interpretation can engage an angular stop in the rest pose.

# Idea

Select the source convention explicitly. Express each endpoint in its body's coordinates, carry both into actor space and check that they define one joint at rest.[^conversion]

# Method

For joint-to-body axes with twist on X, retain the three stored directions. For a body-to-joint rotation with twist on Z, first validate its handedness and orthonormality. Its columns give the joint axes in body coordinates. Select columns Z, X and Y, in that order, for the portable frame. The anchor already belongs to body space.

Keep the signed limits of a hinge on portable X. For a Z-twist three-axis joint, the source twist extent becomes the portable X extent. Lean towards the first transverse direction is produced by turning about the other transverse axis. The second lean extent therefore supplies portable Y and the first supplies portable Z. For example, lean extents 5° and 20° with twist 45° yield principal rotation limits ±45°, ±20° and ±5°.

# Invariants

Both endpoint transforms describe the same rest point and orientation in actor space. The conversion preserves the parent relation, raw source fields and physical dimensions. Choosing a different output storage space changes where the resolved frames are expressed. A signed hinge interval retains its handedness.

# Cost

The conversion performs a fixed amount of work per endpoint. Each joint is resolved once at import.

# Failure modes

A malformed frame, inconsistent rest endpoints or invalid angular limits fail validation. Symmetric frames can hide errors in the interpretation of rows and columns. Valid principal extents permit several possible envelopes for simultaneous rotations. The three reported intervals alone do not specify a coupled swing cone.

# Alternatives

A file whose author supplies the joint axes in body coordinates with twist on X uses the existing default. Guessing a convention from agreement of the two endpoints is ambiguous when both bodies have the same orientation.

# Oracle

The fixture represents fourteen joints, including four signed hinges. The test independently writes both endpoint representations and checks that their complete portable articulations agree in actor and bone coordinates. With the optional binary bridge it also packs both sources and compares the outputs of the loose-binary reader.[^conversion]

# Reuse

Select the source representation with `--joint-frames body-to-joint-z` or `--joint-frames joint-to-body-x` for an articulated fragment. An explicit choice is recorded in `fragmentPhysics.jointFrameConvention`.

The converter can also mark eligible joints with an explicit [swing envelope](/algorithms/articulated-swing-envelope.md).

[^conversion]: [Fragment conversion tests](https://github.com/smichea/osage/blob/main/tools/object-convert/test_fragment.py).
