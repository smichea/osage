---
type: Attested Computation
title: Pedestrian impact on a movable bonnet
description: Two fixed-step simulations use the same imported rig and car body. A geometric check measures contact timing, overlap and lift-off.
status: draft
tags: [oracle, physics, replay]
runtime: shell
parameters:
  - { name: actor-rig, type: path, required: true }
  - { name: out, type: path, required: true }
executor:
  resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/compare-impact.py
  receipt: [case, engines]
attester:
  resource: https://github.com/smichea/osage/blob/main/worlds/ville-paris/impact_geometry.py
generated: { by: "OpenAI/Codex", at: "2026-11-23T17:58:00Z" }
---
<!-- SPDX-FileCopyrightText: The osage contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Computation

From the repository root, build the city binary and import the original example rig. Run the two drops on the same build.

```sh
cargo build --release -p osage-ville-paris
python3 tools/object-convert/convert.py --id pedestrian \
  --fragment tools/object-convert/fixtures/pedestrian.yft.xml -o /tmp/pedestrian.glb
python3 worlds/ville-paris/compare-impact.py bench/out/ville-paris/impact --actor-rig /tmp/pedestrian.glb
```

The run writes a CSV and a JSON receipt per solver and one combined `impact.json`. Each solver repeats its trace in a new process. The comparison checks the digests of the actor, sector, replay and trace, the part table and the step counts. The parked and moving routes use the same articulated pedestrian. The car body is a freely moving 1,250 kg box. Its initial speed along the road is 0 or 2.8 m/s. [Inputs](https://github.com/smichea/osage/blob/main/worlds/ville-paris/fixtures/bonnet.json)

Add `--native-response` to the comparator to measure the contact on the native clock. The comparator can use the built-in human when `--actor-rig` is omitted. The underlying `ville-paris --physics rapier --actor human --replay worlds/ville-paris/fixtures/bonnet.json --impact-response` writes the poses and velocities directly. It retains observations for up to 50 ms on either side of the first bonnet manifold (rounded up to whole substeps where needed), its contact rows and the state at the matching public tick.

# Measurements

The first reported contact gives the earliest solver substep with a recorded part touching the bonnet. Time in seconds is `(tick − 1 + substep / 4) / 60`. The solver solves at 240 Hz. The ordinary trace records the part and box poses at 60 Hz. The optional impact window records them at 240 Hz.

For each sphere or capsule, transform its centre and axis into the moving box frame. Outside the box, minimise squared distance from its centre segment to the box across the intervals where the closest feature stays fixed. Subtract the capsule radius. If the centre segment intersects the box, take the shortest separating translation using the three box axes and the normals formed by crossing the segment with each box edge direction. Include the capsule radius in this depth. The greatest overlap across the recorded limbs and poses gives peak sampled penetration in metres. Orientation, edges and corners enter this check. It also works when the solver stops emitting contact events while resting. [Analytic checks](https://github.com/smichea/osage/blob/main/worlds/ville-paris/test_impact.py)

Whole-body rebound measures the greatest rise of the mass centre along the moving top-face normal from the last recorded contacting pose. It requires all limbs to clear the box by more than 1 mm, the mass centre to stay over the top face and a positive change of height. Another surface contact or an exit past the edge ends the observation. A null result records no qualifying lift-off. A limb can bounce on its own. Events or peaks between pose samples require a higher sampling rate.
The city can also initialize the same rig from two animation samples at the moment it becomes passive. A separate hinge oracle checks the initial velocities and momentum. [Animated ragdoll](/algorithms/animated-ragdoll.md)

## Measuring the relative velocity of the surfaces

A foot can rotate before its centre stops descending. Its nearest point on the bonnet can move from the toe toward the heel. At both ends of the first reported collision substep, the oracle independently finds the closest points on each capsule and the oriented box. It projects the relative centre velocity and both angular contributions onto the outward normal. For a line of equally close points along a flat face, the two end speeds bound the result. When the capsule spine intersects the box, the oracle takes a minimum separating translation and reconstructs both surface points on that plane. [Geometry and velocities](https://github.com/smichea/osage/blob/main/worlds/ville-paris/impact_geometry.py)

The receipt distinguishes the first native contact report from the first sampled geometric touch within the observation window. A positive separation can accompany a speculative contact. For each reported manifold point it also gives the end-of-step velocity field at that world location. This preserves the distinction between the solver's chosen point, the current geometric witness and a material point carried along by the rotating foot. For each route, it measures peak overlap over 25 ms and 50 ms starting at the earlier of the two native contact reports. It records a null peak when a captured window is incomplete. The comparator checks the input digests, first-contact interval and part set, and the float32 pose and velocity of every limb and the box at the public boundary. [Comparator](https://github.com/smichea/osage/blob/main/worlds/ville-paris/compare-impact.py)

## Sustained penetration on a supported moving box

The Bir-Hakeim grid supplies 100 incoming conditions and repeats each with a parked and a moving bonnet. A native-step instrument samples all 15 limbs and the freely moving box at 240 Hz. It preserves the ordinary 60 Hz solver configuration and exposes the poses after each internal advance. On 400 drops and four central controls, grouping four advances reproduced every measured position, orientation and velocity in the release executable's 97,364 poses, and the same first contact substep in all 404 cases. The independent segment-to-box calculation uses each limb's surface, including edges and corners, even on steps without a reported manifold. A run counts consecutive samples of the **same limb** deeper than 20 mm. [Grid generator](https://github.com/smichea/osage/blob/main/worlds/ville-paris/check-bonnet.py) · [Independent geometry](https://github.com/smichea/osage/blob/main/worlds/ville-paris/impact_geometry.py)

Rapier uses the contact stiffness of fixed terrain when the free box has received a supporting impulse from the terrain and a limb is already deeper than the native linear slop. It measures the current overlap and the relative velocity of the two surface points. If the overlap is approaching the native prediction distance within a step, it switches stiffness for that solve. The ordinary compliance returns for shallow contacts. Both rigid bodies keep their mass, inertia, velocity and friction response. [Physics implementation](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier.rs) · [Loaded-foot regression](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs)

| Rig and initial box motion | Peak overlap before → after | Longest run above 20 mm, before → after | Drops with at least three consecutive samples, before → after |
| --- | ---: | ---: | ---: |
| Built-in human, parked | 25.52 → 21.38 mm | 10 → 2 | 4 → 0 |
| Built-in human, moving | 25.71 → 22.46 mm | 5 → 2 | 3 → 0 |
| Imported pedestrian, parked | 25.84 → 20.75 mm | 10 → 2 | 3 → 0 |
| Imported pedestrian, moving | 25.42 → 20.31 mm | 5 → 1 | 2 → 0 |

With the contact stiffness change alone, the central `moving` route retained its full trajectory, including a peak of 11.44 mm for the built-in human and 10.67 mm for the imported pedestrian. The first reported contact is on both feet at 0.3875 s. At that revision, the central human settled at 19.13 s and the imported rig at 5.35 s. Settling requires every limb to stay below 0.10 m/s and 0.25 rad/s for at least half a second. The rigs remain passive throughout this measurement.

The same revision increased the built-in human’s settling time on `moving-8-0` from 3.65 to 19.98 s and on `moving-9-0` from 6.65 to 20.58 s. On `parked-9-0` it reaches rest at 4.70 s for the first time in the 60 s observation. The seven affected seeds and two central controls were also checked on the imported rig. All nine of its trajectories still settled within 60 s. On every sampled frame of all 18 runs the mechanical energy of the actor and bonnet stayed at or below its initial level. The largest rise between adjacent frames increased from 3.02 to 8.33 J for the built-in human and from 2.35 to 3.10 J for the imported rig.

Two sets of nine paired runs timed the ordinary 60 Hz public step, including loading inputs and constructing the eight worlds. On a logical processor that was 98.5% idle before timing, the median paired increase was 0.0057 ms (0.42%), with quartiles of +0.0027 and +0.0188 ms. The other processor, 73.5% idle before timing, recorded +0.056 ms (1.88%) with quartiles of +0.029 and +0.070 ms. The local benchmark receipt, `bench/out/ville-paris/bonnet-comparison-now/penetration/receipt.json`, contains the individual timings, rest times and geometric measurements.

## A free hand after the forearm finds support

On the offset moving-bonnet drop, the supported left forearm had almost stopped while the hand continued to rotate. The left hand was the fastest limb in 708 of the 732 sampled frames that exceeded the angular threshold between 6.65 and 20.58 s. The hand received no direct contact in that interval. [Solver and joint tree](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/src/rapier.rs)

D-184 checked the tree’s movable terminal joints at each native substep. A terminal body qualifies after a full sleep interval with no contact impulse on that body. Its parent must have been slow for the solver’s sleep interval and must currently carry a reaction opposing gravity. For these inputs the interval is 0.5 s. On the free angular axes a viscous joint motor targets zero relative angular velocity. Its rate is √(g/L), where L is the greater of the child anchor distance from its centre and its bounding sphere radius. Losing eligibility removes the motor. Joint limits and the current angle remain free to determine the pose. [Supported pendulum regression](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs)

| Built-in human route | Rest before damping | Rest with damping |
| --- | ---: | ---: |
| Moving, centred | 19.13 s | 3.33 s |
| Moving, offset 8-0 | 19.98 s | 4.55 s |
| Moving, offset 9-0 | 20.58 s | 4.07 s |
| Parked, offset 8-0 | Beyond 60 s | Beyond 60 s |

At 60 Hz, rest starts at the first of 31 consecutive observations below 0.10 m/s and 0.25 rad/s for every limb. In all 17 of the 18 runs that reached the threshold, this first window also began the interval of uninterrupted rest through 60 s. The nine imported-rig rest times were unchanged. On the four-second grid, each of the 400 drops retained its peak overlap and longest sequence above 20 mm. Every first contact kept its limbs and native timestep. A separate animated wrist swung through 29.28° before touching the bonnet. All 15 limb poses matched the earlier solver bit for bit throughout that flight. In a frictionless supported two-body joint, the same initial relative spin fell from 1.400 to 0.045 rad/s by one second with damping. Under common rotation the two solvers produced identical poses and impulses.

A paired CPU benchmark ran eight six-second routes on one logical processor, including input loading and world creation. Over nine alternating pairs the median paired change was −0.172 ms per public step (−8.17%), with quartiles of −0.215 and −0.155 ms. This measures the full trajectories, including earlier rest on three of them. The individual timings are in `bench/out/ville-paris/settling-next/qualification/final/cost.json`.

## An entire arm hanging over the edge

On the parked route at grid row 8, column 0, the built-in human's chest stays on the bonnet with all three left-arm segments in the air. With terminal-joint damping alone, the left hand still moved at 0.289 m/s and rotated at 1.036 rad/s at 60 s. The twelve other segments already stayed below the rest thresholds from 4.183 s. The Rapier adapter now uses the mass and inertia carried by a joint to damp a whole suspended branch. In the corrected replay, the fifteen segments first reach lasting rest at 6.633 s. The chest is still supported and all three arm segments are still clear of contact in every observation from 30 to 60 s. [Branch algorithm](/algorithms/hanging-ragdoll-branches.md)

The 60 s panel repeats nine drops on each of the two rigs. The imported pedestrian on the moving route at row 9, column 0 settles at 3.450 s, down from 3.800 s. The other sixteen runs reproduce every previous public pose and velocity of the pedestrian and bonnet. All eighteen runs now meet the rest criterion. The 404 four-second drops kept the same first-contact step and limbs, peak overlap and longest run above 20 mm at 240 Hz. Later poses changed in 129 of those drops. The largest peak remained 22.456 mm, with no limb beyond 20 mm for more than two consecutive steps.

The finite-ledge test also checks the equilibrium against gravity. Two 0.45 kg capsules hang from a hinge on a supported base. The peak relative angular speed is 8.20 rad/s over the first 0.5 s before and after the change. With compound-branch damping the three bodies reach lasting rest at 0.817 s and the branch's final angle is 0.18° from the downward vertical. A second fixture keeps a distal segment pressed against a frictionless plane and checks that its freely turning vertical hinge retains its motion. [Contact fixtures](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs)

Two 60 s controls ran the same built-in rig through the Jolt adapter. On the parked route at row 8, column 0, Jolt first hit the bonnet with both feet at 395.833 ms and settled at 10.867 s. On the moving route at row 9, column 0, it first hit at 404.167 ms and settled at 11.100 s. The last Jolt contact records in each drop put the chest on the street and the right foot against the bonnet. Rapier settled at 6.633 s and 4.067 s on the same respective inputs. The two adapters can reach different resting poses.

Six alternating CPU pairs on one logical processor timed the public replay, including writing its trace. On the built-in human's parked edge case, median process CPU time fell from 4.77 to 1.28 s for 60 s of simulation. On the imported control with identical poses, median process time was 1.15 s for both builds. Its median physics step across runs rose from 32.8 to 44.3 µs. A separate 552-step control in flight showed no consistent process-level improvement. Earlier sleep accounts for the large saving on the affected drop. Individual pairs are in `bench/out/ville-paris/settling-followup/audit/cpu/result.json`.

## Two feet striking a supported bonnet

In two offset drops of the built-in rig, the first report moved one 240 Hz step earlier after splitting the step at the inward crossing (D-188). Both feet now appear at 395.833 ms on the parked bonnet and at 404.167 ms on the moving bonnet. The table uses the left foot for velocity and all fifteen limbs for penetration. Peak depth covers the same clock interval for all builds, from those instants through 25 ms later. [Supported impact algorithm](/algorithms/supported-box-impact.md)

| Native joint limits and route | Left-foot velocity before | Left-foot velocity after | Peak depth before | Peak depth after | Jolt peak |
| --- | ---: | ---: | ---: | ---: | ---: |
| Parked, row 8 column 0 | +0.313 m/s | +0.0068 m/s | 21.38 mm | 13.48 mm | 7.55 mm |
| Moving, row 9 column 0 | +1.582 m/s | +0.0011 m/s | 19.92 mm | 18.29 mm | 7.60 mm |

In these two runs Jolt reported contact while a positive gap remained. Its closest points were still approaching at 3.62 and 3.18 m/s respectively. The first sampled overlap occurred one boundary later. A separate leaning two-link test shifts the start height across eight phases of the native step, checks a material with positive restitution, and repeats a crash after the whole rig has cleared the support. A streaming regression removes the road page from under a resting box and checks that the box falls after the page is gone. [Impact tests](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/supported_impact.rs) · [Page removal test](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/contacts.rs)

Across all 404 replays, the median absolute normal speed at the first report falls from 0.554 to 0.0033 m/s, and the 95th percentile from 0.749 to 0.0116 m/s. Each replay contributes the largest absolute value on its reported limbs. This improves on 400 replays, ties on two and rises on two. In the two outliers at parked row 9, column 8, the left foot is still 6.8–7.5 mm above the box and approaching at 4.75–4.78 m/s. The right foot has already made contact and is separating at 0.0014–0.0030 m/s. Measuring instead at the first sampled geometric overlap, on the limbs actually touching, gives a median of 0.881 before and 0.220 m/s after. That second measure improves in 241 replays, ties in two and worsens in 161. There is no missed overlap within the captured windows.

Of the 400 offset drops with native rig limits, 247 have a lower peak penetration over four seconds. The largest overall peak rises from 22.46 to 22.58 mm. A limb passes 20 mm on eleven drops, up from eight, and every such excursion ends within two 240 Hz intervals. The largest individual increase is 8.11 to 20.56 mm on the imported rig's parked row 8, column 7. Across those 400 drops and four central controls, 98 first reports advance by one substep and 306 keep their original times. Every first report retains its set of limbs.

Over the eighteen 60 s runs with native limits, seven settle earlier and eleven later. The longest time to lasting rest falls from 11.28 to 7.33 s. The imported rig on moving row 9, column 1 takes 6.25 s instead of 3.35 s. Settling at a given time and maintaining contact depth during the first impact measure different properties.

Nine alternating before/after pairs used the same logical processor for each run, including recording the public CSV. For four 4 s routes in one process, the median paired CPU change was +5.26% on the built-in human and −9.62% on the imported rig. On a 60 s centred parked drop with the built-in rig it was +12.00%. The median step rose from 59.7 to 110.0 µs on that run. The Jolt and flight-only controls retained byte-identical traces and their CPU intervals spanned zero. The group comparison includes the physics, the different trajectories and the trace writer. Individual pairs are in `bench/out/ville-paris/first-response/timing/measured/summary.json`.

On that parked route, all poses of pedestrian and box stayed fixed and all velocities were exactly zero from 20 through 60 s. Nine more paired runs subtracted the cost of recording the 20 s prefix from the cost of recording all 60 s. Across those 2,400 additional quiet ticks the median paired increase was 27.2 µs per tick (nominal 96.1% interval +9.2 to +156.0 µs). The traces contained 88 contact rows per tick before and 80 after. This estimates the marginal cost of a quiet public replay, including CSV serialization. The measurement does not isolate the detector's own cost. Inputs and raw pairs are in `bench/out/ville-paris/first-response/timing/quiet-slope/summary.json`.

## Physics cost after the rig falls asleep

To time the public rigid step, run `ville-paris --physics rapier --actor human --replay worlds/ville-paris/fixtures/bonnet.json --route parked --time-physics-from 120`. The first 120 ticks advance the world. The timer covers each remaining call to the public step. The output includes the initial and final poses and contacts, the count of sampled ticks where all velocities were exactly zero and the total contact rows. Supply a longer replay to inspect a late interval. [City command](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/physics.rs)

The late window is reproducible from the bundled fixture. Set both the route length and its final camera keyframe to 3,600. The timing report goes to standard output.

```sh
jq '.routes = [.routes[] | select(.id == "parked") | .ticks = 3600 | .camera[-1].tick = 3600]' \
  worlds/ville-paris/fixtures/bonnet.json > /tmp/ville-paris-parked-60s.json
target/release/ville-paris --replay /tmp/ville-paris-parked-60s.json \
  --physics rapier --actor human --time-physics-from 1200
```

On the central parked drop extended to 60 s, all fifteen limbs and the bonnet remained motionless throughout the final 2,400 ticks. Nine alternating before/after pairs on one logical CPU gave the following medians. The CPU costs include contact collection in the public step. To measure the marginal cost of the command writing the trace, subtract the 20 s run from the 60 s run in each paired block.

| Cost per 60 Hz tick | D-188 | D-189 |
| --- | ---: | ---: |
| Physics CPU during seconds 0–4 | 1,972 µs | 1,888 µs |
| Physics CPU during seconds 20–60 | 98.1 µs | 24.2 µs |
| Physics elapsed p95 during seconds 20–60 | 110.7 µs | 27.3 µs |
| Marginal CPU with CSV during seconds 20–60 | 360.5 µs | 273.4 µs |

The corresponding median paired CPU savings were 75.7% for physics alone and 25.1% for the recorded command. Both builds produced the same 55,808,945-byte trace. In the independent wake test the two bodies fell on the first tick after removal of their support and landed on the new surface. Across seven observation sequences, 91,750 contact witnesses and 9,558 impulse measurements matched the previous build, including their order. [Sleeping-contact algorithm](/algorithms/resting-contact-witnesses.md) · [Regression fixture](https://github.com/smichea/osage/blob/main/crates/osage-dynamics/tests/resting_contacts.rs)

## Load transfer on offset landings

Drop the integrated and imported pedestrians on 202 release positions and velocities each in the resident city. Measure the deepest capsule overlap with the oriented bonnet at 240 Hz for four seconds. With the D-189 solver the mean of the per-route maxima was 14.37 mm for the integrated rig and 14.53 mm for the imported rig. Refining the loaded interval (D-190) gives 10.50 mm and 11.76 mm. The respective maxima across routes drop from 22.58 to 19.84 mm and from 20.56 to 19.02 mm. The same limbs report first contact at the same configured substep on all 404 runs. Fifty trajectories have over 1 mm more peak overlap. The largest increase is 11.03 mm on the imported rig.

Both versions reached and held rest in all twenty-two 60-second controls. The slowest integrated case takes 10.77 s with refinement, up from a maximum of 7.33 s across those controls. For the imported rig the maxima are 6.07 s and 6.25 s respectively. The city regression follows both feet over the top face in the first 25 ms on two offset drops and requires penetration below 20 mm. It then advances another 15 seconds and checks that the rig and bonnet have remained motionless for at least half a second. [Regression fixture](https://github.com/smichea/osage/blob/main/worlds/ville-paris/src/physics.rs) · [Algorithm](/algorithms/supported-box-impact.md)

In seven alternating paired CPU measurements of the production physics on the resident sector, the median cost per public tick during the first four seconds rose from 1.88 to 1.98 ms on the central drop and from 1.26 to 1.41 ms on the offset drop at (0.125, 10.425, 6.75) m. Between 20 and 60 seconds the corresponding values were 24.3 to 29.4 µs on the centre and 25.5 to 23.8 µs on the offset. The driver calls the public physics step without serializing poses.

# Used by

[Ville-Paris](/modules/ville-paris.md) and [osage-dynamics](/modules/osage-dynamics.md).
