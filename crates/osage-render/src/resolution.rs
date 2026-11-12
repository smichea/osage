// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: LicenseRef-osage-AGPL-3.0-or-later-with-exceptions

//! R11 — resolution and upscaling. The scene is not drawn straight into the
//! window: it is drawn into a picture whose size the tier decides, and the
//! picture is then stretched over the window, under the interface, which
//! stays sharp. A high-density screen no longer makes a modest GPU draw
//! five million pixels, and a tier can draw a little under its output size.
//!
//! - The picture is never taller than the tier allows (T1 720, T2 1080,
//!   T3 1440, T4 2160 lines).
//! - T2 draws at 85 % of that in each direction: on the Iris Xe reference
//!   the scene is bound by the number of pixels, and this is what brings a
//!   1080p output within 16.7 ms.
//! - Holding a frame rate ([`crate::pace`]) draws a share of that again.
//! - Stretching is bilinear for now; sharper upscalers are this
//!   workstream's next milestones.

use bevy::{
    camera::RenderTarget, prelude::*, render::render_resource::TextureFormat, ui::IsDefaultUiCamera,
    window::PrimaryWindow,
};

use crate::{LeftOut, OsageCamera, Technique, Tier, pace::Pace};

/// The size the picture is shown at: the window's (`None`), or a fixed one,
/// as the bench asks.
#[derive(Resource, Default, Clone, Copy)]
pub struct Output(pub Option<UVec2>);

/// The picture the scene is drawn into before it is shown.
#[derive(Resource)]
pub struct Picture {
    pub image: Handle<Image>,
    /// Its size, pixels: what is really rendered.
    pub size: UVec2,
}

pub struct ResolutionPlugin;

impl Plugin for ResolutionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Output>().add_systems(Startup, setup).add_systems(First, (resize, aim).chain());
    }
}

impl Tier {
    /// Lines of the tallest picture this tier draws.
    pub fn max_height(self) -> u32 {
        match self {
            Tier::T1 => 720,
            Tier::T2 => 1080,
            Tier::T3 => 1440,
            Tier::T4 => 2160,
        }
    }

    /// The share of the output size this tier draws, in each direction.
    pub fn render_scale(self) -> f32 {
        if self == Tier::T2 { 0.85 } else { 1.0 }
    }
}

fn target(size: UVec2) -> Image {
    Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8Unorm, Some(TextureFormat::Rgba8UnormSrgb))
}

/// The picture, and what shows it in the window: a camera for the
/// interface, and the picture stretched behind everything else.
fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let size = UVec2::new(16, 9);
    let image = images.add(target(size));
    commands.spawn((Camera2d, Camera { order: 1, ..default() }, IsDefaultUiCamera));
    commands.spawn((
        ImageNode::new(image.clone()),
        Node { position_type: PositionType::Absolute, width: percent(100), height: percent(100), ..default() },
        GlobalZIndex(-1),
    ));
    commands.insert_resource(Picture { image, size });
}

/// Keeps the picture at the size the tier wants for the current output.
fn resize(
    tier: Res<Tier>,
    left_out: Res<LeftOut>,
    output: Res<Output>,
    pace: Option<Res<Pace>>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut picture: ResMut<Picture>,
    mut images: ResMut<Assets<Image>>,
) {
    let shown = match (output.0, window.single()) {
        (Some(fixed), _) => fixed,
        (None, Ok(window)) => UVec2::new(window.physical_width(), window.physical_height()),
        (None, Err(_)) => return,
    };
    if shown.x == 0 || shown.y == 0 {
        return;
    }
    let scale = if left_out.0.contains(&Technique::Upscaling) { 1.0 } else { tier.render_scale() };
    // And what holding the frame rate gives up (pace).
    let scale = scale * pace.map_or(1.0, |pace| pace.rung().scale);
    // Never more lines than the tier draws, then the tier's share of that.
    let fit = (tier.max_height() as f32 / shown.y as f32).min(1.0) * scale;
    let wanted = (shown.as_vec2() * fit).round().as_uvec2().max(UVec2::splat(8));
    if wanted != picture.size {
        picture.size = wanted;
        if let Some(mut image) = images.get_mut(&picture.image) {
            *image = target(wanted);
        }
    }
}

/// Points the scene's camera at the picture.
fn aim(mut commands: Commands, picture: Res<Picture>, cameras: Query<Entity, Added<OsageCamera>>) {
    for camera in &cameras {
        commands.entity(camera).insert(RenderTarget::Image(picture.image.clone().into()));
    }
}
