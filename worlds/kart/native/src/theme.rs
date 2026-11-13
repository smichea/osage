// SPDX-FileCopyrightText: The osage contributors
// SPDX-License-Identifier: MIT OR Apache-2.0

//! The interface's look (D-078): its faces — Saira (SIL OFL 1.1), a
//! squared sans of racing dashboards, from the content: Bold, in place of
//! the engine's default, for figures and titles, Regular for text — its
//! colours, its panels, and a scale that follows the height of the picture
//! (the layout is drawn for 720 lines and grows with the screen).

use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    text::{Font, FontSize, FontSource},
    ui::UiScale,
};
use osage_render::resolution::Output;

/// Panels: deep blue-grey, see-through; their edge a lighter line.
pub const PANEL: Color = Color::srgba(0.05, 0.07, 0.11, 0.78);
/// A dialog's panel (the settings, a question): opaque, as blending is done
/// in linear light, where even 5 % of a white text behind reads as grey.
pub const DIALOG: Color = Color::srgb(0.045, 0.06, 0.095);
pub const EDGE: Color = Color::srgba(1.0, 1.0, 1.0, 0.08);
/// The accent: amber, as a kart's dashboard.
pub const ACCENT: Color = Color::srgb(1.0, 0.72, 0.2);
pub const TEXT: Color = Color::srgb(0.95, 0.96, 0.98);
pub const MUTED: Color = Color::srgba(0.8, 0.84, 0.9, 0.7);
/// Places: gold, silver, bronze.
pub const GOLD: Color = Color::srgb(1.0, 0.82, 0.25);
pub const SILVER: Color = Color::srgb(0.84, 0.88, 0.94);
pub const BRONZE: Color = Color::srgb(0.92, 0.58, 0.32);
/// Lines the layout is drawn for.
const DRAWN_FOR: f32 = 720.0;

/// The colour of a place.
pub fn place_colour(place: u32) -> Color {
    match place {
        1 => GOLD,
        2 => SILVER,
        3 => BRONZE,
        _ => TEXT,
    }
}

/// The interface's faces: a heavy one for figures and titles, a thin one
/// for text. Both are the engine's default face until the content's load.
#[derive(Resource, Clone, Default)]
pub struct Faces {
    pub thin: Handle<Font>,
}

/// The arrow keys' marks, drawn: the faces have no arrows. Up, right,
/// down, left.
#[derive(Resource, Clone, Default)]
pub struct Arrows(pub [Handle<Image>; 4]);

/// A key on a keycap: its name, or an arrow (0 up, 1 right, 2 down, 3 left).
#[derive(Clone, Copy)]
pub enum Key {
    Word(&'static str),
    Arrow(u8),
}

pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Faces>()
            .init_resource::<Arrows>()
            .add_systems(PreStartup, (faces, arrows))
            .add_systems(Update, scale);
    }
}

/// The content's faces, read at the start: the heavy one takes the place of
/// the engine's default, so that every text has it; the thin one is named.
fn faces(mut fonts: ResMut<Assets<Font>>, mut faces: ResMut<Faces>) {
    let Some(root) = osage_content::root() else { return };
    let read = |name: &str| {
        let bytes = std::fs::read(root.join("saira").join(name)).ok()?;
        Some(Font::from_bytes(bytes))
    };
    if let Some(heavy) = read("Saira-Bold.ttf") {
        // Before the first frame: the text system has not read the engine's yet.
        let _ = fonts.insert(AssetId::default(), heavy);
    }
    if let Some(thin) = read("Saira-Regular.ttf") {
        faces.thin = fonts.add(thin);
    }
}

fn arrows(mut images: ResMut<Assets<Image>>, mut arrows: ResMut<Arrows>) {
    arrows.0 = [0, 1, 2, 3].map(|way| images.add(arrow(way)));
}

/// A triangle pointing `way` (0 up, 1 right, 2 down, 3 left), white on clear: an arrow key's mark.
fn arrow(way: u8) -> Image {
    let size = 24usize;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            // Up: the tip at the top; turned for the others.
            let (u, v) = match way {
                0 => (x as f32, y as f32),
                1 => (y as f32, (size - 1 - x) as f32),
                2 => (x as f32, (size - 1 - y) as f32),
                _ => (y as f32, x as f32),
            };
            let half = (v - 4.0) * 0.55;
            let inside = (5.0..=19.0).contains(&v) && (u - 11.5).abs() <= half;
            if inside {
                pixels[(y * size + x) * 4..][..4].copy_from_slice(&[255, 255, 255, 255]);
            }
        }
    }
    Image::new(
        Extent3d { width: size as u32, height: size as u32, depth_or_array_layers: 1 },
        TextureDimension::D2,
        pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// The layout's scale: the picture's height over the height it is drawn for.
fn scale(output: Res<Output>, windows: Query<&Window>, mut ui: ResMut<UiScale>) {
    let height = match (output.0, windows.iter().next()) {
        (Some(size), _) => size.y as f32,
        (None, Some(window)) => window.height(),
        (None, None) => return,
    };
    let wanted = (height / DRAWN_FOR).clamp(0.75, 2.5);
    if (ui.0 - wanted).abs() > 0.01 {
        ui.0 = wanted;
    }
}

/// A panel's node: rounded, padded, with an edge.
pub fn panel(node: Node) -> (Node, BackgroundColor, BorderColor) {
    (
        Node {
            border_radius: BorderRadius::all(px(10)),
            padding: UiRect::axes(px(14), px(10)),
            border: UiRect::all(px(1)),
            ..node
        },
        BackgroundColor(PANEL),
        BorderColor::all(EDGE),
    )
}

/// A text in the heavy face, `size` pixels, in `colour`.
pub fn heavy(words: impl Into<String>, size: f32, colour: Color) -> impl Bundle {
    (Text::new(words), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(colour))
}

/// A text in the thin face, `size` pixels, in `colour`.
pub fn light(words: impl Into<String>, size: f32, colour: Color, faces: &Faces) -> impl Bundle {
    (
        Text::new(words),
        TextFont { font: FontSource::Handle(faces.thin.clone()), font_size: FontSize::Px(size), ..default() },
        TextColor(colour),
    )
}

/// A hint: the keys as keycaps, then what they do, muted. Its entity.
pub fn hint(parent: &mut ChildSpawnerCommands, keys: &[Key], action: &str, arrows: &Arrows) -> Entity {
    parent
        .spawn(Node { column_gap: px(6), align_items: AlignItems::Center, ..default() })
        .with_children(|hint| {
            for key in keys {
                hint.spawn((
                    Node {
                        min_width: px(24),
                        height: px(24),
                        padding: UiRect::horizontal(px(6)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(5)),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.1)),
                    BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.35)),
                ))
                .with_children(|cap| match *key {
                    Key::Word(word) => {
                        cap.spawn(heavy(word, 11.0, TEXT));
                    }
                    Key::Arrow(way) => {
                        cap.spawn((
                            ImageNode::new(arrows.0[way as usize % 4].clone()),
                            Node { width: px(11), height: px(11), ..default() },
                        ));
                    }
                });
            }
            hint.spawn(heavy(action, 12.0, MUTED));
        })
        .id()
}
