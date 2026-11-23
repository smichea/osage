#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""What the Web Audio API of Chromium costs per source on this machine: a
second baseline for the prototype, since browser games build on it.

    python3 bench/web_audio.py [--out FILE] [--browser PATH] [--quick]

A headless browser loads `web_audio.html`, which plays N looping noises
through PannerNode in an OfflineAudioContext and times `startRendering()`
with `performance.now()`. The scene is the one of `openal_soft.py`.

The page's own clock is not taken on trust. The page writes a line to the
console before and after each render and rests in between; this program
reads those lines from the browser's log, times them with its own clock, and
asks the kernel for the CPU time of every process of the browser at both
moments. So each render has three figures: the time the page saw, the time
seen from outside, and the CPU time spent. No virtual time is used:
`--virtual-time-budget` would falsify all three.

Chromium renders an offline context on a thread it starts for that render
and ends with it, so the thread's own account is gone when the page says it
is done. The CPU time of the render is therefore what the page's process
spent, less what its threads still alive spent (the page's script among
them): what is left is the render thread, and any other thread that ended
meanwhile, so the figure errs on the high side if at all.

The whole browser is pinned to one core. Standard library only.
"""

import sys

sys.dont_write_bytecode = True

import argparse  # noqa: E402
import base64  # noqa: E402
import ctypes  # noqa: E402
import json  # noqa: E402
import os  # noqa: E402
import pathlib  # noqa: E402
import re  # noqa: E402
import selectors  # noqa: E402
import shutil  # noqa: E402
import signal  # noqa: E402
import subprocess  # noqa: E402
import tempfile  # noqa: E402
import time  # noqa: E402
import urllib.parse  # noqa: E402

import machine  # noqa: E402

PAGE = machine.HERE / "web_audio.html"
BROWSERS = ("chromium", "google-chrome-stable", "google-chrome", "chromium-browser")
# The page rests this long before and after each render (PAUSE_MS there).
PAUSE_MS = 300.0
MARK = re.compile(rb"PARIS-BENCH ([A-Za-z0-9+/=]+)")
# Nothing here needs the network, and a benchmark must not be disturbed by a
# browser fetching its updates.
FLAGS = (
    "--headless",
    "--disable-gpu",
    "--mute-audio",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--disable-component-update",
    "--disable-background-networking",
    "--disable-sync",
    "--disable-default-apps",
    "--enable-logging=stderr",
    "--log-level=0",
)


class Failed(Exception):
    """The browser is missing, or did not do what the measurement needs."""


def find_browser(asked):
    for name in [asked] if asked else BROWSERS:
        path = shutil.which(name)
        if path:
            return path
    raise Failed(
        f"no browser found ({asked or ', '.join(BROWSERS)}): install Chromium or pass --browser"
        if not asked
        else f"{asked} is not a program: pass the path of Chromium or Chrome to --browser"
    )


def version(browser):
    try:
        done = subprocess.run([browser, "--version"], capture_output=True, text=True, timeout=60, check=False)
    except (OSError, subprocess.TimeoutExpired) as error:
        raise Failed(f"{browser} --version failed: {error}") from error
    return done.stdout.strip() or done.stderr.strip()


def peak_rss_kb(pids):
    """The sum of the peaks of resident memory of these processes."""
    total = 0
    for pid in pids:
        for line in (machine.read(f"/proc/{pid}/status") or "").splitlines():
            if line.startswith("VmHWM:"):
                total += int(line.split()[1])
    return total


_libc = ctypes.CDLL(None, use_errno=True)


def process_cpu_ns(pid):
    """CPU time of a whole process, its ended threads included, or None.

    /proc gives this in hundredths of a second only; the process's CPU clock
    (clock_getcpuclockid) gives it in nanoseconds.
    """
    clock = ctypes.c_int(0)
    try:
        if _libc.clock_getcpuclockid(ctypes.c_int(pid), ctypes.byref(clock)) != 0:
            return None
        return time.clock_gettime_ns(clock.value)
    except (AttributeError, OSError):
        return None


def snapshot(root):
    """The CPU time, now, of every process of the browser and of every thread alive.

    A process's time and its threads' times are read one after the other. If
    the process ran in between, the two do not add up, and taking one from
    the other gives nonsense for a short render. The page rests when this is
    called, so the reading is made again until the process's clock stood
    still around it; `unsettled_ns` tells by how much it did not.
    """
    processes, threads, unsettled = {}, {}, 0
    for pid in machine.tree(root):
        for _ in range(50):
            before = process_cpu_ns(pid)
            own = machine.threads_cpu_ns([pid])
            after = process_cpu_ns(pid)
            if before == after:
                break
            time.sleep(0.002)
        if after is not None:
            processes[pid] = after
            unsettled += after - (before or 0)
        threads.update(own)
    return {"processes": processes, "threads": threads, "unsettled_ns": unsettled}


def kind_of(pid):
    """'renderer', 'gpu-process', ... or 'browser' for the first process."""
    command = (machine.read(f"/proc/{pid}/cmdline") or "").replace("\0", " ")
    return command.split("--type=")[1].split()[0] if "--type=" in command else "browser"


def spent(before, after):
    """What each process of the browser did between two snapshots.

    Chromium's render thread is born after the first snapshot and has ended
    before the second. So what a process spent, less what its threads that
    lived through both snapshots spent, is the time of the threads born or
    ended meanwhile: in the page's process, the render thread.
    """
    report = {}
    for pid, ns in after["processes"].items():
        total = ns - before["processes"].get(pid, 0)
        lasting = main = 0
        for (owner, tid), (thread_ns, _) in after["threads"].items():
            if owner == pid and (owner, tid) in before["threads"]:
                delta = thread_ns - before["threads"][(owner, tid)][0]
                lasting += delta
                if tid == pid:
                    main = delta
        names = lambda one, other, pid=pid: sorted({name for key, (_, name) in one.items() if key[0] == pid and key not in other})  # noqa: E731
        report[pid] = {
            "page_process_cpu_ms": total / 1e6,
            "page_main_thread_cpu_ms": main / 1e6,
            "render_cpu_ms": (total - lasting) / 1e6,
            "threads_born_meanwhile_and_alive": names(after["threads"], before["threads"]),
            "threads_ended_meanwhile": names(before["threads"], after["threads"]),
        }
    if not report:
        raise Failed("the kernel gave no CPU clock for the browser's processes: nothing can be measured")
    return {
        "processes": report,
        "tree_cpu_ms": sum(process["page_process_cpu_ms"] for process in report.values()),
        "accounting_error_ms": (before["unsettled_ns"] + after["unsettled_ns"]) / 1e6,
    }


def attribute(runs):
    """Find the page's process, and keep its figures in each run.

    The browser runs a dozen processes and nothing names the one that holds
    the page. It is the one where, over all the renders, short-lived threads
    spent the most time.
    """
    totals = {}
    for run in runs:
        for pid, process in run["processes"].items():
            totals[pid] = totals.get(pid, 0.0) + process["render_cpu_ms"]
    page = max(totals, key=totals.get)
    for run in runs:
        process = run.pop("processes").get(page)
        if process is None:
            raise Failed("the page's process was not there for every render")
        run.update(process, page_process=kind_of(page))
    return runs


def run_case(browser, case, timeout):
    """One page load in a fresh browser with a fresh profile; the page's events."""
    profile = tempfile.mkdtemp(prefix="paris-web-audio-")
    url = PAGE.as_uri() + "?" + urllib.parse.urlencode(case)
    command = [browser, *FLAGS, f"--user-data-dir={profile}", url]
    # A distribution's launcher may add the user's own flags and extensions
    # from a file of the configuration directory: it finds none in this one.
    environment = dict(os.environ, XDG_CONFIG_HOME=profile)
    process = subprocess.Popen(command, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, start_new_session=True, env=environment)
    events, pending, runs = [], b"", {}
    begun, last_error, command_line = None, "", None
    selector = selectors.DefaultSelector()
    selector.register(process.stderr, selectors.EVENT_READ)
    deadline = time.monotonic() + timeout
    try:
        while True:
            if not selector.select(max(0.0, deadline - time.monotonic())):
                raise Failed(f"the page did not finish in {timeout:.0f} s; the browser last said: {last_error or 'nothing'}")
            chunk = os.read(process.stderr.fileno(), 65536)
            if not chunk:
                raise Failed(f"the browser exited before the page finished; it last said: {last_error or 'nothing'}")
            arrived = time.perf_counter_ns()
            pending += chunk
            *lines, pending = pending.split(b"\n")
            for line in lines:
                found = MARK.search(line)
                if not found:
                    if b"ERROR" in line or b"FATAL" in line:
                        last_error = line.decode("utf-8", "replace").strip()[-300:]
                    continue
                event = json.loads(base64.b64decode(found.group(1)))
                events.append(event)
                kind = event.get("event")
                if kind == "error":
                    raise Failed(f"the page failed: {event.get('message')}")
                if kind == "ready":
                    command_line = (machine.read(f"/proc/{process.pid}/cmdline") or "").split("\0")
                elif kind == "begin":
                    # The page rests now: the times are read at rest.
                    begun = (arrived, snapshot(process.pid))
                elif kind == "end" and begun:
                    event.update(spent(begun[1], snapshot(process.pid)), outside_ms=(arrived - begun[0]) / 1e6)
                    runs[event["run"]] = event
                    begun = None
                elif kind == "done":
                    pids = machine.tree(process.pid)
                    return {
                        "ready": next((e for e in events if e.get("event") == "ready"), None),
                        "runs": attribute([runs[key] for key in sorted(runs)]),
                        "processes": len(pids),
                        "peak_rss_kb": peak_rss_kb(pids),
                        "command_line": [part for part in command_line or [] if part and not part.startswith(("file:", "--user-data-dir"))],
                    }
    finally:
        selector.close()
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            process.wait(10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(10)
        process.stderr.close()
        shutil.rmtree(profile, ignore_errors=True)


def summarise(case, result):
    """The figures of one case. Run 0 was the warm-up and is left out."""
    runs = [run for run in result["runs"] if run["run"] > 0]
    if not runs:
        raise Failed("the page reported no measured run")
    n, seconds = case["n"], case["seconds"]
    per_second = lambda key: machine.spread([run[key] / seconds for run in runs])  # noqa: E731
    inside, render = per_second("render_ms"), per_second("render_cpu_ms")
    # Outside, a render is seen between two console lines: the page's rest
    # and the building of the graph are taken off before comparing.
    outside = machine.spread([(run["outside_ms"] - PAUSE_MS - run["build_ms"]) / seconds for run in runs])
    silent = [run["run"] for run in runs if min(run["rms"]) <= 0.0]
    if silent:
        raise Failed(f"runs {silent} rendered silence: nothing was measured")
    expected = round(seconds * machine.RATE)
    if any(run["frames"] != expected for run in runs):
        raise Failed(f"the page rendered {runs[0]['frames']} frames where {expected} were asked")
    return {
        "n": n,
        "measured_runs": len(runs),
        "render_cpu_ms_per_s": render,
        "inside_wall_ms_per_s": inside,
        "outside_wall_ms_per_s": outside,
        "outside_over_inside": outside["median"] / inside["median"] if inside["median"] else None,
        "ns_per_source_sample": render["median"] * 1e6 / (n * machine.RATE),
        # The page's own thread: building the graph, moving the sources when
        # the page does it block by block, reading the level of the result.
        "page_main_thread_cpu_ms_per_s": per_second("page_main_thread_cpu_ms")["median"],
        # The time the page saw itself spend moving the sources.
        "update_ms_per_s": per_second("update_ms")["median"],
        "updates_per_run": runs[-1]["updates"],
        # Every process of the browser, between the two console lines.
        "tree_cpu_ms_per_s": per_second("tree_cpu_ms")["median"],
        "build_ms": machine.spread([run["build_ms"] for run in runs])["median"],
        "page_process": sorted({run["page_process"] for run in runs}),
        "threads_born_meanwhile_and_alive": sorted({name for run in runs for name in run["threads_born_meanwhile_and_alive"]}),
        "threads_ended_meanwhile": sorted({name for run in runs for name in run["threads_ended_meanwhile"]}),
        "peak_rss_kb": result["peak_rss_kb"],
        "processes": result["processes"],
        "rms": runs[-1]["rms"],
        "runs": result["runs"],
    }


def line(results):
    """The cost as a fixed part and a part per source."""
    if len(results) < 2:
        return None
    fixed, per_source = machine.fit_line([(r["n"], r["render_cpu_ms_per_s"]["median"]) for r in results])
    return {"fixed_ms_per_s": fixed, "per_source_ms_per_s": per_source, "ns_per_added_source_sample": per_source * 1e6 / machine.RATE}


def report(receipt):
    before, after = receipt["machine_before"], receipt["machine_after"]
    pinned = before["pinned"]
    print(f"Web Audio baseline, {receipt['date']}")
    print(f"  browser    {receipt['browser']}: {receipt['version']}")
    print(f"  page says  {receipt.get('user_agent')}")
    print(f"  started as {' '.join(receipt.get('command_line') or [])}")
    print(f"  machine    {before['cpu_model']}, kernel {before['kernel']}, Python {before['python']}")
    if pinned:
        mhz = [state["pinned"]["current_khz"] for state in (before, after)]
        print(
            f"  core       whole browser pinned to CPU {pinned['cpu']} ({pinned['kind'] or 'kind unknown'}), governor {pinned['governor']},"
            f" preference {pinned['energy_performance_preference']}, turbo {'off' if before['turbo_disabled'] else 'on'},"
            f" {(mhz[0] or 0) / 1000:.0f} MHz before, {(mhz[1] or 0) / 1000:.0f} MHz after"
            f" (limit {(pinned['max_khz'] or 0) / 1000:.0f} MHz)"
        )
    else:
        print("  core       not pinned")
    print(f"  load       {machine.load_sentence(receipt)}")
    headers = ["case", "N", "runs", "render CPU ms/s", "min", "max", "page wall ms/s", "outside/page", "ns/source-sample"]
    headers += ["page thread ms/s", "browser ms/s", "peak RSS MB", "sibling busy"]
    rows = []
    for variant in receipt["variants"]:
        if "skipped" in variant:
            rows.append([variant["name"], "skipped: " + variant["skipped"]] + [""] * (len(headers) - 2))
            continue
        for result in variant["results"]:
            cpu = result["render_cpu_ms_per_s"]
            rows.append(
                [
                    variant["name"],
                    result["n"],
                    result["measured_runs"],
                    cpu["median"],
                    cpu["min"],
                    cpu["max"],
                    result["inside_wall_ms_per_s"]["median"],
                    result["outside_over_inside"],
                    result["ns_per_source_sample"],
                    result["page_main_thread_cpu_ms_per_s"],
                    result["tree_cpu_ms_per_s"],
                    result["peak_rss_kb"] / 1024,
                    machine.percent(result.get("sibling_busy_share")),
                ]
            )
    print()
    print(machine.table(headers, rows))
    print()
    print("The cost as a line through these points: a fixed part, and a part per source.")
    fits = [
        [variant["name"], variant["line"]["fixed_ms_per_s"], variant["line"]["per_source_ms_per_s"], variant["line"]["ns_per_added_source_sample"]]
        for variant in receipt["variants"]
        if variant.get("line")
    ]
    print(machine.table(["case", "fixed CPU ms/s", "CPU ms/s per source", "ns per added source-sample"], fits))
    print()
    for key, words in (("threads_born_meanwhile_and_alive", "born during a measured render and still alive after it"), ("threads_ended_meanwhile", "ended during one")):
        found = sorted({name for variant in receipt["variants"] for result in variant.get("results", []) for name in result[key]})
        print(f"threads of the page's process {words}, all counted in the render: {', '.join(found) or 'none seen'}")
    for variant in receipt["variants"]:
        if variant.get("real_time"):
            print(machine.real_time_sentence(variant["name"], variant["real_time"]))


# The first two are the baseline; the others show what each part costs, and
# that no other way of moving a source is cheaper. Their last field limits
# them to the first numbers of sources: these cases cost more than real time
# from a few dozen sources on, and a line needs three points.
VARIANTS = (
    ("HRTF, moved every block, Doppler by playbackRate", {"model": "HRTF", "motion": "steps", "doppler": 1}, None),
    ("equalpower, moved every block, Doppler by playbackRate", {"model": "equalpower", "motion": "steps", "doppler": 1}, None),
    ("HRTF, at rest", {"model": "HRTF", "motion": "none", "doppler": 0}, None),
    ("equalpower, at rest", {"model": "equalpower", "motion": "none", "doppler": 0}, None),
    ("HRTF, moved every block, no Doppler", {"model": "HRTF", "motion": "steps", "doppler": 0}, 3),
    ("equalpower, moved every block, no Doppler", {"model": "equalpower", "motion": "steps", "doppler": 0}, 3),
    ("HRTF, moved every block by setPosition, Doppler", {"model": "HRTF", "motion": "legacy", "doppler": 1}, 3),
    ("HRTF, k-rate lines, Doppler", {"model": "HRTF", "motion": "krate", "doppler": 1}, 3),
    ("HRTF, a-rate lines, Doppler", {"model": "HRTF", "motion": "ramps", "doppler": 1}, 3),
    ("equalpower, a-rate lines, Doppler", {"model": "equalpower", "motion": "ramps", "doppler": 1}, 3),
)


def main():
    parser = argparse.ArgumentParser(description="Cost per source of Chromium's Web Audio API, rendered offline.")
    parser.add_argument("--out", default=str(machine.OUT / "web_audio.json"), help="where the JSON receipt goes")
    parser.add_argument("--browser", help="Chromium or Chrome (default: the first of " + ", ".join(BROWSERS) + ")")
    parser.add_argument("--counts", type=lambda s: [int(n) for n in s.split(",")], default=list(machine.COUNTS), help="numbers of sources")
    parser.add_argument("--seconds", type=float, default=machine.SECONDS, help="sound rendered per run")
    parser.add_argument("--block", type=int, default=machine.BLOCK, help="frames between two positions of a source")
    parser.add_argument("--runs", type=int, default=5, help="measured runs per case, after one warm-up")
    parser.add_argument("--cpu", default="auto", help="the logical CPU to pin to, 'auto' or 'none'")
    parser.add_argument("--quick", action="store_true", help="the two main cases only, without the search for the limit of real time")
    arguments = parser.parse_args()
    try:
        browser = find_browser(arguments.browser)
        if not PAGE.exists():
            raise Failed(f"{PAGE} is missing: it is the page this program loads")
        if arguments.block % 128:
            raise Failed("--block must be a multiple of 128 frames: Web Audio renders by 128")
        receipt = {"what": "Web Audio API of Chromium, cost per source in an OfflineAudioContext", "browser": browser, "version": version(browser)}
    except Failed as error:
        raise SystemExit(f"web_audio.py: {error}") from error
    cpu, lock = machine.claim("web_audio.py", arguments.cpu)  # noqa: F841 (the lock is held until the end)
    receipt.update(date=machine.now(), machine_before=machine.machine(cpu), flags=list(FLAGS), variants=[])
    whole_sibling = machine.watch_siblings(cpu)
    common = {"seconds": arguments.seconds, "runs": arguments.runs, "block": arguments.block}
    common.update(radius=machine.CIRCLE_RADIUS, speed=machine.SOURCE_SPEED, offset=machine.LISTENER_OFFSET)

    def measure(case, change=None):
        case = dict(common, **case, **(change or {}))
        print(f"  {case['n']:5d} sources ...", end="", file=sys.stderr, flush=True)
        sibling = machine.watch_siblings(cpu)
        # A render slower than thirty times real time is not a result.
        result = run_case(browser, case, 60.0 + 30.0 * case["seconds"] * (case["runs"] + 1))
        summary = summarise(case, result)
        summary["sibling_busy_share"] = sibling()
        receipt.setdefault("user_agent", (result["ready"] or {}).get("user_agent"))
        receipt.setdefault("command_line", result["command_line"])
        print(f" {summary['render_cpu_ms_per_s']['median']:8.2f} ms of CPU per second", file=sys.stderr, flush=True)
        return summary

    failure = None
    for index, (name, case, limit) in enumerate(VARIANTS[:2] if arguments.quick else VARIANTS):
        print(name, file=sys.stderr, flush=True)
        try:
            results = [measure(dict(case, n=n)) for n in arguments.counts[:limit]]
        except Failed as error:
            print(f"  skipped: {error}", file=sys.stderr, flush=True)
            receipt["variants"].append({"name": name, "case": case, "skipped": str(error)})
            failure = failure or str(error)
            continue
        variant = {"name": name, "case": case, "line": line(results), "results": results}
        receipt["variants"].append(variant)
        if not arguments.quick and index < 2:
            print(f"{name}: the limit of real time", file=sys.stderr, flush=True)
            few = {"runs": min(3, arguments.runs)}
            try:
                variant["real_time"] = machine.realtime_limit(
                    [(r["n"], r["render_cpu_ms_per_s"]["median"]) for r in results],
                    None,
                    lambda n, case=case: measure(dict(case, n=n), few)["render_cpu_ms_per_s"]["median"],
                )
            except Failed as error:
                print(f"  stopped: {error}", file=sys.stderr, flush=True)
                variant["real_time_stopped"] = str(error)
    receipt["machine_after"] = machine.machine(cpu)
    receipt["sibling_busy_share"] = whole_sibling()
    if not any("results" in variant for variant in receipt["variants"]):
        raise SystemExit(f"web_audio.py: nothing could be measured: {failure}")
    report(receipt)
    path = machine.write_receipt(arguments.out, receipt)
    print(f"\nreceipt: {path}")


if __name__ == "__main__":
    main()
