#!/usr/bin/env python3
# SPDX-FileCopyrightText: The osage contributors
# SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

"""What OpenAL Soft costs per source on this machine: a baseline for the
prototype's own cost per source.

    python3 bench/openal_soft.py [--out FILE] [--quick]

OpenAL Soft is driven through ctypes and renders into memory through its
loopback device (ALC_SOFT_loopback), so no sound card, no audio thread and no
clock take part: the mixing happens inside `alcRenderSamplesSOFT`, in the
thread that calls it, and only that call is timed.

The scene: N mono sources, each looping its own second of seeded noise,
evenly spread on a circle of 10 m and turning at 10 m/s. The listener stands
6 m off the centre, so that distance (4 m to 16 m) and Doppler (up to 6 m/s
of closing speed) both change all the time. Positions and velocities are set
every 1024 frames. Ten seconds are rendered to stereo float at 48 kHz, six
times; the first time is a warm-up, the median of the five others is kept.

Every case runs in a process of its own, pinned to one core, so that a case
neither inherits the memory nor the caches of the one before. Standard
library only.
"""

import sys

sys.dont_write_bytecode = True

import argparse  # noqa: E402
import array  # noqa: E402
import ctypes  # noqa: E402
import ctypes.util  # noqa: E402
import json  # noqa: E402
import math  # noqa: E402
import os  # noqa: E402
import pathlib  # noqa: E402
import random  # noqa: E402
import resource  # noqa: E402
import subprocess  # noqa: E402
import time  # noqa: E402

import machine  # noqa: E402

# Values of the OpenAL 1.1 headers and of OpenAL Soft's extensions (AL/al.h,
# AL/alc.h, AL/alext.h, AL/efx.h).
AL_NO_ERROR = 0
AL_PITCH = 0x1003
AL_POSITION = 0x1004
AL_VELOCITY = 0x1006
AL_LOOPING = 0x1007
AL_BUFFER = 0x1009
AL_GAIN = 0x100A
AL_SOURCE_STATE = 0x1010
AL_PLAYING = 0x1012
AL_VENDOR = 0xB001
AL_VERSION = 0xB002
AL_RENDERER = 0xB003
AL_EXTENSIONS = 0xB004
AL_DOPPLER_FACTOR = 0xC000
AL_SPEED_OF_SOUND = 0xC003
AL_DISTANCE_MODEL = 0xD000
AL_FORMAT_MONO_FLOAT32 = 0x10010
AL_NUM_RESAMPLERS_SOFT = 0x1210
AL_DEFAULT_RESAMPLER_SOFT = 0x1211
AL_SOURCE_RESAMPLER_SOFT = 0x1212
AL_RESAMPLER_NAME_SOFT = 0x1213
AL_AUXILIARY_SEND_FILTER = 0x20006
AL_AIR_ABSORPTION_FACTOR = 0x20007
AL_EFFECT_TYPE = 0x8001
AL_EFFECT_REVERB = 0x0001
AL_EFFECT_EAXREVERB = 0x8000
AL_EFFECTSLOT_EFFECT = 0x0001
AL_FILTER_NULL = 0

ALC_FREQUENCY = 0x1007
ALC_EXTENSIONS = 0x1006
ALC_MONO_SOURCES = 0x1010
ALC_STEREO_SOURCES = 0x1011
ALC_FORMAT_CHANNELS_SOFT = 0x1990
ALC_FORMAT_TYPE_SOFT = 0x1991
ALC_FLOAT_SOFT = 0x1406
ALC_STEREO_SOFT = 0x1501
ALC_HRTF_SOFT = 0x1992
ALC_HRTF_STATUS_SOFT = 0x1993
ALC_NUM_HRTF_SPECIFIERS_SOFT = 0x1994
ALC_HRTF_SPECIFIER_SOFT = 0x1995
ALC_OUTPUT_LIMITER_SOFT = 0x199A
ALC_OUTPUT_MODE_SOFT = 0x19AC
ALC_MAX_AUXILIARY_SENDS = 0x20003

HRTF_STATUS = {0: "disabled", 1: "enabled", 2: "denied", 3: "required", 4: "headphones detected", 5: "unsupported format"}
OUTPUT_MODE = {0x1500: "mono", 0x1501: "stereo", 0x19AE: "stereo, basic panning", 0x19AF: "stereo, UHJ", 0x19B2: "stereo, HRTF"}
DISTANCE_MODEL = {0: "none", 0xD001: "inverse", 0xD002: "inverse, clamped", 0xD003: "linear", 0xD004: "linear, clamped"}

# Beyond this many sources, the seconds of noise are shared in turn: a
# thousand sources would otherwise hold 200 MB of samples, which no game does.
DISTINCT_BUFFERS = 256
SEED = 0x05A6E


class Missing(Exception):
    """The library, or something this benchmark needs of it, is not there."""


class OpenAL:
    """The few entry points of OpenAL this benchmark calls, with their C types."""

    def __init__(self, path=None):
        name = path or ctypes.util.find_library("openal")
        if not name:
            raise Missing("no OpenAL library found (looked for libopenal); install OpenAL Soft or pass --library")
        try:
            lib = ctypes.CDLL(name)
        except OSError as error:
            raise Missing(f"cannot load {name}: {error}") from error
        self.path = name
        void, c_int, c_uint, c_float, text = None, ctypes.c_int, ctypes.c_uint, ctypes.c_float, ctypes.c_char_p
        pointer, boolean = ctypes.c_void_p, ctypes.c_ubyte
        ints, uints = ctypes.POINTER(c_int), ctypes.POINTER(c_uint)
        signatures = {
            "alcLoopbackOpenDeviceSOFT": (pointer, [text]),
            "alcIsRenderFormatSupportedSOFT": (boolean, [pointer, c_int, c_int, c_int]),
            "alcRenderSamplesSOFT": (void, [pointer, pointer, c_int]),
            "alcCreateContext": (pointer, [pointer, ints]),
            "alcMakeContextCurrent": (boolean, [pointer]),
            "alcDestroyContext": (void, [pointer]),
            "alcCloseDevice": (boolean, [pointer]),
            "alcGetIntegerv": (void, [pointer, c_int, c_int, ints]),
            "alcGetString": (text, [pointer, c_int]),
            "alcGetStringiSOFT": (text, [pointer, c_int, c_int]),
            "alcIsExtensionPresent": (boolean, [pointer, text]),
            "alcGetError": (c_int, [pointer]),
            "alGetError": (c_int, []),
            "alGetString": (text, [c_int]),
            "alGetStringiSOFT": (text, [c_int, c_int]),
            "alGetInteger": (c_int, [c_int]),
            "alGetFloat": (c_float, [c_int]),
            "alIsExtensionPresent": (boolean, [text]),
            "alGenBuffers": (void, [c_int, uints]),
            "alBufferData": (void, [c_uint, c_int, pointer, c_int, c_int]),
            "alGenSources": (void, [c_int, uints]),
            "alSourcei": (void, [c_uint, c_int, c_int]),
            "alSourcef": (void, [c_uint, c_int, c_float]),
            "alSource3f": (void, [c_uint, c_int, c_float, c_float, c_float]),
            "alSource3i": (void, [c_uint, c_int, c_int, c_int, c_int]),
            "alGetSourcei": (void, [c_uint, c_int, ints]),
            "alSourcePlayv": (void, [c_int, uints]),
            "alListener3f": (void, [c_int, c_float, c_float, c_float]),
            "alGenEffects": (void, [c_int, uints]),
            "alEffecti": (void, [c_uint, c_int, c_int]),
            "alGenAuxiliaryEffectSlots": (void, [c_int, uints]),
            "alAuxiliaryEffectSloti": (void, [c_uint, c_int, c_int]),
        }
        for symbol, (result, arguments) in signatures.items():
            try:
                function = getattr(lib, symbol)
            except AttributeError as error:
                raise Missing(f"{name} has no {symbol}: this benchmark needs OpenAL Soft 1.19 or later") from error
            function.restype, function.argtypes = result, arguments
            setattr(self, symbol, function)
        if not self.alcIsExtensionPresent(None, b"ALC_SOFT_loopback"):
            raise Missing(f"{name} has no ALC_SOFT_loopback: it cannot render into memory")

    def integer(self, device, parameter):
        value = ctypes.c_int(0)
        self.alcGetIntegerv(device, parameter, 1, ctypes.byref(value))
        return value.value

    def check(self, what):
        error = self.alGetError()
        if error != AL_NO_ERROR:
            raise Missing(f"OpenAL refused to {what} (error 0x{error:04X})")


def text(value):
    return value.decode("utf-8", "replace") if value else None


class Scene:
    """A loopback device, its context, and N sources playing."""

    def __init__(self, al, case):
        self.al, self.case, self.n = al, case, case["n"]
        self.device = al.alcLoopbackOpenDeviceSOFT(None)
        if not self.device:
            raise Missing("OpenAL Soft would not open its loopback device")
        if not al.alcIsRenderFormatSupportedSOFT(self.device, machine.RATE, ALC_STEREO_SOFT, ALC_FLOAT_SOFT):
            raise Missing(f"the loopback device does not render stereo float at {machine.RATE} Hz")
        self.hrtf_extension = bool(al.alcIsExtensionPresent(self.device, b"ALC_SOFT_HRTF"))
        if case["hrtf"] and not self.hrtf_extension:
            raise Missing("this OpenAL has no ALC_SOFT_HRTF: HRTF cannot be asked for")
        self.efx = bool(al.alcIsExtensionPresent(self.device, b"ALC_EXT_EFX"))
        attributes = [ALC_FORMAT_CHANNELS_SOFT, ALC_STEREO_SOFT, ALC_FORMAT_TYPE_SOFT, ALC_FLOAT_SOFT]
        attributes += [ALC_FREQUENCY, machine.RATE]
        if self.hrtf_extension:
            attributes += [ALC_HRTF_SOFT, 1 if case["hrtf"] else 0]
        if case.get("ask_sources"):
            attributes += [ALC_MONO_SOURCES, case["ask_sources"], ALC_STEREO_SOURCES, 0]
        attributes.append(0)
        self.context = al.alcCreateContext(self.device, (ctypes.c_int * len(attributes))(*attributes))
        if not self.context or not al.alcMakeContextCurrent(self.context):
            raise Missing(f"OpenAL Soft would not make a context (error 0x{al.alcGetError(self.device):04X})")
        self.sources = (ctypes.c_uint * self.n)()
        self.resampler = None
        self.reverb = None

    def granted(self):
        """What the device says it does, whatever was asked."""
        al, device = self.al, self.device
        hrtf = bool(al.integer(device, ALC_HRTF_SOFT)) if self.hrtf_extension else False
        mode = al.integer(device, ALC_OUTPUT_MODE_SOFT) if al.alcIsExtensionPresent(device, b"ALC_SOFT_output_mode") else None
        return {
            "hrtf": hrtf,
            "hrtf_status": HRTF_STATUS.get(al.integer(device, ALC_HRTF_STATUS_SOFT)) if self.hrtf_extension else None,
            "hrtf_name": text(al.alcGetString(device, ALC_HRTF_SPECIFIER_SOFT)) if hrtf else None,
            "output_mode": OUTPUT_MODE.get(mode, mode),
            "output_limiter": bool(al.integer(device, ALC_OUTPUT_LIMITER_SOFT)),
            "frequency": al.integer(device, ALC_FREQUENCY),
            "mono_sources": al.integer(device, ALC_MONO_SOURCES),
            "stereo_sources": al.integer(device, ALC_STEREO_SOURCES),
            "sends_per_source": al.integer(device, ALC_MAX_AUXILIARY_SENDS) if self.efx else 0,
        }

    def start(self):
        """Make the buffers and the sources, and set them playing together."""
        al, case, n = self.al, self.case, self.n
        distinct = min(n, DISTINCT_BUFFERS)
        buffers = (ctypes.c_uint * distinct)()
        al.alGenBuffers(distinct, buffers)
        al.check("make buffers")
        scale = (1.0 / 32768.0).__mul__
        for index in range(distinct):
            # White noise, uniform over 16 bits, from a generator seeded per source.
            words = array.array("h", random.Random(SEED + index).randbytes(2 * machine.RATE))
            samples = array.array("f", map(scale, words))
            address, count = samples.buffer_info()
            al.alBufferData(buffers[index], AL_FORMAT_MONO_FLOAT32, address, 4 * count, machine.RATE)
        al.check("fill a buffer with 32-bit float samples (AL_EXT_FLOAT32)")
        al.alGenSources(n, self.sources)
        error = al.alGetError()
        if error != AL_NO_ERROR:
            raise Missing(f"OpenAL Soft would not make {n} sources (error 0x{error:04X}): its limit is lower")
        if case.get("resampler") is not None:
            if not al.alIsExtensionPresent(b"AL_SOFT_source_resampler"):
                raise Missing("this OpenAL has no AL_SOFT_source_resampler: the resampler cannot be chosen")
            self.resampler = case["resampler"]
        elif al.alIsExtensionPresent(b"AL_SOFT_source_resampler"):
            self.resampler = al.alGetInteger(AL_DEFAULT_RESAMPLER_SOFT)
        slot = None
        if case.get("reverb"):
            slot = self.reverb_slot()
        # The sum of N equal noises grows as the square root of N.
        gain = 1.0 / math.sqrt(n)
        for index, source in enumerate(self.sources):
            al.alSourcei(source, AL_BUFFER, buffers[index % distinct])
            al.alSourcei(source, AL_LOOPING, 1)
            al.alSourcef(source, AL_GAIN, gain)
            if case.get("resampler") is not None:
                al.alSourcei(source, AL_SOURCE_RESAMPLER_SOFT, case["resampler"])
            if slot is not None:
                al.alSource3i(source, AL_AUXILIARY_SEND_FILTER, slot, 0, AL_FILTER_NULL)
            if case.get("air"):
                al.alSourcef(source, AL_AIR_ABSORPTION_FACTOR, 1.0)
        al.check("set up the sources")
        al.alListener3f(AL_POSITION, machine.LISTENER_OFFSET, 0.0, 0.0)
        self.place(0.0)
        al.alSourcePlayv(n, self.sources)
        al.check("play the sources")

    def reverb_slot(self):
        """One reverb, fed by every source: the use the API is designed for."""
        al = self.al
        if not self.efx:
            raise Missing("this OpenAL has no ALC_EXT_EFX: no reverb send")
        effect, slot = ctypes.c_uint(0), ctypes.c_uint(0)
        al.alGenEffects(1, ctypes.byref(effect))
        al.alEffecti(effect, AL_EFFECT_TYPE, AL_EFFECT_EAXREVERB)
        self.reverb = "EAX reverb, default preset"
        if al.alGetError() != AL_NO_ERROR:
            al.alEffecti(effect, AL_EFFECT_TYPE, AL_EFFECT_REVERB)
            self.reverb = "standard reverb, default preset"
        al.alGenAuxiliaryEffectSlots(1, ctypes.byref(slot))
        al.alAuxiliaryEffectSloti(slot, AL_EFFECTSLOT_EFFECT, effect.value)
        al.check("load a reverb in an effect slot")
        return slot.value

    def place(self, seconds):
        """Every source where it is at this time, with its velocity."""
        set3 = self.al.alSource3f
        radius, speed, n = machine.CIRCLE_RADIUS, machine.SOURCE_SPEED, self.n
        turned = seconds * speed / radius if self.case.get("moving", True) else 0.0
        still = not self.case.get("moving", True)
        for index, source in enumerate(self.sources):
            angle = math.tau * index / n + turned
            cos, sin = math.cos(angle), math.sin(angle)
            set3(source, AL_POSITION, radius * cos, 0.0, radius * sin)
            if not still:
                set3(source, AL_VELOCITY, -speed * sin, 0.0, speed * cos)

    def render(self, seconds, block):
        """Render, and tell the engine's time from the time of driving it."""
        al, device, moving = self.al, self.device, self.case.get("moving", True)
        output = (ctypes.c_float * (2 * block))()
        frames = round(seconds * machine.RATE)
        blocks = [block] * (frames // block) + ([frames % block] if frames % block else [])
        wall, cpu, thread, perf = time.perf_counter_ns, time.process_time_ns, time.thread_time_ns, time.perf_counter_ns
        render_wall = render_cpu = render_thread = update_wall = 0
        usage = resource.getrusage(resource.RUSAGE_SELF)
        begin = wall()
        done = 0
        for count in blocks:
            if moving:
                before = perf()
                self.place(done / machine.RATE)
                update_wall += perf() - before
            t0, c0, w0 = thread(), cpu(), wall()
            al.alcRenderSamplesSOFT(device, output, count)
            w1, c1, t1 = wall(), cpu(), thread()
            render_wall += w1 - w0
            render_cpu += c1 - c0
            render_thread += t1 - t0
            done += count
        whole_wall = wall() - begin
        after = resource.getrusage(resource.RUSAGE_SELF)
        left = math.sqrt(sum(output[2 * i] ** 2 for i in range(blocks[-1])) / blocks[-1])
        right = math.sqrt(sum(output[2 * i + 1] ** 2 for i in range(blocks[-1])) / blocks[-1])
        return {
            "frames": frames,
            "render_wall_ns": render_wall,
            "render_cpu_ns": render_cpu,
            "render_thread_ns": render_thread,
            "update_wall_ns": update_wall,
            "whole_wall_ns": whole_wall,
            "whole_rusage_ns": round(1e9 * ((after.ru_utime + after.ru_stime) - (usage.ru_utime + usage.ru_stime))),
            "last_block_rms": [left, right],
        }

    def playing(self):
        state, count = ctypes.c_int(0), 0
        for source in self.sources:
            self.al.alGetSourcei(source, AL_SOURCE_STATE, ctypes.byref(state))
            count += state.value == AL_PLAYING
        return count

    def close(self):
        self.al.alcMakeContextCurrent(None)
        self.al.alcDestroyContext(self.context)
        self.al.alcCloseDevice(self.device)


def threads():
    """The names of this process's threads."""
    names = []
    for tid in sorted(os.listdir("/proc/self/task"), key=int):
        names.append(machine.read(f"/proc/self/task/{tid}/comm") or "?")
    return names


def child_run(case):
    """One case, in this process: a warm-up, then the measured renders."""
    al = OpenAL(case.get("library"))
    rss_before = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    threads_before = threads()
    scene = Scene(al, case)
    granted = scene.granted()
    scene.start()
    scene.render(case["seconds"], case["block"])
    runs = [scene.render(case["seconds"], case["block"]) for _ in range(case["runs"])]
    result = {
        "case": case,
        "granted": granted,
        "resampler": text(al.alGetStringiSOFT(AL_RESAMPLER_NAME_SOFT, scene.resampler)) if scene.resampler is not None else None,
        "reverb": scene.reverb,
        "runs": runs,
        "sources_playing_at_the_end": scene.playing(),
        "threads_before": threads_before,
        "threads_while_rendering": threads(),
        "max_rss_kb_before": rss_before,
        "max_rss_kb_after": resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
        "sample_data_kb": min(case["n"], DISTINCT_BUFFERS) * machine.RATE * 4 // 1024,
    }
    al.check("render")
    scene.close()
    return result


def tone_frequency(al, scene, towards_z):
    """The frequency heard of the tone, by counting its zero crossings."""
    al.alSource3f(scene.sources[0], AL_VELOCITY, 0.0, 0.0, towards_z)
    block = 4096
    output = (ctypes.c_float * (2 * block))()
    al.alcRenderSamplesSOFT(scene.device, output, block)  # let the change settle
    crossings, frames, last = 0, 0, 0.0
    for _ in range(12):
        al.alcRenderSamplesSOFT(scene.device, output, block)
        for i in range(block):
            sample = output[2 * i] + output[2 * i + 1]
            crossings += (last < 0.0) != (sample < 0.0)
            last = sample
        frames += block
    return crossings / 2 * machine.RATE / frames


def tone_level(al, scene, distance):
    """The level of a source at this distance, straight ahead."""
    al.alSource3f(scene.sources[0], AL_POSITION, machine.LISTENER_OFFSET, 0.0, -distance)
    block = 4096
    output = (ctypes.c_float * (2 * block))()
    al.alcRenderSamplesSOFT(scene.device, output, block)
    al.alcRenderSamplesSOFT(scene.device, output, block)
    return math.sqrt(sum(sample * sample for sample in output) / len(output))


def loaded_from(name):
    """The file the library was really loaded from."""
    for line in (machine.read("/proc/self/maps") or "").splitlines():
        path = line.split(None, 5)[-1]
        if "openal" in os.path.basename(path).lower() or os.path.basename(path) == os.path.basename(name):
            return os.path.realpath(path)
    return name


def child_info(case):
    """What the library is, what it grants, and whether the scene exercises it."""
    al = OpenAL(case.get("library"))
    threads_before = threads()
    info = {"library": loaded_from(al.path)}
    scene = Scene(al, {"n": 1, "hrtf": True})
    device = scene.device
    info.update(
        vendor=text(al.alGetString(AL_VENDOR)),
        renderer=text(al.alGetString(AL_RENDERER)),
        version=text(al.alGetString(AL_VERSION)),
        extensions=sorted((text(al.alGetString(AL_EXTENSIONS)) or "").split()),
        device_extensions=sorted((text(al.alcGetString(device, ALC_EXTENSIONS)) or "").split()),
        hrtf_names=[
            text(al.alcGetStringiSOFT(device, ALC_HRTF_SPECIFIER_SOFT, i))
            for i in range(al.integer(device, ALC_NUM_HRTF_SPECIFIERS_SOFT))
        ],
        granted_with_hrtf_asked=scene.granted(),
        distance_model=DISTANCE_MODEL.get(al.alGetInteger(AL_DISTANCE_MODEL)),
        doppler_factor=al.alGetFloat(AL_DOPPLER_FACTOR),
        speed_of_sound=al.alGetFloat(AL_SPEED_OF_SOUND),
        threads_before=threads_before,
        threads_with_a_context=threads(),
    )
    if al.alIsExtensionPresent(b"AL_SOFT_source_resampler"):
        names = [text(al.alGetStringiSOFT(AL_RESAMPLER_NAME_SOFT, i)) for i in range(al.alGetInteger(AL_NUM_RESAMPLERS_SOFT))]
        info["resamplers"] = names
        info["default_resampler"] = names[al.alGetInteger(AL_DEFAULT_RESAMPLER_SOFT)]

    # Does the scene exercise what it claims to? A 1 kHz tone coming at a
    # tenth of the speed of sound must be heard at 1000 / (1 - 0.1) Hz, and a
    # source four times as far must be four times as weak.
    tone = array.array("f", (math.sin(math.tau * 1000.0 * i / machine.RATE) for i in range(machine.RATE)))
    buffer = ctypes.c_uint(0)
    al.alGenBuffers(1, ctypes.byref(buffer))
    address, count = tone.buffer_info()
    al.alBufferData(buffer, AL_FORMAT_MONO_FLOAT32, address, 4 * count, machine.RATE)
    al.alGenSources(1, scene.sources)
    source = scene.sources[0]
    al.alSourcei(source, AL_BUFFER, buffer.value)
    al.alSourcei(source, AL_LOOPING, 1)
    al.alListener3f(AL_POSITION, machine.LISTENER_OFFSET, 0.0, 0.0)
    al.alSource3f(source, AL_POSITION, machine.LISTENER_OFFSET, 0.0, -2.0)
    al.alSourcePlayv(1, scene.sources)
    al.check("play a tone")
    closing = 0.1 * info["speed_of_sound"]
    info["checks"] = {
        "tone_hz": 1000.0,
        "heard_at_rest_hz": tone_frequency(al, scene, 0.0),
        "closing_speed_m_s": closing,
        # The source is ahead of the listener (towards -z) and moves towards +z.
        "heard_closing_hz": None,
        "expected_closing_hz": 1000.0 / 0.9,
    }
    info["checks"]["heard_closing_hz"] = tone_frequency(al, scene, closing)
    tone_frequency(al, scene, 0.0)
    near, far = tone_level(al, scene, 2.0), tone_level(al, scene, 8.0)
    info["checks"]["level_at_2_m_over_level_at_8_m"] = near / far if far else None
    scene.close()

    # How many sources a context gives: by default, then when more are asked for.
    info["source_limit"] = {}
    for name, ask in (("default", None), ("asked_4096", 4096)):
        probe = Scene(al, {"n": 1, "hrtf": True, "ask_sources": ask})
        granted = probe.granted()
        made, one = 0, ctypes.c_uint(0)
        while made < 8192:
            al.alGenSources(1, ctypes.byref(one))
            if al.alGetError() != AL_NO_ERROR:
                break
            made += 1
        info["source_limit"][name] = {
            "mono_sources": granted["mono_sources"],
            "stereo_sources": granted["stereo_sources"],
            "sources_made_before_refusal": made if made < 8192 else "8192 or more",
        }
        probe.close()
    return info


def run_child(mode, case, timeout):
    """Run one case in a fresh process and return what it measured."""
    command = [sys.executable, str(pathlib.Path(__file__).resolve()), "--child", mode, json.dumps(case)]
    done = subprocess.run(command, capture_output=True, text=True, timeout=timeout, check=False)
    if done.returncode != 0:
        raise Missing((done.stderr.strip().splitlines() or [f"the case exited with status {done.returncode}"])[-1])
    return json.loads(done.stdout)


def summarise(result):
    """The figures of one case, from its runs."""
    case, runs = result["case"], result["runs"]
    n, seconds = case["n"], case["seconds"]
    cpu = machine.spread([run["render_cpu_ns"] / seconds / 1e6 for run in runs])
    wall = machine.spread([run["render_wall_ns"] / seconds / 1e6 for run in runs])
    render_cpu = sum(run["render_cpu_ns"] for run in runs)
    return {
        "n": n,
        "measured_runs": len(runs),
        "cpu_ms_per_s": cpu,
        "wall_ms_per_s": wall,
        "ns_per_source_sample": cpu["median"] * 1e6 / (n * machine.RATE),
        # The calls that move the sources, made from Python: mostly the
        # interpreter's time, which a game written in C would not pay.
        "update_wall_ms_per_s": machine.spread([run["update_wall_ns"] / seconds / 1e6 for run in runs])["median"],
        # CPU time of the whole process over the whole run, from getrusage:
        # the engine, the updates and the loop around them.
        "whole_rusage_ms_per_s": machine.spread([run["whole_rusage_ns"] / seconds / 1e6 for run in runs])["median"],
        # The share of the CPU time spent, during the render calls, in other
        # threads than the caller: zero when the engine mixes where it is called.
        "share_of_cpu_in_other_threads": max(0.0, 1.0 - sum(run["render_thread_ns"] for run in runs) / render_cpu)
        if render_cpu
        else None,
        "max_rss_growth_kb": result["max_rss_kb_after"] - result["max_rss_kb_before"],
        "of_which_sample_data_kb": result["sample_data_kb"],
        "threads_while_rendering": result["threads_while_rendering"],
        "last_block_rms": runs[-1]["last_block_rms"],
        "sources_playing_at_the_end": result["sources_playing_at_the_end"],
        "runs": runs,
    }


def line(results):
    """The cost as a fixed part and a part per source.

    With HRTF, OpenAL Soft pays a fixed price whatever the number of sources,
    so the cost of one source alone says little about the cost of one more.
    """
    if len(results) < 2:
        return None
    fixed, per_source = machine.fit_line([(r["n"], r["cpu_ms_per_s"]["median"]) for r in results])
    return {"fixed_ms_per_s": fixed, "per_source_ms_per_s": per_source, "ns_per_added_source_sample": per_source * 1e6 / machine.RATE}


def parent(arguments):
    try:
        OpenAL(arguments.library)
    except Missing as error:
        raise SystemExit(f"openal_soft.py: {error}") from error
    cpu, lock = machine.claim("openal_soft.py", arguments.cpu)  # noqa: F841 (the lock is held until the end)
    receipt = {
        "what": "OpenAL Soft, cost per source through its loopback device",
        "date": machine.now(),
        "machine_before": machine.machine(cpu),
        "configuration_files_found": [
            path
            for path in ("/etc/xdg/alsoft.conf", os.path.expanduser("~/.config/alsoft.conf"), os.path.expanduser("~/.alsoftrc"))
            if os.path.exists(path)
        ],
        "environment": {key: value for key, value in os.environ.items() if key.startswith("ALSOFT_")},
    }
    common = {"seconds": arguments.seconds, "block": arguments.block, "runs": arguments.runs, "library": arguments.library}
    timeout = 120.0 + 60.0 * arguments.seconds * (arguments.runs + 1)
    try:
        info = run_child("info", {"library": arguments.library}, 300.0)
    except Missing as error:
        raise SystemExit(f"openal_soft.py: {error}") from error
    receipt["engine"] = info
    whole_sibling = machine.watch_siblings(cpu)
    resamplers = info.get("resamplers") or []
    default_cap = info["source_limit"]["default"]["sources_made_before_refusal"]
    raised = info["source_limit"]["asked_4096"]["sources_made_before_refusal"]
    raised = 8192 if isinstance(raised, str) else raised
    can_raise = raised > default_cap

    variants = [
        ("HRTF", {"hrtf": True}),
        ("panning", {"hrtf": False}),
    ]
    if resamplers:
        # The first resampler of the list is the cheapest: no interpolation.
        variants += [
            (f"HRTF, resampler '{resamplers[0]}'", {"hrtf": True, "resampler": 0}),
            (f"panning, resampler '{resamplers[0]}'", {"hrtf": False, "resampler": 0}),
        ]
    variants += [
        ("HRTF, sources at rest", {"hrtf": True, "moving": False}),
        ("HRTF, air absorption", {"hrtf": True, "air": True}),
    ]
    if "ALC_EXT_EFX" in info["device_extensions"]:
        variants.append(("HRTF, one reverb send per source", {"hrtf": True, "reverb": True}))
    counts = [n for n in arguments.counts if n <= default_cap]
    skipped = [n for n in arguments.counts if n > default_cap]

    # Beyond the default limit a run takes minutes: three measured runs there.
    few = {"runs": min(3, arguments.runs)}

    def measure(case, change=None):
        case = dict(common, **case, **(change or {}))
        if case["n"] > default_cap:
            case["ask_sources"] = case["n"]
        print(f"  {case['n']:5d} sources ...", end="", file=sys.stderr, flush=True)
        sibling = machine.watch_siblings(cpu)
        result = run_child("run", case, timeout)
        summary = summarise(result)
        summary.update(granted=result["granted"], resampler=result["resampler"], reverb=result["reverb"], sibling_busy_share=sibling())
        print(f" {summary['cpu_ms_per_s']['median']:8.2f} ms of CPU per second", file=sys.stderr, flush=True)
        return summary

    receipt["variants"] = []
    for name, case in variants:
        print(f"{name}", file=sys.stderr, flush=True)
        try:
            results = [measure(dict(case, n=n)) for n in counts]
        except Missing as error:
            print(f"  skipped: {error}", file=sys.stderr, flush=True)
            receipt["variants"].append({"name": name, "case": case, "skipped": str(error)})
            continue
        granted = results[0]["granted"]
        if granted["hrtf"] != case["hrtf"]:
            raise SystemExit(f"openal_soft.py: HRTF was {'asked' if case['hrtf'] else 'refused'} but the device says otherwise")
        receipt["variants"].append(
            {
                "name": name,
                "case": case,
                "granted": granted,
                "resampler": results[0]["resampler"],
                "reverb": results[0]["reverb"],
                "line": line(results),
                "results": results,
            }
        )

    # Beyond the default limit, and up to real time, for the two main cases.
    for variant in receipt["variants"][:2]:
        if "results" not in variant:
            continue
        print(f"{variant['name']}: beyond {default_cap} sources, and the limit of real time", file=sys.stderr, flush=True)
        variant["beyond_the_default_limit"] = []
        try:
            if can_raise and not arguments.quick:
                for n in sorted(set(skipped) | {2 * default_cap, 4 * default_cap}):
                    if n <= raised:
                        variant["beyond_the_default_limit"].append(measure(dict(variant["case"], n=n), few))
            points = [(r["n"], r["cpu_ms_per_s"]["median"]) for r in variant["results"] + variant["beyond_the_default_limit"]]
            variant["real_time"] = machine.realtime_limit(
                points,
                None if can_raise else default_cap,
                lambda n, case=variant["case"]: measure(dict(case, n=n), few)["cpu_ms_per_s"]["median"],
            )
        except (Missing, subprocess.TimeoutExpired) as error:
            # What was measured so far is still worth its receipt.
            print(f"  stopped: {error}", file=sys.stderr, flush=True)
            variant["beyond_the_default_limit_stopped"] = str(error)

    receipt["machine_after"] = machine.machine(cpu)
    receipt["sibling_busy_share"] = whole_sibling()
    receipt["source_limit"] = {"default": default_cap, "when_4096_are_asked": info["source_limit"]["asked_4096"], "can_be_raised": can_raise}
    report(receipt)
    path = machine.write_receipt(arguments.out, receipt)
    print(f"\nreceipt: {path}")


def report(receipt):
    """The tables, for a person."""
    engine, before, after = receipt["engine"], receipt["machine_before"], receipt["machine_after"]
    pinned = before["pinned"]
    print(f"OpenAL Soft baseline, {receipt['date']}")
    print(f"  library    {engine['library']}: {engine['vendor']}, {engine['renderer']}, {engine['version']}")
    print(f"  HRTF       {engine['granted_with_hrtf_asked']['hrtf_name']} (available: {', '.join(engine['hrtf_names'])})")
    print(f"  resampler  default '{engine.get('default_resampler')}', cheapest '{(engine.get('resamplers') or [None])[0]}'")
    print(f"  distance   {engine['distance_model']}; Doppler factor {engine['doppler_factor']:g}, sound at {engine['speed_of_sound']:g} m/s")
    checks = engine["checks"]
    print(
        f"  checks     a 1 kHz tone closing at {checks['closing_speed_m_s']:.1f} m/s is heard at {checks['heard_closing_hz']:.0f} Hz"
        f" (expected {checks['expected_closing_hz']:.0f}); level at 2 m over level at 8 m: {checks['level_at_2_m_over_level_at_8_m']:.2f}"
    )
    limit = receipt["source_limit"]
    print(
        f"  sources    {limit['default']} by default (ALC_MONO_SOURCES {engine['source_limit']['default']['mono_sources']},"
        f" ALC_STEREO_SOURCES {engine['source_limit']['default']['stereo_sources']});"
        f" {limit['when_4096_are_asked']['sources_made_before_refusal']} when 4096 are asked for"
    )
    print(f"  threads    {', '.join(engine['threads_with_a_context'])} (before the library was used: {', '.join(engine['threads_before'])})")
    print(f"  machine    {before['cpu_model']}, kernel {before['kernel']}, Python {before['python']}")
    if pinned:
        mhz = [state["pinned"]["current_khz"] for state in (before, after)]
        print(
            f"  core       pinned to CPU {pinned['cpu']} ({pinned['kind'] or 'kind unknown'}), governor {pinned['governor']},"
            f" preference {pinned['energy_performance_preference']}, turbo {'off' if before['turbo_disabled'] else 'on'},"
            f" {(mhz[0] or 0) / 1000:.0f} MHz before, {(mhz[1] or 0) / 1000:.0f} MHz after"
            f" (limit {(pinned['max_khz'] or 0) / 1000:.0f} MHz)"
        )
    else:
        print("  core       not pinned")
    print(f"  load       {machine.load_sentence(receipt)}")
    if receipt["configuration_files_found"] or receipt["environment"]:
        print(f"  WARNING    not the library's defaults: {receipt['configuration_files_found']} {receipt['environment']}")
    headers = ["case", "N", "CPU ms/s", "min", "max", "wall ms/s", "ns/source-sample", "other threads", "updates ms/s", "RSS +MB", "samples MB", "sibling busy"]
    rows = []
    for variant in receipt["variants"]:
        if "skipped" in variant:
            rows.append([variant["name"], "skipped: " + variant["skipped"]] + [""] * (len(headers) - 2))
            continue
        for result in variant["results"] + variant.get("beyond_the_default_limit", []):
            cpu = result["cpu_ms_per_s"]
            rows.append(
                [
                    variant["name"],
                    result["n"],
                    cpu["median"],
                    cpu["min"],
                    cpu["max"],
                    result["wall_ms_per_s"]["median"],
                    result["ns_per_source_sample"],
                    f"{100 * result['share_of_cpu_in_other_threads']:.1f} %",
                    result["update_wall_ms_per_s"],
                    result["max_rss_growth_kb"] / 1024,
                    result["of_which_sample_data_kb"] / 1024,
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
    for variant in receipt["variants"]:
        if variant.get("real_time"):
            print(machine.real_time_sentence(variant["name"], variant["real_time"]))


def main():
    parser = argparse.ArgumentParser(description="Cost per source of OpenAL Soft, rendered through its loopback device.")
    parser.add_argument("--out", default=str(machine.OUT / "openal_soft.json"), help="where the JSON receipt goes")
    parser.add_argument("--library", help="the OpenAL library to load (default: the system's libopenal)")
    parser.add_argument("--counts", type=lambda s: [int(n) for n in s.split(",")], default=list(machine.COUNTS), help="numbers of sources")
    parser.add_argument("--seconds", type=float, default=machine.SECONDS, help="sound rendered per run")
    parser.add_argument("--block", type=int, default=machine.BLOCK, help="frames per render call and between two updates")
    parser.add_argument("--runs", type=int, default=5, help="measured runs per case, after one warm-up")
    parser.add_argument("--cpu", default="auto", help="the logical CPU to pin to, 'auto' or 'none'")
    parser.add_argument("--quick", action="store_true", help="skip the cases beyond the default limit of sources")
    parser.add_argument("--child", nargs=2, metavar=("MODE", "CASE"), help=argparse.SUPPRESS)
    arguments = parser.parse_args()
    if arguments.child:
        mode, case = arguments.child[0], json.loads(arguments.child[1])
        try:
            result = child_info(case) if mode == "info" else child_run(case)
        except Missing as error:
            print(error, file=sys.stderr)
            raise SystemExit(1) from error
        json.dump(result, sys.stdout)
        return
    parent(arguments)


if __name__ == "__main__":
    main()
