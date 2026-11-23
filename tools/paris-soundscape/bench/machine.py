# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""What the two baselines share: the machine's description, the choice of one
core, CPU time read from /proc, a table and a receipt.

A cost in nanoseconds means nothing without the state of the machine that
gave it: the same laptop is three times slower with turbo off. So every
receipt carries that state, read before and after the measurement.
"""

import datetime
import fcntl
import json
import os
import pathlib
import platform
import statistics
import time

HERE = pathlib.Path(__file__).resolve().parent
# `out/` of the prototype is ignored by git: receipts are made again, not kept.
OUT = HERE.parent / "out" / "bench"

RATE = 48_000
SECONDS = 10.0
COUNTS = (1, 8, 32, 64, 128, 256)
# The scene both baselines play, so that their figures can be set side by
# side: sources evenly spread on a circle, all turning at the same speed.
CIRCLE_RADIUS = 10.0  # m
SOURCE_SPEED = 10.0  # m/s
# The listener stands away from the centre. At the centre, every source would
# keep its distance and have no speed towards the listener: neither the
# distance law nor Doppler would be exercised. Here the distance goes from
# 4 m to 16 m and the closing speed reaches 6 m/s.
LISTENER_OFFSET = 6.0  # m, along +x
BLOCK = 1024  # frames between two updates of the positions


def read(path):
    """The text of a small file of /proc or /sys, or None."""
    try:
        return pathlib.Path(path).read_text().strip()
    except OSError:
        return None


def cpu_list(text):
    """'0-3,8' as a list of CPU numbers."""
    cpus = []
    for part in (text or "").split(","):
        part = part.strip()
        if not part:
            continue
        low, _, high = part.partition("-")
        cpus.extend(range(int(low), int(high or low) + 1))
    return cpus


def cpu_model():
    for line in (read("/proc/cpuinfo") or "").splitlines():
        if line.startswith("model name"):
            return line.split(":", 1)[1].strip()
    return platform.processor() or "unknown"


def cpu_state(cpu):
    """What sets the speed of one core right now. Every field may be None."""
    base = f"/sys/devices/system/cpu/cpu{cpu}/cpufreq/"

    def khz(name):
        text = read(base + name)
        return int(text) if text and text.isdigit() else None

    kind = None
    if cpu in cpu_list(read("/sys/devices/cpu_core/cpus")):
        kind = "performance"
    elif cpu in cpu_list(read("/sys/devices/cpu_atom/cpus")):
        kind = "efficiency"
    return {
        "cpu": cpu,
        "kind": kind,
        "governor": read(base + "scaling_governor"),
        "energy_performance_preference": read(base + "energy_performance_preference"),
        "current_khz": khz("scaling_cur_freq"),
        "max_khz": khz("scaling_max_freq"),
        "hardware_max_khz": khz("cpuinfo_max_freq"),
        "siblings": cpu_list(read(f"/sys/devices/system/cpu/cpu{cpu}/topology/core_cpus_list")),
    }


def machine(cpu):
    no_turbo = read("/sys/devices/system/cpu/intel_pstate/no_turbo")
    return {
        "cpu_model": cpu_model(),
        "logical_cpus": os.cpu_count(),
        "kernel": platform.release(),
        "python": platform.python_version(),
        "turbo_disabled": None if no_turbo is None else no_turbo == "1",
        "load_average": list(os.getloadavg()),
        "pinned": None if cpu is None else cpu_state(cpu),
    }


def _ticks():
    """Per logical CPU, the hundredths of a second it was (busy, idle) since boot."""
    ticks = {}
    for line in (read("/proc/stat") or "").splitlines():
        fields = line.split()
        if fields and fields[0].startswith("cpu") and fields[0][3:].isdigit():
            # user nice system idle iowait irq softirq steal
            values = [int(field) for field in fields[1:9]]
            idle = values[3] + values[4]
            ticks[int(fields[0][3:])] = (sum(values) - idle, idle)
    return ticks


def _idle_ticks():
    return {cpu: idle for cpu, (_, idle) in _ticks().items()}


def watch_siblings(cpu):
    """Start watching the hyper-thread siblings of `cpu`; the function returned
    gives the share of the time since then that they were busy, from 0 to 1.

    Two threads of one core share its arithmetic units and its caches, so a
    busy sibling makes every figure worse without showing in the CPU time's
    own account. The share is None when the core has no sibling or when the
    process is not pinned.
    """
    siblings = [] if cpu is None else [s for s in cpu_state(cpu)["siblings"] if s != cpu]
    start = _ticks()

    def share():
        end = _ticks()
        busy = sum(end[s][0] - start[s][0] for s in siblings if s in start and s in end)
        idle = sum(end[s][1] - start[s][1] for s in siblings if s in start and s in end)
        return busy / (busy + idle) if busy + idle else None

    return share


def _lock(name, core):
    """Take the lock of one physical core, or None when another baseline holds it."""
    OUT.mkdir(parents=True, exist_ok=True)
    lock = open(OUT / f".measuring-core-{core}", "a+")  # noqa: SIM115 (kept open on purpose)
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except OSError:
        lock.close()
        return None
    lock.seek(0)
    lock.truncate()
    lock.write(f"{name}, pid {os.getpid()}\n")
    lock.flush()
    return lock


def claim(name, request="auto"):
    """Pin this process, and every process it starts, to one logical CPU that
    no other baseline is measuring on. Returns (cpu, lock); keep the lock.

    'auto' takes, among the performance cores when the processor has two
    kinds, the one that was idlest over a third of a second, counting its
    hyper-thread sibling: a busy sibling slows a core by a third or more.
    Two baselines on one physical core would each count the other's
    disturbance as the engine's cost, so a core is locked, through a file of
    the receipts' directory, until the process that measures on it ends.
    The CPU is None when pinning is not asked or not possible.
    """
    if request == "none" or not hasattr(os, "sched_setaffinity"):
        return None, None
    allowed = sorted(os.sched_getaffinity(0))
    if request != "auto":
        candidates = [int(request)]
        if candidates[0] not in allowed:
            raise SystemExit(f"{name}: CPU {request} is not among those this process may use: {allowed}")
    else:
        performance = [c for c in cpu_list(read("/sys/devices/cpu_core/cpus")) if c in allowed]
        before = _idle_ticks()
        time.sleep(0.35)
        after = _idle_ticks()

        def idleness(c):
            group = cpu_state(c)["siblings"] or [c]
            return sum(after.get(s, 0) - before.get(s, 0) for s in group) / len(group)

        candidates = sorted(performance or allowed, key=idleness, reverse=True)
    for cpu in candidates:
        lock = _lock(name, min(cpu_state(cpu)["siblings"] or [cpu]))
        if lock:
            os.sched_setaffinity(0, {cpu})
            return cpu, lock
    raise SystemExit(f"{name}: other baselines are being measured on every core asked for ({candidates}); wait, or pass --cpu")


def tree(root):
    """The process `root` and all its descendants, as pids."""
    children = {}
    for entry in os.listdir("/proc"):
        if entry.isdigit():
            stat = read(f"/proc/{entry}/stat")
            if stat:
                # The name may hold spaces and brackets: the parent follows the last ')'.
                parent = int(stat.rsplit(")", 1)[1].split()[1])
                children.setdefault(parent, []).append(int(entry))
    pids, queue = [], [root]
    while queue:
        pid = queue.pop()
        pids.append(pid)
        queue.extend(children.get(pid, []))
    return pids


def threads_cpu_ns(pids):
    """Time on a CPU of every thread of these processes: {(pid, tid): (ns, name)}.

    From the scheduler's own account (/proc/PID/task/TID/schedstat), in
    nanoseconds, where /proc/PID/stat only counts hundredths of a second.
    """
    times = {}
    for pid in pids:
        try:
            tids = os.listdir(f"/proc/{pid}/task")
        except OSError:
            continue
        for tid in tids:
            stat = read(f"/proc/{pid}/task/{tid}/schedstat")
            if stat:
                times[(pid, int(tid))] = (int(stat.split()[0]), read(f"/proc/{pid}/task/{tid}/comm") or "?")
    return times


def spread(values):
    """Median, smallest and largest of a few measurements."""
    return {"median": statistics.median(values), "min": min(values), "max": max(values)}


def fit_line(points):
    """Least squares y = a + b x over (x, y) points; (a, b)."""
    n = len(points)
    mean_x = sum(x for x, _ in points) / n
    mean_y = sum(y for _, y in points) / n
    spread_x = sum((x - mean_x) ** 2 for x, _ in points)
    if spread_x == 0.0:
        return mean_y, 0.0
    slope = sum((x - mean_x) * (y - mean_y) for x, y in points) / spread_x
    return mean_y - slope * mean_x, slope


def realtime_limit(points, limit, measure):
    """The largest N whose second of sound costs less than a second of one core.

    `points` are (N, milliseconds of CPU per second of sound) already
    measured. A line through them gives a first guess; `measure(n)` then
    measures the guess, and corrects it once or twice, since the cost per
    source rises when the sources no longer fit the caches. `limit` is the
    most sources the engine grants, or None.
    """
    intercept, slope = fit_line(points)
    if slope <= 0.0:
        return {"note": "the cost does not grow with N: no limit can be drawn from it"}
    guess = int((1000.0 - intercept) / slope)
    found = {"line_ms_per_s": {"at_zero": intercept, "per_source": slope}, "extrapolated_n": guess, "probes": []}
    under = max(((n, ms) for n, ms in points if ms < 1000.0), default=None)
    over = min(((n, ms) for n, ms in points if ms >= 1000.0), default=None)
    n = guess
    for _ in range(3):
        if limit is not None and n > limit:
            found["note"] = f"beyond the {limit} sources the engine grants: not measured"
            break
        if n < 1 or (under and over and over[0] - under[0] <= max(1, under[0] // 50)):
            break
        ms = measure(n)
        found["probes"].append({"n": n, "cpu_ms_per_s": ms})
        if ms < 1000.0:
            under = max(under, (n, ms)) if under else (n, ms)
        else:
            over = min(over, (n, ms)) if over else (n, ms)
        if under and over and under[0] < over[0] - 1:
            # Between the two, the cost is close to a line.
            n = int(under[0] + (over[0] - under[0]) * (1000.0 - under[1]) / (over[1] - under[1]))
            n = min(max(n, under[0] + 1), over[0] - 1)
        elif under and over:
            break
        else:
            n = int(n * 1000.0 / ms * (0.97 if ms >= 1000.0 else 1.03))
    found["largest_measured_under_real_time"] = {"n": under[0], "cpu_ms_per_s": under[1]} if under else None
    found["smallest_measured_over_real_time"] = {"n": over[0], "cpu_ms_per_s": over[1]} if over else None
    return found


def real_time_sentence(name, limit):
    """What `realtime_limit` found, as one line."""
    under, over = limit.get("largest_measured_under_real_time"), limit.get("smallest_measured_over_real_time")
    line = f"real time on one core, {name}: the line through the points gives N = {limit.get('extrapolated_n')}"
    if under:
        line += f"; measured under: N = {under['n']} ({under['cpu_ms_per_s']:.0f} ms/s)"
    if over:
        line += f"; measured over: N = {over['n']} ({over['cpu_ms_per_s']:.0f} ms/s)"
    if limit.get("note"):
        line += f"; {limit['note']}"
    return line


def table(headers, rows):
    """A table a person reads in a terminal: text left, numbers right."""
    cells = [[str(h) for h in headers]] + [[cell if isinstance(cell, str) else number(cell) for cell in row] for row in rows]
    widths = [max(len(row[i]) for row in cells) for i in range(len(headers))]
    lines = []
    for index, row in enumerate(cells):
        text = [c.ljust(w) if isinstance(src, str) else c.rjust(w) for c, w, src in zip(row, widths, (rows[index - 1] if index else headers))]
        lines.append("  ".join(text).rstrip())
        if index == 0:
            lines.append("  ".join("-" * w for w in widths))
    return "\n".join(lines)


def number(value):
    if value is None:
        return "-"
    if isinstance(value, int):
        return str(value)
    if abs(value) >= 100:
        return f"{value:.0f}"
    if abs(value) >= 10:
        return f"{value:.1f}"
    return f"{value:.2f}"


def percent(share):
    return "-" if share is None else f"{100 * share:.0f} %"


def load_sentence(receipt):
    """How disturbed the measurement was, in one line."""
    before, after = receipt["machine_before"], receipt["machine_after"]
    text = f"{before['load_average'][0]:.1f} before, {after['load_average'][0]:.1f} after (1 min average, all cores)"
    if receipt.get("sibling_busy_share") is not None:
        text += f"; the hyper-thread sibling of the pinned core was busy {percent(receipt['sibling_busy_share'])} of the time"
    return text


def write_receipt(path, receipt):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(receipt, indent=1) + "\n")
    return path


def now():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")
