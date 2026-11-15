// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The splash screens while the game starts (D-131): the window showed
//! black for seconds while the world was joined and the circuit built.
//! First osage — its mark and its name, as on the site — then osage kart,
//! with a spinner, until the circuit is there and its models have loaded;
//! then it fades away.
//!
//! Only in a game: the bench's pictures stay the same.

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::theme;

/// How long osage's own screen shows before the game's, s from the first
/// frame: the interface is drawn only once its shaders are made, two or
/// three seconds in on an integrated GPU, so osage's screen is seen for
/// about a second.
const ENGINE: f32 = 4.5;
/// The least time the splash shows, s from the first frame, and how long it
/// takes to fade.
const LEAST: f32 = 7.0;
const FADE: f32 = 0.6;
/// The mark's side, px, drawn at twice that.
const MARK: f32 = 112.0;

pub struct SplashPlugin;

impl Plugin for SplashPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(Update, show);
    }
}

#[derive(Component)]
struct Splash {
    /// When the splash was first drawn, s since the start: the window and
    /// the GPU take about a second to open, and osage's screen is counted
    /// from when it can be seen.
    shown: Option<f32>,
    /// When the game was ready, s since the start; none until then.
    ready: Option<f32>,
}

#[derive(Component)]
enum Line {
    Name,
    Under,
}

#[derive(Component)]
struct Turning;

/// The splash's own pictures: the only ones it fades.
#[derive(Component)]
struct Picture;

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mark = images.add(mark_image((MARK * 2.0) as u32));
    commands
        .spawn((
            Splash { shown: None, ready: None },
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(18),
                ..default()
            },
            BackgroundColor(theme::DIALOG),
            GlobalZIndex(100),
        ))
        .with_children(|splash| {
            splash.spawn((Picture, ImageNode::new(mark), Node { width: px(MARK), height: px(MARK), ..default() }));
            splash.spawn((Line::Name, theme::heavy("osage", 72.0, theme::TEXT)));
            splash.spawn((Line::Under, theme::heavy("an open-source engine for open worlds", 16.0, theme::MUTED)));
            splash.spawn((
                Turning,
                Picture,
                ImageNode::new(images.add(arc_image(64))),
                Node { width: px(32), height: px(32), margin: UiRect::top(px(18)), ..default() },
                Visibility::Hidden,
            ));
        });
}

/// osage's screen, then the game's, then away once the circuit is there.
#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    time: Res<Time>,
    course: Option<Res<crate::scene::Course>>,
    waiting: Option<Res<osage_props::Waiting>>,
    mut splash: Query<(Entity, &mut Splash, &mut BackgroundColor)>,
    mut lines: Query<(&Line, &mut Text, &mut TextColor)>,
    mut images: Query<&mut ImageNode, With<Picture>>,
    mut turning: Query<(&mut Visibility, &mut UiTransform), With<Turning>>,
) {
    let Ok((entity, mut state, mut background)) = splash.single_mut() else { return };
    let now = time.elapsed_secs();
    let shown = *state.shown.get_or_insert(now);
    let game = now - shown >= ENGINE;
    for (line, mut text, _) in &mut lines {
        let wanted = match (line, game) {
            (Line::Name, false) => "osage",
            (Line::Name, true) => "osage kart",
            (Line::Under, false) => "an open-source engine for open worlds",
            (Line::Under, true) => "building the circuit",
        };
        if text.0 != wanted {
            text.0 = wanted.to_owned();
        }
    }
    if let Ok((mut visibility, mut transform)) = turning.single_mut() {
        visibility.set_if_neq(if game { Visibility::Inherited } else { Visibility::Hidden });
        transform.rotation = Rot2::radians(now * 5.0);
    }
    let loaded = course.is_some() && waiting.is_none_or(|w| w.0 == 0);
    if state.ready.is_none() && loaded && now - shown >= LEAST {
        state.ready = Some(now);
    }
    let Some(ready) = state.ready else { return };
    let left = 1.0 - ((now - ready) / FADE).clamp(0.0, 1.0);
    if left <= 0.0 {
        commands.entity(entity).despawn();
        return;
    }
    background.0 = theme::DIALOG.with_alpha(left);
    for (_, _, mut colour) in &mut lines {
        colour.0 = colour.0.with_alpha(left);
    }
    for mut image in &mut images {
        image.color = image.color.with_alpha(left);
    }
}

/// osage's mark, as the site draws it: a rounded square, and in it an
/// amber stroke like a lower-case a.
fn mark_image(size: u32) -> Image {
    let s = size as f32 / 32.0;
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    // The stroke's path, in the site's 32-unit box: three curves and a line.
    let curves = [
        [Vec2::new(9.0, 21.5), Vec2::new(9.0, 15.5), Vec2::new(12.6, 10.5), Vec2::new(17.5, 10.5)],
        [Vec2::new(17.5, 10.5), Vec2::new(20.9, 10.5), Vec2::new(23.0, 12.7), Vec2::new(23.0, 15.7)],
        [Vec2::new(23.0, 15.7), Vec2::new(23.0, 19.3), Vec2::new(20.0, 21.5), Vec2::new(16.0, 21.5)],
        [Vec2::new(16.0, 21.5), Vec2::new(13.7, 21.5), Vec2::new(11.3, 21.5), Vec2::new(9.0, 21.5)],
    ];
    let path: Vec<Vec2> = curves
        .iter()
        .flat_map(|[a, b, c, d]| {
            (0..=48).map(move |i| {
                let t = i as f32 / 48.0;
                let u = 1.0 - t;
                *a * u * u * u + *b * 3.0 * u * u * t + *c * 3.0 * u * t * t + *d * t * t * t
            })
        })
        .collect();
    for y in 0..size {
        for x in 0..size {
            let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / s;
            // The square: 1.5 to 30.5, corners of radius 8, a line 3 wide.
            let q = (p - Vec2::splat(16.0)).abs() - Vec2::splat(14.5 - 8.0);
            let square = (q.max(Vec2::ZERO).length() + q.max_element().min(0.0) - 8.0).abs() - 1.5;
            let stroke = path.iter().map(|c| c.distance(p)).fold(f32::MAX, f32::min) - 1.5;
            let cover = |d: f32| (0.5 - d * s).clamp(0.0, 1.0);
            let (white, amber) = (cover(square), cover(stroke));
            let at = ((y * size + x) * 4) as usize;
            let rgba = if amber > 0.0 {
                [255, 184, 51, (amber * 255.0) as u8]
            } else {
                [242, 244, 248, (white * 255.0) as u8]
            };
            pixels[at..at + 4].copy_from_slice(&rgba);
        }
    }
    image(size, pixels)
}

/// An arc fading round to nothing: turned, it spins.
fn arc_image(size: u32) -> Image {
    let mut pixels = vec![0u8; (size * size * 4) as usize];
    let c = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let r = (dx * dx + dy * dy).sqrt() / c;
            let edge = (1.0 - ((r - 0.78).abs() / 0.12)).clamp(0.0, 1.0);
            let turn = (dy.atan2(dx) / std::f32::consts::TAU).rem_euclid(1.0);
            let at = ((y * size + x) * 4) as usize;
            pixels[at..at + 4].copy_from_slice(&[255, 184, 51, (edge * turn * 255.0) as u8]);
        }
    }
    image(size, pixels)
}

fn image(size: u32, pixels: Vec<u8>) -> Image {
    Image::new(
        Extent3d { width: size, height: size, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_has_its_square_and_its_stroke() {
        let image = mark_image(64);
        let data = image.data.unwrap();
        let amber = data.chunks(4).filter(|p| p[0] == 255 && p[1] == 184 && p[3] > 128).count();
        let white = data.chunks(4).filter(|p| p[0] == 242 && p[3] > 128).count();
        assert!(amber > 100 && white > 300, "amber {amber}, white {white}");
        // The middle of the square is empty but for the stroke.
        let at = |x: usize, y: usize| data[(y * 64 + x) * 4 + 3];
        assert_eq!(at(32, 8), 0, "inside the square, above the stroke");
    }
}
