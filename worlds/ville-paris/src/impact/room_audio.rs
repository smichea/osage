// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Render a stationary mono emitter through authored rooms to a stationary listener.
mod file;
mod wave;

use super::{
    Listening,
    listener::Listener,
    transmission::{DirectPath, Transmission},
};
use crate::{object::rooms::Route, scene::Sector};
use anyhow::{Context, Result, bail, ensure};
use file::Output;
use osage_objects::BAND_HZ;
use osage_paris_soundscape::{
    ears::hear_for,
    scene::Head,
    space::Vec3,
    ways::{SOUND, edge_loss},
};
use osage_paris_voices::RATE;
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};

const HELP: &str =
    "ville-paris --room-audio --scene FILE.glb --input MONO.wav --source X,Y,Z --listener X,Y,Z --out STEREO.wav
    [--listening speakers|headphones] [--emit-frame N] [--float]
The source WAV is mono at 48000 Hz (up to 60 s). Output is stereo at 48000 Hz (up to 120 s).
The default writes PCM16. --float writes raw IEEE float32, including values outside +/-1.
World metres: the listener faces -Z, +X is right and +Y is up. Frame N schedules sample 0 of the clip.
Disconnected rooms produce silence. Endpoints outside rooms or in overlapping rooms are errors.";
const MAX_OUTPUT: u64 = 120 * RATE as u64;
const BLOCK: usize = 2048;

fn same_file(a: &Path, b: &Path) -> Result<bool> {
    if a.canonicalize()? == b.canonicalize()? {
        return Ok(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let (a, b) = (a.metadata()?, b.metadata()?);
        Ok(a.dev() == b.dev() && a.ino() == b.ino())
    }
    #[cfg(not(unix))]
    Ok(false)
}

pub(crate) fn cli(mut args: impl Iterator<Item = String>) -> Result<()> {
    let (mut scene, mut input, mut output) = (None, None, None);
    let (mut source, mut receiver) = (None, None);
    let mut emit = 0;
    let mut mode = Listening::Speakers;
    let mut float = false;
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("missing value for {arg}"));
        match arg.as_str() {
            "--help" | "-h" => {
                println!("{HELP}");
                return Ok(());
            }
            "--scene" => scene = Some(PathBuf::from(value()?)),
            "--input" => input = Some(PathBuf::from(value()?)),
            "--out" => output = Some(PathBuf::from(value()?)),
            "--float" => float = true,
            "--emit-frame" => emit = value()?.parse::<u64>().context("invalid emission frame")?,
            "--source" => source = Some(crate::point(value()?)?),
            "--listener" => receiver = Some(crate::point(value()?)?),
            "--listening" => {
                mode = match value()?.as_str() {
                    "speakers" => Listening::Speakers,
                    "headphones" => Listening::Headphones,
                    other => bail!("unknown listening mode {other}, expected speakers or headphones"),
                }
            }
            _ => bail!("unknown room audio option {arg}\n{HELP}"),
        }
    }
    let scene = scene.context("--room-audio needs --scene")?;
    let input = input.context("--room-audio needs --input")?;
    let output = output.context("--room-audio needs --out")?;
    if output.try_exists()? {
        ensure!(!same_file(&output, &input)? && !same_file(&output, &scene)?, "audio output would overwrite an input");
    }
    let source = source.context("--room-audio needs --source")?;
    let receiver = receiver.context("--room-audio needs --listener")?;
    ensure!(source.into_iter().chain(receiver).all(f32::is_finite), "source and listener coordinates must be finite");
    ensure!(emit < MAX_OUTPUT, "emission exceeds the 120-second output limit");
    let samples = wave::read(&input).with_context(|| format!("reading {}", input.display()))?;
    let scene = Sector::read(&fs::read(&scene).with_context(|| format!("reading {}", scene.display()))?)?;
    let graph = scene.object.as_ref().and_then(|o| o.sound.rooms.as_ref()).context("scene has no acoustic rooms")?;
    let route = graph.query(source, receiver);
    let report = render(&output, &samples, emit, mode, float, &scene, &route)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[derive(Serialize)]
struct PathSound {
    direction_from_m: [f64; 3],
    distance_gain: f64,
    bends: usize,
    diffraction_model: &'static str,
    excess_m: f64,
    diffraction_energy: [f64; 3],
    material_energy: [f64; 3],
    material_crossing_rule: &'static str,
    received_energy_before_spreading: [f64; 3],
    leg_transmission: Vec<DirectPath>,
    ear_delay_frames: [f64; 2],
    speaker_pan: Option<[f64; 2]>,
}

fn sound(route: &Route, distance: f64, sector: &Sector, mode: Listening) -> (PathSound, Listener) {
    let mut points: Vec<Vec3> = route.points_m.iter().copied().map(Vec3::from).collect();
    points.dedup();
    let head = Head { at: *points.last().unwrap(), forward: -Vec3::Z, right: Vec3::X, up: Vec3::Y };
    let mut turns = vec![points[0]];
    // A portal on the same straight leg does not split a material ray. Otherwise a
    // sheet placed exactly across such an opening would disappear at both endpoints.
    for tri in points.windows(3) {
        let [a, b, c] = [tri[0], tri[1], tri[2]];
        let axis = c - a;
        let tolerance = 64. * f64::EPSILON * (1. + distance);
        if (b - a).cross(axis).length() > tolerance * axis.length() || (b - a).dot(c - b) < 0. {
            turns.push(b);
        }
    }
    turns.push(head.at);
    let bends = turns.len() - 2;
    let direction_from_m = turns[turns.len() - 2];
    let heard = hear_for(direction_from_m, &head, mode, true);
    let excess = (distance - points[0].distance(head.at)).max(0.);
    // The graph describes apertures. Treat a shadowing wall as a rigid screen;
    // no corresponding transfer model is available here for three or more bends.
    let (diffraction_model, diffraction_energy) = match bends {
        1 => ("single_thin_screen", BAND_HZ.map(|f| 10f64.powf(-edge_loss(excess, f64::from(f)) / 10.))),
        2 => (
            "two_rigid_screens",
            super::diffraction::screen_loss_ratios(excess, Some(turns[1].distance(turns[2]))).map(|r| 1. / r),
        ),
        0 => ("straight", [1.; 3]),
        _ => ("unmodelled_multiple_bends", [1.; 3]),
    };
    let bridge = paris_soundscape::bridge();
    let leg_transmission = turns
        .windows(2)
        .filter(|p| p[0] != p[1])
        .map(|p| DirectPath::trace(sector.into(), &bridge, p[0], p[1], 0.))
        .collect::<Vec<_>>();
    let material_energy =
        leg_transmission.iter().fold([1.; 3], |a, leg| std::array::from_fn(|i| a[i] * leg.transmission_energy[i]));
    let energy = std::array::from_fn(|i| material_energy[i] * diffraction_energy[i]);
    let distance_gain = 1. / distance.max(1.);
    let path = PathSound {
        direction_from_m: direction_from_m.array(),
        distance_gain,
        bends,
        diffraction_model,
        excess_m: excess,
        diffraction_energy,
        material_energy,
        material_crossing_rule: "open_leg_interiors_with_collinear_openings_merged",
        received_energy_before_spreading: energy,
        leg_transmission,
        ear_delay_frames: heard.delay.map(|d| d * f64::from(RATE)),
        speaker_pan: (mode == Listening::Speakers).then_some(heard.pan),
    };
    (path, Listener::new(mode, heard, distance_gain))
}

fn silence(file: &mut Output, mut frames: u64) -> Result<()> {
    while frames > 0 {
        let n = frames.min(BLOCK as u64);
        file.write(vec![0.; 2 * n as usize])?;
        frames -= n;
    }
    Ok(())
}

fn render(
    path: &Path,
    input: &[f32],
    emission: u64,
    mode: Listening,
    float: bool,
    sector: &Sector,
    route: &Route,
) -> Result<Value> {
    let mut schedule = None;
    if route.status == "connected" {
        let distance = route.distance_m.context("connected room path has no length")?;
        let travel = distance / SOUND * f64::from(RATE);
        ensure!(travel.is_finite() && travel <= MAX_OUTPUT as f64, "propagation exceeds the 120-second output limit");
        schedule = Some((distance, emission + travel.round() as u64));
    } else {
        ensure!(route.status == "disconnected", "room endpoint classification: {}", route.status);
    }
    let mut response = None;
    let mut tail = 0;
    let mut overs = 0;
    let mut peak = [0f32; 2];
    let mut file = Output::new(path, float)?;
    if let Some((distance, arrival)) = schedule {
        let (acoustic, mut listener) = sound(route, distance, sector, mode);
        let energy = acoustic.received_energy_before_spreading;
        if mode == Listening::Headphones || energy[0] != energy[1] || energy[0] != energy[2] {
            // Flush both the ear history and the low-frequency crossover for 50 ms.
            tail = RATE as usize / 20;
        }
        ensure!(arrival + input.len() as u64 + tail as u64 <= MAX_OUTPUT, "render exceeds the 120-second output limit");
        silence(&mut file, arrival)?;
        let mut transmission = Transmission::new(energy);
        for start in (0..input.len() + tail).step_by(BLOCK) {
            let n = BLOCK.min(input.len() + tail - start);
            let mut mono = vec![0.; n];
            if start < input.len() {
                let count = n.min(input.len() - start);
                mono[..count].copy_from_slice(&input[start..start + count]);
            }
            transmission.apply(&mut mono);
            let mut samples = vec![0.; 2 * n];
            listener.add(&mono, samples.as_chunks_mut::<2>().0);
            ensure!(samples.iter().all(|v| v.is_finite()), "source exceeds available filter headroom");
            for pair in samples.as_chunks_mut::<2>().0 {
                for (x, p) in pair.iter_mut().zip(&mut peak) {
                    *p = p.max(x.abs());
                    if x.abs() > 1. {
                        overs += 1;
                        if !float {
                            *x = x.clamp(-1., 1.);
                        }
                    }
                }
            }
            file.write(samples)?;
        }
        response = Some(acoustic);
    } else {
        let frames = emission + input.len() as u64;
        ensure!(frames <= MAX_OUTPUT, "render exceeds the 120-second output limit");
        silence(&mut file, frames)?;
    }
    let (frames, first) = file.finish()?;
    Ok(
        json!({"format":"ville-paris/room-audio/1","listening":if mode==Listening::Speakers{"speakers"}else{"headphones"},
        "sample_rate":RATE,"frames":frames,"input_frames":input.len(),"emission_frame":emission,
        "first_nonzero_input_frame":input.iter().position(|&x|x!=0.),"arrival_frame":schedule.map(|(_,f)|f),
        "output_format":if float{"float32"}else{"pcm16"},"first_nonzero_pcm_frame":first,"first_nonzero_output_frame":first,
        "filter_tail_frames":tail,"clipped_samples":if float{0}else{overs},"samples_over_full_scale":overs,"peak_preclip_per_ear":peak,
        "speed_m_s":SOUND,"band_hz":BAND_HZ,"roomPath":route,"path_sound":response}),
    )
}
