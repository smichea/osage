#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Summarises a Chrome trace of the bench (Bevy's `trace_chrome`) on the
bench machine itself, where the trace is too large to bring back (D-126):
every system by its mean, 95th percentile and worst time, and, for the
frames whose main world took longest, the systems that took longest in
them.

    trace-summary.py TRACE.json > summary.txt
"""

import collections
import json
import sys


def spans(path):
    """Yields (name, thread, start µs, duration ms) for each span closed."""
    stacks = collections.defaultdict(list)
    with open(path) as trace:
        for line in trace:
            line = line.strip().rstrip(",")
            if not line.startswith("{"):
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError:
                continue
            phase = event.get("ph")
            if phase == "B":
                stacks[event["tid"]].append((event["name"], event["ts"]))
            elif phase == "E" and stacks[event["tid"]]:
                name, start = stacks[event["tid"]].pop()
                yield name, event["tid"], start, (event["ts"] - start) / 1000.0


def main():
    updates = []
    systems = collections.defaultdict(list)
    for name, _, start, ms in spans(sys.argv[1]):
        if name == "update: ":
            updates.append((start, ms))
        elif name.startswith("system: name=") or name.startswith("schedule: name="):
            systems[name].append((start, ms))
    updates.sort()
    # The race's frames: after the loading ones.
    steady = updates[len(updates) // 8 :]
    print(f"frames: {len(updates)}; main world (update) mean {sum(m for _, m in steady) / max(len(steady), 1):.2f} ms")
    rows = []
    for name, runs in systems.items():
        late = sorted(ms for start, ms in runs if steady and start >= steady[0][0])
        if len(late) < len(steady) // 2:
            continue
        rows.append((sum(late) / len(late), late[int(len(late) * 0.95)], late[-1], name))
    rows.sort(reverse=True)
    print("\nmean    p95     worst   system or schedule (race frames)")
    for mean, p95, worst, name in rows[:40]:
        print(f"{mean:6.2f} {p95:7.2f} {worst:8.2f}  {name[:120]}")
    print("\nthe slowest main-world frames, and what took longest in them:")
    for start, ms in sorted(steady, key=lambda u: -u[1])[:6]:
        end = start + ms * 1000.0
        inside = sorted(
            ((d, n) for n, runs in systems.items() for s, d in runs if start <= s < end and n.startswith("system")),
            reverse=True,
        )[:8]
        print(f"  frame at {start / 1e6:8.3f} s: {ms:7.2f} ms")
        for d, n in inside:
            print(f"      {d:7.2f}  {n[14:120]}")


if __name__ == "__main__":
    main()
